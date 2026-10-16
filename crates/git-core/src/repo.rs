use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("failed to open repository: {0}")]
    Open(#[from] git2::Error),
}

pub fn open(path: &Path) -> Result<git2::Repository, RepoError> {
    Ok(git2::Repository::discover(path)?)
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
