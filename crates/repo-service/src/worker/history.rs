use super::{Command, WorkerHandle};
use git_core::pick::PickOutcome;
use git_core::repo::HeadInfo;
use git_core::reset::ResetMode;
use std::sync::mpsc::Sender;

pub(super) fn head_info(repo: &git2::Repository, reply: Sender<Result<HeadInfo, String>>) {
    let _ = reply.send(git_core::repo::head_info(repo).map_err(|error| error.to_string()));
}
pub(super) fn amend(repo: &git2::Repository, message: String, reply: Sender<Result<(), String>>) {
    let _ = reply.send(
        git_core::commit::amend_head(repo, &message, git_core::hooks::HookOptions::default())
            .map(|_| ())
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn cherry_pick(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<PickOutcome, String>>,
) {
    let _ = reply
        .send(git_core::pick::cherry_pick(repo, &commit_id).map_err(|error| error.to_string()));
}
pub(super) fn revert(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<PickOutcome, String>>,
) {
    let _ = reply.send(git_core::pick::revert(repo, &commit_id).map_err(|error| error.to_string()));
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
    let _ = reply.send(
        git_core::branch::checkout_commit(repo, &commit_id).map_err(|error| error.to_string()),
    );
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
    pub fn amend_commit(&self, message: String) -> Result<(), String> {
        round_trip(self, |reply| Command::AmendCommit { message, reply })
    }
    pub fn cherry_pick(&self, commit_id: String) -> Result<PickOutcome, String> {
        round_trip(self, |reply| Command::CherryPick { commit_id, reply })
    }
    pub fn revert_commit(&self, commit_id: String) -> Result<PickOutcome, String> {
        round_trip(self, |reply| Command::RevertCommit { commit_id, reply })
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
