use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("failed to open repository: {0}")]
    Open(#[from] git2::Error),
    #[error("This folder is already a Git repository.")]
    AlreadyRepository,
    #[error("failed to create repository: {0}")]
    Init(git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn open(path: &Path) -> Result<git2::Repository, RepoError> {
    Ok(git2::Repository::discover(path)?)
}

/// Creates an empty repository at `path` (and any missing parents). The initial branch honours
/// the user's global `init.defaultBranch`, else `main`.
pub fn init_repo(path: &Path) -> Result<(), RepoError> {
    if !path.is_absolute() {
        return Err(RepoError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Destination must be an absolute path.",
        )));
    }
    if path.join(".git").exists() {
        return Err(RepoError::AlreadyRepository);
    }
    std::fs::create_dir_all(path)?;
    let branch = git2::Config::open_default()
        .ok()
        .and_then(|config| config.get_string("init.defaultBranch").ok())
        .filter(|branch| !branch.is_empty())
        .unwrap_or_else(|| "main".to_string());
    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head(&branch);
    git2::Repository::init_opts(path, &options).map_err(RepoError::Init)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Merge,
    CherryPick,
    Revert,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadInfo {
    pub head_id: Option<String>,
    pub detached_at: Option<String>,
    pub head_pushed: bool,
    pub operation: Option<OperationKind>,
    /// A pending squash merge's message (`SQUASH_MSG`), which pre-fills the commit box.
    pub squash_message: Option<String>,
}

/// Snapshot of what HEAD is and which single-commit operation (if any) is paused on it. Rebase
/// is deliberately absent — it has its own richer `rebase::rebase_progress`.
pub fn head_info(repo: &git2::Repository) -> Result<HeadInfo, RepoError> {
    use git2::RepositoryState;
    let operation = match repo.state() {
        RepositoryState::Merge => Some(OperationKind::Merge),
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => {
            Some(OperationKind::CherryPick)
        }
        RepositoryState::Revert | RepositoryState::RevertSequence => Some(OperationKind::Revert),
        _ => None,
    };
    let head = match repo.head() {
        Ok(head) => head,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
            return Ok(HeadInfo {
                head_id: None,
                detached_at: None,
                head_pushed: false,
                operation,
                squash_message: crate::merge::squash_message(repo),
            });
        }
        Err(error) => return Err(error.into()),
    };
    let head_oid = head.peel_to_commit()?.id();
    let detached = repo.head_detached()?;
    let detached_at = detached.then(|| head_oid.to_string()[..7].to_string());
    let head_pushed = !detached && upstream_contains(repo, &head, head_oid);
    Ok(HeadInfo {
        head_id: Some(head_oid.to_string()),
        detached_at,
        head_pushed,
        operation,
        squash_message: crate::merge::squash_message(repo),
    })
}

fn upstream_contains(
    repo: &git2::Repository,
    head: &git2::Reference<'_>,
    head_oid: git2::Oid,
) -> bool {
    let Ok(name) = head.shorthand() else {
        return false;
    };
    let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) else {
        return false;
    };
    let Ok(upstream) = branch.upstream() else {
        return false;
    };
    let Some(upstream_oid) = upstream.get().target() else {
        return false;
    };
    upstream_oid == head_oid
        || repo
            .graph_descendant_of(upstream_oid, head_oid)
            .unwrap_or(false)
}
