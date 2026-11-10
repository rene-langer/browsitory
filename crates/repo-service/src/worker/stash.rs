use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;

use git_core::stash::{StashApplyOutcome, StashEntry, StashFiles, StashOptions};

use super::{Command, WorkerHandle};

pub(super) fn list(repo: &mut git2::Repository, reply: Sender<Result<Vec<StashEntry>, ErrorDto>>) {
    let result = git_core::stash::list_stashes(repo).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn save(
    repo: &mut git2::Repository,
    options: StashOptions,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let result = git_core::stash::save_stash(repo, &options).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn apply(
    repo: &mut git2::Repository,
    index: usize,
    commit_id: &str,
    restore_index: bool,
    reply: Sender<Result<StashApplyOutcome, ErrorDto>>,
) {
    let result = git_core::stash::apply_stash(repo, index, Some(commit_id), restore_index)
        .map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn pop(
    repo: &mut git2::Repository,
    index: usize,
    commit_id: &str,
    restore_index: bool,
    reply: Sender<Result<StashApplyOutcome, ErrorDto>>,
) {
    let result = git_core::stash::pop_stash(repo, index, Some(commit_id), restore_index)
        .map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn files(
    repo: &mut git2::Repository,
    commit_id: String,
    reply: Sender<Result<StashFiles, ErrorDto>>,
) {
    let result = git_core::stash::stash_files(repo, &commit_id).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn drop(
    repo: &mut git2::Repository,
    index: usize,
    commit_id: &str,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let result =
        git_core::stash::drop_stash(repo, index, Some(commit_id)).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn list_stashes(&self) -> Result<Vec<StashEntry>, ErrorDto> {
        self.request(|reply| Command::ListStashes { reply })
    }

    pub fn save_stash(&self, options: StashOptions) -> Result<(), ErrorDto> {
        self.request(|reply| Command::SaveStash { options, reply })
    }

    /// Applies `stash@{index}`, which must still be the stash `commit_id` (else "The stash list
    /// changed"); `Conflicted` (not an error) when it applied with conflicts.
    pub fn apply_stash(
        &self,
        index: usize,
        commit_id: String,
        restore_index: bool,
    ) -> Result<StashApplyOutcome, ErrorDto> {
        self.request(|reply| Command::ApplyStash {
            index,
            commit_id,
            restore_index,
            reply,
        })
    }

    /// Applies `stash@{index}` and drops it only when it applied cleanly.
    pub fn pop_stash(
        &self,
        index: usize,
        commit_id: String,
        restore_index: bool,
    ) -> Result<StashApplyOutcome, ErrorDto> {
        self.request(|reply| Command::PopStash {
            index,
            commit_id,
            restore_index,
            reply,
        })
    }

    pub fn get_stash_files(&self, commit_id: String) -> Result<StashFiles, ErrorDto> {
        self.request(|reply| Command::GetStashFiles { commit_id, reply })
    }

    /// Drops `stash@{index}` if it is still the stash `commit_id`.
    pub fn drop_stash(&self, index: usize, commit_id: String) -> Result<(), ErrorDto> {
        self.request(|reply| Command::DropStash {
            index,
            commit_id,
            reply,
        })
    }
}
