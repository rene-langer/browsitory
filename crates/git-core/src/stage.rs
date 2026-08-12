use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StageError {
    #[error("failed to update the index: {0}")]
    Index(#[from] git2::Error),
    #[error("repository has no working directory (bare repository)")]
    NoWorkdir,
}

pub fn stage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    let mut index = repo.index()?;
    if workdir.join(path).exists() {
        index.add_path(Path::new(path))?;
    } else {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    Ok(())
}

pub fn unstage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let target = head_commit.as_ref().map(|c| c.as_object());
    repo.reset_default(target, [path])?;
    Ok(())
}
