use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use git_core::worktree::WorktreeInfo;

use super::{Command, WorkerHandle};

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<WorktreeInfo>, String>>) {
    let result = git_core::worktree::list_worktrees(repo).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn create(
    repo_path: &Path,
    repo: &mut git2::Repository,
    name: String,
    path: PathBuf,
    branch: String,
    start_point: Option<String>,
    reply: Sender<Result<(), String>>,
) {
    let mut result =
        git_core::worktree::create_worktree(repo, &name, &path, &branch, start_point.as_deref())
            .map_err(|error| error.to_string());
    if result.is_ok() {
        result = git_core::repo::open(repo_path)
            .map(|reopened| *repo = reopened)
            .map_err(|error| error.to_string());
    }
    let _ = reply.send(result);
}

pub(super) fn remove(
    repo_path: &Path,
    repo: &mut git2::Repository,
    name: String,
    reply: Sender<Result<(), String>>,
) {
    let current_workdir = repo
        .workdir()
        .and_then(|path| path.canonicalize().ok())
        .ok_or_else(|| "cannot determine the open worktree".to_string());
    let mut result = current_workdir.and_then(|current_workdir| {
        git_core::worktree::list_worktrees(repo)
            .and_then(|worktrees| {
                worktrees
                    .into_iter()
                    .find(|worktree| !worktree.is_main && worktree.name == name)
                    .ok_or(git_core::worktree::WorktreeError::Git)
            })
            .map_err(|error| error.to_string())
            .and_then(|worktree| {
                if worktree.path == current_workdir {
                    return Err("cannot remove the currently open worktree".to_string());
                }
                git_core::worktree::remove_worktree(repo, &worktree.path)
                    .map_err(|error| error.to_string())
            })
    });
    if result.is_ok() {
        result = git_core::repo::open(repo_path)
            .map(|reopened| *repo = reopened)
            .map_err(|error| error.to_string());
    }
    let _ = reply.send(result);
}

pub(super) fn prune(
    repo_path: &Path,
    repo: &mut git2::Repository,
    reply: Sender<Result<(), String>>,
) {
    let mut result = git_core::worktree::prune_worktrees(repo).map_err(|error| error.to_string());
    if result.is_ok() {
        result = git_core::repo::open(repo_path)
            .map(|reopened| *repo = reopened)
            .map_err(|error| error.to_string());
    }
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>, String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListWorktrees { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn create_worktree(
        &self,
        name: String,
        path: PathBuf,
        branch: String,
        start_point: Option<String>,
    ) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::CreateWorktree {
                name,
                path,
                branch,
                start_point,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn remove_worktree(&self, name: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::RemoveWorktree {
                name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn prune_worktrees(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::PruneWorktrees { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
