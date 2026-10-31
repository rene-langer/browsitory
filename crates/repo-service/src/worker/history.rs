use super::sequence_store::SequenceStore;
use super::{Command, WorkerHandle};
use git_core::pick::{PickOutcome, SequenceAbort, SequenceKind, SequenceOutcome, SequenceRun};
use git_core::repo::HeadInfo;
use git_core::reset::ResetMode;
use std::sync::mpsc::Sender;

const NO_SEQUENCE: &str = "No cherry-pick or revert is in progress.";

/// `HeadInfo` with the stored sequence's progress and the stored rebase filled in (git-core has
/// no persistence). A rebase whose health can't be read counts as diverged: Abort is then the
/// safe way out. `clean_merge`: the staged merge is the clean one this worker remembers, so its
/// commit runs `pre-merge-commit` rather than `pre-commit`; `commit_hooks` then lists only the
/// pre-hook that commit will really run.
pub(super) fn head_info(
    repo: &git2::Repository,
    probe: &super::gate::OperationProbe<'_>,
    clean_merge: bool,
    reply: Sender<Result<HeadInfo, String>>,
) {
    let result = git_core::repo::head_info(repo)
        .map(|mut info| {
            if repo.state() == git2::RepositoryState::Merge {
                let skipped = if clean_merge {
                    "pre-commit"
                } else {
                    "pre-merge-commit"
                };
                info.commit_hooks.retain(|hook| hook != skipped);
            }
            info.sequence = probe.sequence.load(repo).map(|state| state.progress());
            info.rebase = probe.rebase(repo).map(|state| {
                let health =
                    git_core::rebase::rebase_health(repo, &state).unwrap_or_else(|error| {
                        eprintln!("browsitory: can't check the paused rebase: {error}");
                        git_core::rebase::RebaseHealth::Diverged(
                            git_core::rebase::RebaseDivergence::OtherOperation,
                        )
                    });
                git_core::repo::RebaseInfo::new(&state, health)
            });
            info
        })
        .map_err(|error| error.to_string());
    let _ = reply.send(result);
}

/// Keeps or forgets the sequence as `run.state` says and turns the run into the reply. A state
/// that can't be saved is reported with the outcome, since Continue/Abort would not find it.
fn persist(sequence_store: &SequenceStore, run: SequenceRun) -> Result<SequenceOutcome, String> {
    let keep = run.state.is_some();
    let saved = match &run.state {
        Some(state) => sequence_store.save(state),
        None => sequence_store.clear(),
    };
    let Err(store_error) = saved else {
        return run.outcome.map_err(|error| error.to_string());
    };
    eprintln!("browsitory: could not update the sequence state: {store_error}");
    match run.outcome {
        // Nothing to continue: the sequence is over, a leftover file only offers Abort.
        Ok(outcome) if !keep => Ok(outcome),
        Ok(SequenceOutcome::HookFailed { hook, .. }) => Err(format!(
            "The {hook} hook failed, and the cherry-pick or revert's progress could not be saved ({store_error}). Commit the paused step, or abort it."
        )),
        Ok(_) => Err(format!(
            "The cherry-pick or revert stopped on a conflict, but its progress could not be saved ({store_error}). Resolve the conflict and commit, or abort it."
        )),
        Err(error) if keep => Err(format!(
            "{error} (its progress could not be saved: {store_error})"
        )),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn pick_many(
    repo: &git2::Repository,
    sequence_store: &SequenceStore,
    kind: SequenceKind,
    commit_ids: Vec<String>,
) -> Result<SequenceOutcome, String> {
    let run = match kind {
        SequenceKind::CherryPick => git_core::pick::cherry_pick_many(repo, &commit_ids),
        SequenceKind::Revert => git_core::pick::revert_many(repo, &commit_ids),
    };
    persist(sequence_store, run)
}

/// The stored sequence's kind (`None` when there is none) and the continue result. The step it
/// commits runs the commit hooks, reported on `events` and cancellable through `op` (#133); a
/// failing hook keeps the stored sequence as it was.
pub(super) fn continue_sequence(
    repo: &git2::Repository,
    sequence_store: &SequenceStore,
    no_verify: bool,
    op: super::remote::TransferOp<'_>,
    events: Sender<super::HookEvent>,
) -> (Option<SequenceKind>, Result<SequenceOutcome, String>) {
    let Some(state) = sequence_store.load(repo) else {
        op.clear();
        return (None, Err(NO_SEQUENCE.to_string()));
    };
    let kind = state.kind;
    let cancel = op.cancel_check();
    let mut reporter = super::hooks::ChannelHookReporter {
        events,
        operation_id: op.id.clone(),
    };
    let run = git_core::pick::continue_sequence(
        repo,
        state,
        git_core::hooks::HookOptions {
            no_verify,
            cancel: Some(&cancel),
            reporter: Some(&mut reporter),
            ..Default::default()
        },
    );
    op.clear();
    (Some(kind), persist(sequence_store, run))
}

fn abort_stored(
    repo: &git2::Repository,
    sequence_store: &SequenceStore,
) -> Result<SequenceAbort, String> {
    let state = sequence_store.load(repo).ok_or(NO_SEQUENCE)?;
    let abort = git_core::pick::abort_sequence(repo, &state).map_err(|error| error.to_string())?;
    sequence_store.clear()?;
    Ok(abort)
}

pub(super) fn abort_sequence(
    repo: &git2::Repository,
    sequence_store: &SequenceStore,
    reply: Sender<Result<SequenceAbort, String>>,
) {
    let _ = reply.send(abort_stored(repo, sequence_store));
}

/// `AbortMerge` while a sequence is stored: abort the whole sequence.
pub(super) fn abort_sequence_for_merge(
    repo: &git2::Repository,
    sequence_store: &SequenceStore,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(abort_stored(repo, sequence_store).map(|_| ()));
}
#[allow(clippy::too_many_arguments)]
pub(super) fn amend(
    repo: &git2::Repository,
    message: String,
    no_verify: bool,
    author: Option<git_core::commit::SignatureInput>,
    trailers: Vec<git_core::commit::Trailer>,
    op: super::remote::TransferOp<'_>,
    events: Sender<super::HookEvent>,
    reply: Sender<Result<git_core::commit::CommitOutcome, String>>,
) {
    let cancel = op.cancel_check();
    let mut reporter = super::hooks::ChannelHookReporter {
        events,
        operation_id: op.id.clone(),
    };
    // Trailers before the hooks, as for a commit.
    let result = git_core::commit::append_trailers(&message, &trailers).and_then(|message| {
        git_core::commit::amend_head(
            repo,
            &message,
            author.as_ref(),
            git_core::hooks::HookOptions {
                no_verify,
                cancel: Some(&cancel),
                reporter: Some(&mut reporter),
                ..Default::default()
            },
        )
    });
    op.clear();
    let _ = reply.send(super::status::commit_reply(result));
}
/// The commit box's co-author suggestions.
pub(super) fn recent_authors(
    repo: &git2::Repository,
    reply: Sender<Result<Vec<git_core::commit::SignatureInput>, String>>,
) {
    let _ = reply.send(
        git_core::commit::recent_authors(repo, RECENT_AUTHORS_LIMIT).map_err(|e| e.to_string()),
    );
}

/// How many co-author suggestions the commit box offers.
pub const RECENT_AUTHORS_LIMIT: usize = 20;

pub(super) fn cherry_pick(
    repo: &git2::Repository,
    commit_id: String,
    mainline: Option<u32>,
    reply: Sender<Result<PickOutcome, String>>,
) {
    let _ = reply.send(
        git_core::pick::cherry_pick(repo, &commit_id, mainline).map_err(|error| error.to_string()),
    );
}
pub(super) fn revert(
    repo: &git2::Repository,
    commit_id: String,
    mainline: Option<u32>,
    reply: Sender<Result<PickOutcome, String>>,
) {
    let _ = reply.send(
        git_core::pick::revert(repo, &commit_id, mainline).map_err(|error| error.to_string()),
    );
}
pub(super) fn reset(
    repo: &git2::Repository,
    commit_id: String,
    mode: ResetMode,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply
        .send(git_core::reset::reset_to(repo, &commit_id, mode).map_err(|error| error.to_string()));
}
pub(super) fn checkout(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<(), String>>,
) {
    let before = super::hooks::head_oid(repo);
    let result =
        git_core::branch::checkout_commit(repo, &commit_id).map_err(|error| error.to_string());
    if result.is_ok() {
        super::hooks::run_post_checkout(repo, before);
    }
    let _ = reply.send(result);
}

fn round_trip<T>(
    handle: &WorkerHandle,
    build: impl FnOnce(Sender<Result<T, String>>) -> Command,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    handle
        .tx
        .send(build(tx))
        .map_err(|_| "worker thread stopped".to_string())?;
    rx.recv()
        .map_err(|_| "worker thread stopped before replying".to_string())?
}

impl WorkerHandle {
    pub fn get_head_info(&self) -> Result<HeadInfo, String> {
        round_trip(self, |reply| Command::GetHeadInfo { reply })
    }
    /// Amends HEAD; hooks, events and cancel as for `commit`.
    pub fn amend_commit(
        &self,
        message: String,
        no_verify: bool,
        author: Option<git_core::commit::SignatureInput>,
        trailers: Vec<git_core::commit::Trailer>,
        events: Sender<super::HookEvent>,
    ) -> Result<git_core::commit::CommitOutcome, String> {
        let operation_id = super::remote::next_operation_id("amend");
        round_trip(self, |reply| Command::AmendCommit {
            message,
            no_verify,
            author,
            trailers,
            operation_id,
            events,
            reply,
        })
    }
    /// Up to `RECENT_AUTHORS_LIMIT` recent authors other than the current user, newest first.
    pub fn list_recent_authors(&self) -> Result<Vec<git_core::commit::SignatureInput>, String> {
        round_trip(self, |reply| Command::ListRecentAuthors { reply })
    }
    /// `mainline` (1-based parent) is required for a merge commit and refused otherwise.
    pub fn cherry_pick(
        &self,
        commit_id: String,
        mainline: Option<u32>,
    ) -> Result<PickOutcome, String> {
        round_trip(self, |reply| Command::CherryPick {
            commit_id,
            mainline,
            reply,
        })
    }
    pub fn revert_commit(
        &self,
        commit_id: String,
        mainline: Option<u32>,
    ) -> Result<PickOutcome, String> {
        round_trip(self, |reply| Command::RevertCommit {
            commit_id,
            mainline,
            reply,
        })
    }
    /// Cherry-picks `commit_ids` (oldest-first) as one sequence; a conflict pauses it, stored
    /// in `<gitdir>/browsitory/sequence.json` until `continue_sequence`/`abort_sequence`.
    pub fn cherry_pick_many(&self, commit_ids: Vec<String>) -> Result<SequenceOutcome, String> {
        round_trip(self, |reply| Command::CherryPickMany { commit_ids, reply })
    }
    /// Reverts `commit_ids` (oldest-first; applied newest-first) as one sequence.
    pub fn revert_many(&self, commit_ids: Vec<String>) -> Result<SequenceOutcome, String> {
        round_trip(self, |reply| Command::RevertMany { commit_ids, reply })
    }
    /// Continues the stored sequence. The step it commits runs the commit hooks (skipped with
    /// `no_verify`), reported on `events` under a `continue-<n>` id that `cancel_transfer`
    /// stops (#133).
    pub fn continue_sequence(
        &self,
        no_verify: bool,
        events: Sender<super::HookEvent>,
    ) -> Result<SequenceOutcome, String> {
        round_trip(self, |reply| Command::ContinueSequence {
            no_verify,
            operation_id: super::remote::next_operation_id("continue"),
            events,
            reply,
        })
    }
    pub fn abort_sequence(&self) -> Result<SequenceAbort, String> {
        round_trip(self, |reply| Command::AbortSequence { reply })
    }
    pub fn reset_to_commit(&self, commit_id: String, mode: ResetMode) -> Result<(), String> {
        round_trip(self, |reply| Command::ResetToCommit {
            commit_id,
            mode,
            reply,
        })
    }
    pub fn checkout_commit(&self, commit_id: String) -> Result<(), String> {
        round_trip(self, |reply| Command::CheckoutCommit { commit_id, reply })
    }
}
