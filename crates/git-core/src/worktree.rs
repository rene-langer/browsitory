use std::path::{Path, PathBuf};

use git2::{BranchType, ErrorCode, Repository, WorktreeLockStatus};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("cannot remove the main worktree")]
    MainWorktree,
    #[error("worktree path already exists")]
    PathExists,
    #[error("a start point is required to create a new branch")]
    StartPointRequired,
    #[error("worktree has uncommitted changes")]
    Dirty,
    #[error("worktree is locked")]
    Locked,
    #[error("git operation failed")]
    Git,
}

impl From<git2::Error> for WorktreeError {
    fn from(error: git2::Error) -> Self {
        match error.code() {
            ErrorCode::Uncommitted | ErrorCode::Modified | ErrorCode::IndexDirty => Self::Dirty,
            ErrorCode::Locked => Self::Locked,
            _ => Self::Git,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub head: String,
    pub is_main: bool,
    pub is_locked: bool,
    pub is_prunable: bool,
}

pub fn list_worktrees(repo: &Repository) -> Result<Vec<WorktreeInfo>, WorktreeError> {
    let main_path = main_worktree_path(repo)?;
    let main_repo = Repository::open(&main_path)?;
    let mut worktrees = vec![WorktreeInfo {
        name: "main".to_string(),
        path: main_path,
        head: head_identity(&main_repo),
        is_main: true,
        is_locked: false,
        is_prunable: false,
    }];

    for name in repo.worktrees()?.iter().flatten().flatten() {
        let worktree = repo.find_worktree(name)?;
        let path = display_path(worktree.path())?;
        if worktrees.iter().any(|existing| existing.path == path) {
            continue;
        }
        worktrees.push(WorktreeInfo {
            name: name.to_string(),
            head: Repository::open(&path)
                .map(|worktree_repo| head_identity(&worktree_repo))
                .unwrap_or_default(),
            path,
            is_main: false,
            is_locked: matches!(worktree.is_locked()?, WorktreeLockStatus::Locked(_)),
            is_prunable: worktree.is_prunable(None)?,
        });
    }

    Ok(worktrees)
}

pub fn create_worktree(
    repo: &Repository,
    name: &str,
    path: &Path,
    branch: &str,
    start_point: Option<&str>,
) -> Result<(), WorktreeError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return Err(WorktreeError::PathExists),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(WorktreeError::Git),
    }

    let reference = match repo.find_branch(branch, BranchType::Local) {
        Ok(branch) => branch.into_reference(),
        Err(error) if error.code() == ErrorCode::NotFound => {
            let start_point = start_point.ok_or(WorktreeError::StartPointRequired)?;
            let commit = repo.revparse_single(start_point)?.peel_to_commit()?;
            repo.branch(branch, &commit, false)?.into_reference()
        }
        Err(error) => return Err(error.into()),
    };
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    repo.worktree(name, path, Some(&options))?;
    Ok(())
}

pub fn remove_worktree(repo: &Repository, path: &Path) -> Result<(), WorktreeError> {
    let target_path = canonical_path(path)?;
    if target_path == main_worktree_path(repo)? {
        return Err(WorktreeError::MainWorktree);
    }

    for name in repo.worktrees()?.iter().flatten().flatten() {
        let worktree = repo.find_worktree(name)?;
        if canonical_path(worktree.path())? != target_path {
            continue;
        }
        if matches!(worktree.is_locked()?, WorktreeLockStatus::Locked(_)) {
            return Err(WorktreeError::Locked);
        }
        let linked_repo = Repository::open(&target_path)?;
        if !crate::status::status(&linked_repo)
            .map_err(|_| WorktreeError::Git)?
            .is_empty()
        {
            return Err(WorktreeError::Dirty);
        }
        let mut options = git2::WorktreePruneOptions::new();
        options.valid(true).working_tree(true);
        worktree.prune(Some(&mut options))?;
        return Ok(());
    }

    Err(WorktreeError::Git)
}

pub fn prune_worktrees(repo: &Repository) -> Result<(), WorktreeError> {
    let names: Vec<String> = repo
        .worktrees()?
        .iter()
        .flatten()
        .flatten()
        .map(str::to_string)
        .collect();
    for name in names {
        let worktree = repo.find_worktree(&name)?;
        if worktree.is_prunable(None)? {
            worktree.prune(None)?;
        }
    }
    Ok(())
}

fn main_worktree_path(repo: &Repository) -> Result<PathBuf, WorktreeError> {
    let main_repo = Repository::open(repo.commondir())?;
    let Some(path) = main_repo.workdir() else {
        return Err(WorktreeError::Git);
    };
    canonical_path(path)
}

fn canonical_path(path: &Path) -> Result<PathBuf, WorktreeError> {
    path.canonicalize().map_err(|_| WorktreeError::Git)
}

fn display_path(path: &Path) -> Result<PathBuf, WorktreeError> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(_) => Err(WorktreeError::Git),
    }
}

fn head_identity(repo: &Repository) -> String {
    let Ok(head) = repo.head() else {
        return String::new();
    };
    if head.is_branch() {
        return head.shorthand().unwrap_or_default().to_string();
    }
    head.target().map(|oid| oid.to_string()).unwrap_or_default()
}
