use std::path::Path;

use git2::Repository;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error(transparent)]
    Git(#[from] git2::Error),
    /// One or more refs were rejected by the remote on push — surfaced via
    /// `RemoteCallbacks::push_update_reference`, since `Remote::push()`
    /// itself returns `Ok(())` even when the server rejected every ref (see
    /// `transfer::push`'s doc comment).
    #[error("push rejected: {0}")]
    Rejected(String),
}

pub type Result<T> = std::result::Result<T, GitError>;

/// Opens the repository containing `path`, walking up parent directories to
/// find `.git` the same way the `git` CLI does (so this works from any
/// subdirectory of a working tree, not just the repo root).
pub fn open(path: impl AsRef<Path>) -> Result<Repository> {
    Ok(Repository::discover(path)?)
}
