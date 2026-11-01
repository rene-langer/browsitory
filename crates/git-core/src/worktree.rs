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
    #[error("Can't create the folder '{}': {reason}", path.display())]
    CreateFolder { path: PathBuf, reason: String },
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
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
    create_worktree_with(
        repo,
        name,
        path,
        branch,
        start_point,
        crate::worktree_guard::dir_folds_case,
    )
}

/// [`create_worktree`] with the probe of the destination's case folding given (`probe`, called
/// with the nearest existing folder of `path`), so tests can stand in for a case-insensitive
/// drive.
#[doc(hidden)]
pub fn create_worktree_with(
    repo: &Repository,
    name: &str,
    path: &Path,
    branch: &str,
    start_point: Option<&str>,
    probe: impl FnOnce(&Path) -> std::io::Result<bool>,
) -> Result<(), WorktreeError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return Err(WorktreeError::PathExists),
        // A file where a parent folder should be is named when the parents are created.
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) => {}
        Err(_) => return Err(WorktreeError::Git),
    }
    // The worktree may live on another file system than the repository (#162): probed only
    // when the repository's setting doesn't already say case folds. A failed probe leaves the
    // repository's setting to decide, as before.
    let case_matters = crate::worktree_guard::folds_case(repo)
        || crate::worktree_guard::nearest_existing(path)
            .is_some_and(|dir| probe(dir).unwrap_or(false));

    let (existing, commit) = match repo.find_branch(branch, BranchType::Local) {
        Ok(branch) => {
            let commit = branch.get().peel_to_commit()?;
            (Some(branch), commit)
        }
        Err(error) if error.code() == ErrorCode::NotFound => {
            let start_point = start_point.ok_or(WorktreeError::StartPointRequired)?;
            (None, repo.revparse_single(start_point)?.peel_to_commit()?)
        }
        Err(error) => return Err(error.into()),
    };
    // git2 has no checkout options for worktree add, so the tree is checked up front (#156),
    // before anything (a new branch included) is created: when the repository's config says
    // case matters, or the destination's file system folds case (#162).
    let tree = commit.tree()?;
    crate::worktree_guard::refuse_case_collision_if::<WorktreeError, &str>(
        case_matters,
        &crate::worktree_guard::GuardSource::Tree(&tree),
        None,
    )?;
    // Like `git worktree add`, missing parent folders of the destination are created (#200);
    // libgit2 only creates the last one. Whatever this call creates is removed again when a
    // later step fails, so a retry with the same name and branch starts clean.
    let mut created = Vec::new();
    if let Err(error) = create_parent_folders(path, &mut created) {
        remove_created_folders(&created);
        return Err(error);
    }
    let admin = single_component(name).map(|name| repo.commondir().join("worktrees").join(name));
    let admin_existed = admin
        .as_deref()
        .is_none_or(|admin| std::fs::symlink_metadata(admin).is_ok());
    let mut created_branch = false;
    let added = (|| {
        let reference = match existing {
            Some(branch) => branch.into_reference(),
            None => {
                let new_branch = repo.branch(branch, &commit, false)?;
                created_branch = true;
                new_branch.into_reference()
            }
        };
        let mut options = git2::WorktreeAddOptions::new();
        options.reference(Some(&reference));
        repo.worktree(name, path, Some(&options))?;
        Ok::<(), WorktreeError>(())
    })();
    if added.is_err() {
        // A half-made entry (no `gitdir` file yet) blocks a retry with the same name, and its
        // HEAD would keep the new branch from being deleted.
        if let Some(admin) = admin.filter(|_| !admin_existed) {
            if !admin.join("gitdir").exists() {
                let _ = std::fs::remove_dir_all(&admin);
            }
        }
        if created_branch {
            if let Ok(mut new_branch) = repo.find_branch(branch, BranchType::Local) {
                let _ = new_branch.delete();
            }
        }
        // libgit2 creates the destination itself; it goes only while still empty.
        let _ = std::fs::remove_dir(path);
        remove_created_folders(&created);
    }
    added
}

/// Creates the missing ancestors of `path` below its nearest existing folder, top-down, pushing
/// each folder made here on `created`.
fn create_parent_folders(path: &Path, created: &mut Vec<PathBuf>) -> Result<(), WorktreeError> {
    let (Some(parent), Some(base)) = (path.parent(), crate::worktree_guard::nearest_existing(path))
    else {
        return Ok(());
    };
    let missing: Vec<&Path> = parent
        .ancestors()
        .take_while(|ancestor| *ancestor != base && !ancestor.as_os_str().is_empty())
        .collect();
    for folder in missing.into_iter().rev() {
        match std::fs::create_dir(folder) {
            Ok(()) => created.push(folder.to_path_buf()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && folder.is_dir() => {}
            Err(error) => {
                return Err(WorktreeError::CreateFolder {
                    path: folder.to_path_buf(),
                    reason: error.to_string(),
                })
            }
        }
    }
    Ok(())
}

/// Removes the folders [`create_parent_folders`] made, deepest first, each only while empty.
fn remove_created_folders(created: &[PathBuf]) {
    for folder in created.iter().rev() {
        let _ = std::fs::remove_dir(folder);
    }
}

/// `name` when it is one plain path component, so joining it can't leave `worktrees/`.
fn single_component(name: &str) -> Option<&str> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None) => Some(name),
        _ => None,
    }
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
