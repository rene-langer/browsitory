use git2::{BranchType, Repository};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BranchError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("branch '{0}' has unmerged commits; use force to delete anyway")]
    NotMerged(String),
    #[error("branch '{0}' is checked out in another worktree")]
    CheckedOutInAnotherWorktree(String),
    #[error("worktree operation failed: {0}")]
    Worktree(#[from] crate::worktree::WorktreeError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
}

pub fn list_branches(repo: &Repository) -> Result<Vec<BranchInfo>, BranchError> {
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        branches.push(BranchInfo {
            name: name.to_string(),
            is_current: branch.is_head(),
        });
    }
    Ok(branches)
}

fn resolve_start_point<'repo>(
    repo: &'repo Repository,
    start_point: &str,
) -> Result<git2::Commit<'repo>, BranchError> {
    if start_point == "HEAD" {
        Ok(repo.head()?.peel_to_commit()?)
    } else {
        Ok(repo.revparse_single(start_point)?.peel_to_commit()?)
    }
}

pub fn create_branch(repo: &Repository, name: &str, start_point: &str) -> Result<(), BranchError> {
    let commit = resolve_start_point(repo, start_point)?;
    repo.branch(name, &commit, false)?;
    // The branch ref above is created regardless of what happens next — if switch_branch
    // fails (e.g. a dirty-tree conflict), the branch stays created rather than being rolled
    // back, matching `git checkout -b`'s own behavior. See the design spec's note on this.
    switch_branch(repo, name)
}

pub fn switch_branch(repo: &Repository, name: &str) -> Result<(), BranchError> {
    let branch_ref = format!("refs/heads/{name}");
    let current_workdir = repo.workdir().and_then(|path| path.canonicalize().ok());
    let checked_out_elsewhere = crate::worktree::list_worktrees(repo)?
        .into_iter()
        .any(|worktree| worktree.head == name && current_workdir.as_ref() != Some(&worktree.path));
    if checked_out_elsewhere {
        return Err(BranchError::CheckedOutInAnotherWorktree(name.to_string()));
    }
    let target = repo.find_reference(&branch_ref)?.peel_to_commit()?;
    // Checking out the tree before moving HEAD means a refused checkout (libgit2's default
    // "safe" strategy errors rather than overwriting modified/untracked files that differ from
    // the target) leaves the repo exactly as it was — HEAD only moves once the working
    // directory update has already succeeded.
    repo.checkout_tree(target.as_object(), None)?;
    repo.set_head(&branch_ref)?;
    Ok(())
}

pub fn rename_branch(repo: &Repository, old_name: &str, new_name: &str) -> Result<(), BranchError> {
    let mut branch = repo.find_branch(old_name, BranchType::Local)?;
    branch.rename(new_name, false)?;
    Ok(())
}

pub fn delete_branch(repo: &Repository, name: &str, force: bool) -> Result<(), BranchError> {
    let mut branch = repo.find_branch(name, BranchType::Local)?;
    if !force {
        let branch_oid = branch.get().peel_to_commit()?.id();
        let head_oid = repo.head()?.peel_to_commit()?.id();
        // libgit2's graph_descendant_of treats a commit as not-a-descendant-of-itself, so an
        // exact tip match (the branch is fully caught up with HEAD, no divergent commits)
        // needs its own check to count as merged.
        let merged = branch_oid == head_oid || repo.graph_descendant_of(head_oid, branch_oid)?;
        if !merged {
            return Err(BranchError::NotMerged(name.to_string()));
        }
    }
    branch.delete()?;
    Ok(())
}
