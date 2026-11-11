use crate::wire::ErrorDto;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use git_core::worktree::WorktreeInfo;

use super::{Command, ReadHandle, WorkerHandle};

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<WorktreeInfo>, ErrorDto>>) {
    let result = git_core::worktree::list_worktrees(repo).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn create(
    repo_path: &Path,
    repo: &mut git2::Repository,
    name: String,
    path: PathBuf,
    branch: String,
    start_point: Option<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let mut result =
        git_core::worktree::create_worktree(repo, &name, &path, &branch, start_point.as_deref())
            .map_err(ErrorDto::from_error);
    if result.is_ok() {
        result = git_core::repo::open(repo_path)
            .map(|reopened| *repo = reopened)
            .map_err(ErrorDto::from_error);
    }
    let _ = reply.send(result);
}

pub(super) fn remove(
    repo_path: &Path,
    repo: &mut git2::Repository,
    name: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    // A bare repository has no open worktree to protect.
    let current_workdir = match repo.workdir() {
        None => Ok(None),
        Some(path) => path
            .canonicalize()
            .map(Some)
            .map_err(|_| ErrorDto::from("cannot determine the open worktree")),
    };
    let mut result = current_workdir.and_then(|current_workdir| {
        git_core::worktree::list_worktrees(repo)
            .and_then(|worktrees| {
                worktrees
                    .into_iter()
                    .find(|worktree| !worktree.is_main && worktree.name == name)
                    .ok_or(git_core::worktree::WorktreeError::Git)
            })
            .map_err(ErrorDto::from_error)
            .and_then(|worktree| {
                if current_workdir.as_ref() == Some(&worktree.path) {
                    return Err("cannot remove the currently open worktree".into());
                }
                git_core::worktree::remove_worktree_named(repo, &worktree.name, &worktree.path)
                    .map_err(ErrorDto::from_error)
            })
    });
    if result.is_ok() {
        result = git_core::repo::open(repo_path)
            .map(|reopened| *repo = reopened)
            .map_err(ErrorDto::from_error);
    }
    let _ = reply.send(result);
}

pub(super) fn prune(
    repo_path: &Path,
    repo: &mut git2::Repository,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let mut result = git_core::worktree::prune_worktrees(repo).map_err(ErrorDto::from_error);
    if result.is_ok() {
        result = git_core::repo::open(repo_path)
            .map(|reopened| *repo = reopened)
            .map_err(ErrorDto::from_error);
    }
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn create_worktree(
        &self,
        name: String,
        path: PathBuf,
        branch: String,
        start_point: Option<String>,
    ) -> Result<(), ErrorDto> {
        self.request(|reply_tx| Command::CreateWorktree {
            name,
            path,
            branch,
            start_point,
            reply: reply_tx,
        })
    }

    pub fn remove_worktree(&self, name: String) -> Result<(), ErrorDto> {
        self.request(|reply_tx| Command::RemoveWorktree {
            name,
            reply: reply_tx,
        })
    }

    pub fn prune_worktrees(&self) -> Result<(), ErrorDto> {
        self.request(|reply_tx| Command::PruneWorktrees { reply: reply_tx })
    }
}

impl ReadHandle {
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>, ErrorDto> {
        self.read(|reply_tx| Command::ListWorktrees { reply: reply_tx })
    }
}
