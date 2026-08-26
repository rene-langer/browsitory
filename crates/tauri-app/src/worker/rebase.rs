use super::{Command, WorkerHandle};
use git_core::rebase::{RebasePlanCommit, RebasePlanEntry, RebaseState, RebaseStepResult};
use std::sync::mpsc::Sender;

pub(super) fn commits_since(
    repo: &git2::Repository,
    onto: String,
    reply: Sender<Result<Vec<RebasePlanCommit>, String>>,
) {
    let _ =
        reply.send(git_core::rebase::commits_since(repo, &onto).map_err(|error| error.to_string()));
}

pub(super) fn start(
    repo: &git2::Repository,
    onto: String,
    plan: Vec<RebasePlanEntry>,
    rebase_state: &mut Option<RebaseState>,
    reply: Sender<Result<RebaseStepResult, String>>,
) {
    let result = git_core::rebase::start_rebase(repo, &onto, plan)
        .map_err(|error| error.to_string())
        .map(|(state, step)| {
            if !matches!(step, RebaseStepResult::Done) {
                *rebase_state = Some(state);
            }
            step
        });
    let _ = reply.send(result);
}

pub(super) fn continue_rebase(
    repo: &git2::Repository,
    rebase_state: &mut Option<RebaseState>,
    reply: Sender<Result<RebaseStepResult, String>>,
) {
    let result = match rebase_state.as_mut() {
        Some(state) => {
            git_core::rebase::rebase_continue(repo, state).map_err(|error| error.to_string())
        }
        None => Err("no rebase is currently in progress".to_string()),
    };
    if matches!(result, Ok(RebaseStepResult::Done)) {
        *rebase_state = None;
    }
    let _ = reply.send(result);
}

pub(super) fn abort(
    repo: &git2::Repository,
    rebase_state: &mut Option<RebaseState>,
    reply: Sender<Result<(), String>>,
) {
    let result = match rebase_state.take() {
        Some(state) => {
            git_core::rebase::abort_rebase(repo, state).map_err(|error| error.to_string())
        }
        None => Err("no rebase is currently in progress".to_string()),
    };
    let _ = reply.send(result);
}

pub(super) fn progress(
    rebase_state: &Option<RebaseState>,
    reply: Sender<Result<Option<(usize, usize)>, String>>,
) {
    let progress = rebase_state
        .as_ref()
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
    pub fn abort_rebase(&self) -> Result<(), String> {
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
