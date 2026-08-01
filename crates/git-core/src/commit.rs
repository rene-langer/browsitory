use git2::{Oid, Repository};

use crate::repo::Result;

/// Commits the current index contents onto HEAD (or as the first commit, on
/// an unborn HEAD). Requires `user.name`/`user.email` to be set in the
/// repository or global git config, same as the real `git commit`.
pub fn create_commit(repo: &Repository, message: &str) -> Result<Oid> {
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = repo.signature()?;

    let parent_commit = match repo.head() {
        Ok(head) => Some(head.peel_to_commit()?),
        Err(_) => None,
    };
    let parents: Vec<&git2::Commit> = parent_commit.iter().collect();

    let oid = repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )?;
    Ok(oid)
}
