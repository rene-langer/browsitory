use std::sync::mpsc::Sender;

use git_core::stash::{StashApplyOutcome, StashEntry, StashFiles, StashOptions};

use super::{Command, WorkerHandle};

pub(super) fn list(repo: &mut git2::Repository, reply: Sender<Result<Vec<StashEntry>, String>>) {
    let result = git_core::stash::list_stashes(repo).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn save(
    repo: &mut git2::Repository,
    options: StashOptions,
    reply: Sender<Result<(), String>>,
) {
    let result = git_core::stash::save_stash(repo, &options).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn apply(
    repo: &mut git2::Repository,
    index: usize,
    restore_index: bool,
    reply: Sender<Result<StashApplyOutcome, String>>,
) {
    let result =
        git_core::stash::apply_stash(repo, index, restore_index).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn pop(
    repo: &mut git2::Repository,
    index: usize,
    restore_index: bool,
    reply: Sender<Result<StashApplyOutcome, String>>,
) {
    let result =
        git_core::stash::pop_stash(repo, index, restore_index).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn files(
    repo: &mut git2::Repository,
    commit_id: String,
    reply: Sender<Result<StashFiles, String>>,
) {
    let result = git_core::stash::stash_files(repo, &commit_id).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn drop(repo: &mut git2::Repository, index: usize, reply: Sender<Result<(), String>>) {
    let result = git_core::stash::drop_stash(repo, index).map_err(|error| error.to_string());
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
    pub fn list_stashes(&self) -> Result<Vec<StashEntry>, String> {
        round_trip(self, |reply| Command::ListStashes { reply })
    }

    pub fn save_stash(&self, options: StashOptions) -> Result<(), String> {
        round_trip(self, |reply| Command::SaveStash { options, reply })
    }

    /// Applies `stash@{index}`; `Conflicted` (not an error) when it applied with conflicts.
    pub fn apply_stash(
        &self,
        index: usize,
        restore_index: bool,
    ) -> Result<StashApplyOutcome, String> {
        round_trip(self, |reply| Command::ApplyStash {
            index,
            restore_index,
            reply,
        })
    }

    /// Applies `stash@{index}` and drops it only when it applied cleanly.
    pub fn pop_stash(
        &self,
        index: usize,
        restore_index: bool,
    ) -> Result<StashApplyOutcome, String> {
        round_trip(self, |reply| Command::PopStash {
            index,
            restore_index,
            reply,
        })
    }

    pub fn get_stash_files(&self, commit_id: String) -> Result<StashFiles, String> {
        round_trip(self, |reply| Command::GetStashFiles { commit_id, reply })
    }

    pub fn drop_stash(&self, index: usize) -> Result<(), String> {
        round_trip(self, |reply| Command::DropStash { index, reply })
    }
}
