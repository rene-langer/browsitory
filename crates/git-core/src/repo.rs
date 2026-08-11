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
