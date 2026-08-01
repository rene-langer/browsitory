use git2::{BranchType, Repository};

use crate::repo::Result;

/// Lists local branch names for a branch-picker dropdown (merge's "merge
/// this branch into HEAD", rebase's "upstream"/"onto").
///
/// Deliberately minimal: a fuller branch-management module (create/delete/
/// checkout/rename, remote branches, ...) is expected from the parallel
/// branch/stash workstream. This exists only so the merge/rebase UI has
/// something to populate a dropdown with in the meantime, and is expected to
/// be de-duplicated against that workstream's version when the two branches
/// are merged together.
pub fn list_local_branch_names(repo: &Repository) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for branch in repo.branches(Some(BranchType::Local))? {
        let (branch, _kind) = branch?;
        if let Some(name) = branch.name()? {
            names.push(name.to_string());
        }
    }
    Ok(names)
}
