use super::sequence_store::SequenceStore;
use super::{Command, WorkerHandle};
use git_core::pick::{PickOutcome, SequenceAbort, SequenceKind, SequenceOutcome, SequenceRun};
use git_core::repo::HeadInfo;
use git_core::reset::ResetMode;
use std::sync::mpsc::Sender;

const NO_SEQUENCE: &str = "No cherry-pick or revert is in progress.";

/// `HeadInfo` with the stored sequence's progress filled in (git-core has no persistence).
pub(super) fn head_info(
    repo: &git2::Repository,
    sequence_store: &SequenceStore,
    reply: Sender<Result<HeadInfo, String>>,
) {
    let result = git_core::repo::head_info(repo)
        .map(|mut info| {
            info.sequence = sequence_store.load(repo).map(|state| state.progress());
            info
        })
        .map_err(|error| error.to_string());
    let _ = reply.send(result);
}

/// While a sequence is stored, commands that would start another operation or rewrite HEAD
/// (`CherryPick`, `RevertCommit`, `CherryPickMany`, `RevertMany`, `StartMerge`, `StartRebase`,
/// `AmendCommit`) are refused with git-core's `OperationInProgress` text, before git-core runs.
/// Returns `true` when it replied.
pub(super) fn refused_during_sequence<T>(
    repo: &git2::Repository,
    sequence_store: &SequenceStore,
    reply: &Sender<Result<T, String>>,
) -> bool {
    if sequence_store.load(repo).is_none() {
        return false;
    }
    let _ = reply.send(Err(
        git_core::pick::PickError::OperationInProgress.to_string()
    ));
    true
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
    reply: Sender<Result<SequenceOutcome, String>>,
) {
    let run = match kind {
        SequenceKind::CherryPick => git_core::pick::cherry_pick_many(repo, &commit_ids),
        SequenceKind::Revert => git_core::pick::revert_many(repo, &commit_ids),
    };
    let _ = reply.send(persist(sequence_store, run));
}

pub(super) fn continue_sequence(
    repo: &git2::Repository,
    sequence_store: &SequenceStore,
    reply: Sender<Result<SequenceOutcome, String>>,
) {
    let Some(state) = sequence_store.load(repo) else {
        let _ = reply.send(Err(NO_SEQUENCE.to_string()));
        return;
    };
    let run = git_core::pick::continue_sequence(repo, state);
    let _ = reply.send(persist(sequence_store, run));
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
pub(super) fn amend(
    repo: &git2::Repository,
    message: String,
    no_verify: bool,
    op: super::remote::TransferOp<'_>,
    events: Sender<super::HookEvent>,
    reply: Sender<Result<git_core::commit::CommitOutcome, String>>,
) {
    let cancel = op.cancel_check();
    let mut reporter = super::hooks::ChannelHookReporter {
        events,
        operation_id: op.id.clone(),
    };
    let result = git_core::commit::amend_head(
        repo,
        &message,
        git_core::hooks::HookOptions {
            no_verify,
            cancel: Some(&cancel),
            reporter: Some(&mut reporter),
            ..Default::default()
        },
    );
    op.clear();
    let _ = reply.send(super::status::commit_reply(result));
}
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
        events: Sender<super::HookEvent>,
    ) -> Result<git_core::commit::CommitOutcome, String> {
        let operation_id = super::remote::next_operation_id("amend");
        round_trip(self, |reply| Command::AmendCommit {
            message,
            no_verify,
            operation_id,
            events,
            reply,
        })
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
    pub fn continue_sequence(&self) -> Result<SequenceOutcome, String> {
        round_trip(self, |reply| Command::ContinueSequence { reply })
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
