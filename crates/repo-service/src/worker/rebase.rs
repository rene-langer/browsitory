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

/// Where the paused rebase lives: `rebase.json`, plus this session's copy when saving it
/// failed (a full disk, a read-only `.git/browsitory`), so the user isn't stranded mid-rebase.
/// The copy is newer than whatever the file still holds, so it wins until the next successful
/// save or clear drops it.
pub(super) struct RebaseSlot<'a> {
    pub store: &'a RebaseStore,
    pub fallback: &'a mut Option<RebaseState>,
}

impl RebaseSlot<'_> {
    pub fn load(&self, repo: &git2::Repository) -> Option<RebaseState> {
        self.fallback.clone().or_else(|| self.store.load(repo))
    }

    /// Keeps `state` (or forgets the rebase when it's `None`) and turns `outcome` into the
    /// reply. A state that can't be saved is kept for this session and reported with the
    /// outcome, since a restart would not find it.
    fn persist(
        &mut self,
        state: Option<&RebaseState>,
        outcome: Result<RebaseStepResult, String>,
    ) -> Result<RebaseStepResult, String> {
        let saved = match state {
            Some(state) => self.store.save(state),
            None => self.store.clear(),
        };
        let Err(store_error) = saved else {
            *self.fallback = None;
            return outcome;
        };
        eprintln!("browsitory: could not update the rebase state: {store_error}");
        let Some(state) = state else {
            // The rebase is over; a leftover file only offers Abort, which leaves HEAD alone.
            *self.fallback = None;
            return outcome;
        };
        *self.fallback = Some(state.clone());
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
    onto: String,
    plan: Vec<RebasePlanEntry>,
    mut slot: RebaseSlot<'_>,
    reply: Sender<Result<RebaseStepResult, String>>,
) {
    let result = match git_core::rebase::start_rebase(repo, &onto, plan) {
        // Nothing to keep: the rebase finished in one go.
        Ok((_, RebaseStepResult::Done)) => slot.persist(None, Ok(RebaseStepResult::Done)),
        Ok((state, step)) => slot.persist(Some(&state), Ok(step)),
        // `start_rebase` rolled back: there is no rebase to keep.
        Err(error) => Err(error.to_string()),
    };
    let _ = reply.send(result);
}

pub(super) fn continue_rebase(
    repo: &git2::Repository,
    mut slot: RebaseSlot<'_>,
    reply: Sender<Result<RebaseStepResult, String>>,
) {
    let result = match slot.load(repo) {
        Some(mut state) => {
            let outcome = git_core::rebase::rebase_continue(repo, &mut state)
                .map_err(|error| error.to_string());
            let keep = !matches!(outcome, Ok(RebaseStepResult::Done));
            slot.persist(keep.then_some(&state), outcome)
        }
        None => Err(NO_REBASE.to_string()),
    };
    let _ = reply.send(result);
}

/// Forgets the rebase only when Abort succeeded: a failed Abort (a refused checkout) must stay
/// retryable.
pub(super) fn abort(
    repo: &git2::Repository,
    slot: RebaseSlot<'_>,
    reply: Sender<Result<RebaseAbort, String>>,
) {
    let result = (|| {
        let state = slot.load(repo).ok_or(NO_REBASE)?;
        let abort =
            git_core::rebase::abort_rebase(repo, &state).map_err(|error| error.to_string())?;
        *slot.fallback = None;
        if let Err(error) = slot.store.clear() {
            eprintln!("browsitory: could not remove the aborted rebase's state: {error}");
        }
        Ok(abort)
    })();
    let _ = reply.send(result);
}

pub(super) fn progress(
    repo: &git2::Repository,
    slot: RebaseSlot<'_>,
    reply: Sender<Result<Option<(usize, usize)>, String>>,
) {
    let progress = slot
        .load(repo)
        .map(|state| (state.current_step(), state.total_steps()));
    let _ = reply.send(Ok(progress));
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

    #[allow(dead_code)]
    pub fn get_rebase_progress(&self) -> Result<Option<(usize, usize)>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetRebaseProgress { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
