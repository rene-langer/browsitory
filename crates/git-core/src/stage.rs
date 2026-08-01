use std::path::Path;

use git2::Repository;

use crate::repo::Result;

/// Stages a path's current working-tree content (add for new/modified files,
/// remove-from-index for files deleted on disk).
pub fn stage_path(repo: &Repository, path: &str) -> Result<()> {
    let mut index = repo.index()?;
    let exists_on_disk = repo
        .workdir()
        .map(|workdir| workdir.join(path).exists())
        .unwrap_or(false);

    if exists_on_disk {
        index.add_path(Path::new(path))?;
    } else {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    Ok(())
}

/// Unstages a path back to its HEAD state (or removes it from the index
/// entirely on an unborn HEAD, since there is no HEAD state to reset to).
pub fn unstage_path(repo: &Repository, path: &str) -> Result<()> {
    match repo.head() {
        Ok(head) => {
            let head_commit = head.peel_to_commit()?;
            repo.reset_default(Some(head_commit.as_object()), [path])?;
        }
        Err(_) => {
            let mut index = repo.index()?;
            index.remove_path(Path::new(path))?;
            index.write()?;
        }
    }
    Ok(())
}
