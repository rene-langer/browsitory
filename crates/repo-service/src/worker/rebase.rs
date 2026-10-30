use super::gate::RebaseMemory;
use super::rebase_store::RebaseStore;
use super::{Command, WorkerHandle};
use git_core::rebase::{
    RebaseAbort, RebasePlanCommit, RebasePlanEntry, RebaseState, RebaseStepResult,
};
use std::sync::mpsc::Sender;

pub(super) fn commits_since(
    repo: &git2::Repository,
    onto: String,
    reply: Sender<Result<Vec<RebasePlanCommit>, String>>,
) {
    let _ =
        reply.send(git_core::rebase::commits_since(repo, &onto).map_err(|error| error.to_string()));
}

const NO_REBASE: &str = "no rebase is currently in progress";

/// Where the paused rebase lives: `rebase.json`, plus what this session remembers when the file
/// can't be written (`gate::RebaseMemory`): a newer state whose save failed (so the user isn't
/// stranded mid-rebase on a full disk or a read-only `.git/browsitory`), or a state the rebase
/// ended with but whose file couldn't be deleted (so it doesn't keep gating the repository).
pub(super) struct RebaseSlot<'a> {
    pub store: &'a RebaseStore,
    pub memory: &'a mut RebaseMemory,
}

impl RebaseSlot<'_> {
    pub fn load(&self, repo: &git2::Repository) -> Option<RebaseState> {
        super::gate::stored_rebase(repo, self.store, self.memory)
    }

    /// Forgets the rebase. When its file can't be deleted, whatever it still holds is
    /// remembered as ended for this session.
    fn forget(&mut self, repo: &git2::Repository) -> Result<(), String> {
        self.memory.fallback = None;
        match self.store.clear() {
            Ok(()) => {
                self.memory.forgotten = None;
                Ok(())
            }
            Err(error) => {
                eprintln!("browsitory: could not remove the ended rebase's state: {error}");
                self.memory.forgotten = self.store.load(repo);
                Err(error)
            }
        }
    }

    /// Keeps `state` (or forgets the rebase when it's `None`) and turns `outcome` into the
    /// reply. A state that can't be saved is kept for this session and reported with the
    /// outcome, since a restart would not find it.
    fn persist(
        &mut self,
        repo: &git2::Repository,
        state: Option<&RebaseState>,
        outcome: Result<RebaseStepResult, String>,
    ) -> Result<RebaseStepResult, String> {
        let Some(state) = state else {
            // The rebase is over: a file that can't be deleted is remembered as ended.
            let _ = self.forget(repo);
            return outcome;
        };
        let Err(store_error) = self.store.save(state) else {
            *self.memory = RebaseMemory::default();
            return outcome;
        };
        eprintln!("browsitory: could not update the rebase state: {store_error}");
        self.memory.fallback = Some(state.clone());
        match outcome {
            Ok(_) => Err(format!(
                "The rebase stopped, but its progress could not be saved ({store_error}). Abort \
                 it now, or finish it with git: resolve, `git cherry-pick --continue`, then \
                 `git switch {}`.",
                state
                    .original_branch_ref
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&state.original_branch_ref)
            )),
            Err(error) => Err(format!(
                "{error} (its progress could not be saved: {store_error})"
            )),
        }
    }
}

pub(super) fn start(
    repo: &git2::Repository,
    onto: &str,
    plan: Vec<RebasePlanEntry>,
    mut slot: RebaseSlot<'_>,
) -> Result<RebaseStepResult, String> {
    match git_core::rebase::start_rebase(repo, onto, plan) {
        // Nothing to keep: the rebase finished in one go.
        Ok((_, RebaseStepResult::Done)) => slot.persist(repo, None, Ok(RebaseStepResult::Done)),
        Ok((state, step)) => slot.persist(repo, Some(&state), Ok(step)),
        // `start_rebase` rolled back: there is no rebase to keep.
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn continue_rebase(
    repo: &git2::Repository,
    mut slot: RebaseSlot<'_>,
    reply: Sender<Result<RebaseStepResult, String>>,
) {
    let result = match slot.load(repo) {
        Some(loaded) => {
            let mut state = loaded.clone();
            let outcome = git_core::rebase::rebase_continue(repo, &mut state)
                .map_err(|error| error.to_string());
            if matches!(outcome, Ok(RebaseStepResult::Done)) {
                slot.persist(repo, None, outcome)
            } else if state == loaded || (outcome.is_err() && lost_head(repo, &state)) {
                // Refused before anything changed, or HEAD was moved by another writer while
                // this one was landing a step (its commit lost HEAD's compare-and-swap): write
                // nothing, so this copy never replaces a state the other app has moved on from.
                outcome
            } else {
                slot.persist(repo, Some(&state), outcome)
            }
        }
        None => Err(NO_REBASE.to_string()),
    };
    let _ = reply.send(result);
}

/// After a failed Continue, HEAD is not where this worker's copy of the rebase expects it: every
/// error of `rebase_continue` leaves HEAD at `expected_head`, unless something else moved it
/// meanwhile.
fn lost_head(repo: &git2::Repository, state: &RebaseState) -> bool {
    repo.head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .is_none_or(|head| head.id() != state.expected_head)
}

/// Forgets the rebase only when Abort succeeded: a failed Abort (a refused checkout) must stay
/// retryable.
pub(super) fn abort(
    repo: &git2::Repository,
    mut slot: RebaseSlot<'_>,
    reply: Sender<Result<RebaseAbort, String>>,
) {
    let result = (|| {
        let state = slot.load(repo).ok_or(NO_REBASE)?;
        let abort =
            git_core::rebase::abort_rebase(repo, &state).map_err(|error| error.to_string())?;
        let _ = slot.forget(repo);
        Ok(abort)
    })();
    let _ = reply.send(result);
}

impl WorkerHandle {
    #[allow(dead_code)]
    pub fn commits_since(&self, onto: String) -> Result<Vec<RebasePlanCommit>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::CommitsSince { onto, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn start_rebase(
        &self,
        onto: String,
        plan: Vec<RebasePlanEntry>,
    ) -> Result<RebaseStepResult, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::StartRebase {
                onto,
                plan,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn rebase_continue(&self) -> Result<RebaseStepResult, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::RebaseContinue { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn abort_rebase(&self) -> Result<RebaseAbort, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::AbortRebase { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
