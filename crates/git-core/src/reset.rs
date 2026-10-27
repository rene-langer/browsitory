use git2::{Oid, Repository, RepositoryState, ResetType};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Debug, Error)]
pub enum ResetError {
    #[error("{0}")]
    Git(#[from] git2::Error),
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Moves the current branch (or detached HEAD) to `commit_id`. Soft keeps index + working tree,
/// Mixed resets the index only, Hard resets both — untracked files are never removed (same as
/// `git reset --hard`).
pub fn reset_to(repo: &Repository, commit_id: &str, mode: ResetMode) -> Result<(), ResetError> {
    if repo.state() != RepositoryState::Clean {
        return Err(ResetError::OperationInProgress);
    }
    let target = repo.find_commit(Oid::from_str(commit_id)?)?;
    let kind = match mode {
        ResetMode::Soft => ResetType::Soft,
        ResetMode::Mixed => ResetType::Mixed,
        ResetMode::Hard => ResetType::Hard,
    };
    repo.reset(target.as_object(), kind, None)?;
    // Like git's `remove_branch_state`: a reset abandons a pending squash merge's message.
    crate::merge::clear_squash_message(repo)?;
    Ok(())
}
