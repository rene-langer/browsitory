use git2::{BranchType, Repository};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BranchError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("branch '{0}' has unmerged commits; use force to delete anyway")]
    NotMerged(String),
    /// Another worktree holds the branch: checked out, being rebased or bisected, or used by a
    /// paused cherry-pick or revert there (#291, #293).
    #[error(transparent)]
    Held(#[from] crate::repo::BranchHeld),
    #[error("worktree operation failed: {0}")]
    Worktree(#[from] crate::worktree::WorktreeError),
    #[error("{0}")]
    LinkedFolder(#[from] crate::LinkedFolder),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
    /// `core.ignorecase` can't be read, so the case-collision check can't run (#225).
    #[error(transparent)]
    CaseConfig(#[from] crate::CaseConfigUnreadable),
    /// Windows: a checkout stopped at a file another program holds open (#303).
    #[error(transparent)]
    HeldOpen(#[from] crate::HeldOpen),
    /// The branch is being rebased in a terminal, in this or another worktree (#263).
    #[error(transparent)]
    BeingRebased(#[from] crate::repo::BranchBeingRebased),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
    /// `<remote>/<branch>` from `branch.<name>.remote`/`.merge`; `None` without upstream config.
    pub upstream: Option<String>,
    /// Commits on the branch not on its tracking ref; `None` without upstream or tracking ref.
    pub ahead: Option<usize>,
    /// Commits on the tracking ref not on the branch; `None` as for `ahead`.
    pub behind: Option<usize>,
}

pub fn list_branches(repo: &Repository) -> Result<Vec<BranchInfo>, BranchError> {
    let config = repo.config()?;
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        let configured = crate::remote::configured_upstream(&config, name)?;
        let (ahead, behind) = match &configured {
            Some((remote, remote_branch)) => {
                crate::remote::ahead_behind(repo, name, remote, remote_branch)
            }
            None => (None, None),
        };
        branches.push(BranchInfo {
            name: name.to_string(),
            is_current: branch.is_head(),
            upstream: configured.map(|(remote, remote_branch)| format!("{remote}/{remote_branch}")),
            ahead,
            behind,
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
    // HEAD is detached during a rebase, so the check below doesn't see a branch another
    // worktree is rebasing; git refuses it ("already used by worktree"), since a commit made on
    // it here is lost when that rebase is aborted or finished (#263).
    crate::repo::check_branch_not_rebased_elsewhere(repo, name)?;
    // Read from each worktree's git folder, so this works in a linked worktree of a bare
    // repository too (#291).
    if let Some(held) = crate::repo::branch_held(
        repo,
        name,
        crate::repo::HoldRule::CheckOut {
            include_this: false,
        },
    )? {
        return Err(held.into());
    }
    let branch_ref = format!("refs/heads/{name}");
    let target = repo.find_reference(&branch_ref)?.peel_to_commit()?;
    // Checking out the tree before moving HEAD means a refused checkout (libgit2's default
    // "safe" strategy errors rather than overwriting modified/untracked files that differ from
    // the target) leaves the repo exactly as it was — HEAD only moves once the working
    // directory update has already succeeded.
    // Safe checkouts create files through a symlinked folder (#114).
    let tree = target.tree()?;
    crate::worktree_guard::refuse_linked_folders::<BranchError>(
        repo,
        &[crate::worktree_guard::Incoming::Checkout(&tree)],
    )?;
    // A file with other hard links gets a name of its own before it is written (#420).
    let mut checkout = git2::build::CheckoutBuilder::new();
    crate::hardlink::on_checkout(repo, &mut checkout);
    // A file another program holds open: say so, not libgit2's raw error (#303).
    repo.checkout_tree(target.as_object(), Some(&mut checkout))
        .map_err(crate::worktree_guard::held_open::<BranchError>(
            repo.workdir(),
            crate::HeldDuring::Switch,
        ))?;
    repo.set_head(&branch_ref)?;
    Ok(())
}

/// Checks out `commit_id` as a detached HEAD. Same refuse-before-move ordering as
/// `switch_branch`: the tree checkout (safe strategy) runs first, so a refused checkout leaves
/// HEAD and the working tree untouched.
pub fn checkout_commit(repo: &Repository, commit_id: &str) -> Result<(), BranchError> {
    let target = repo.find_commit(git2::Oid::from_str(commit_id)?)?;
    // Safe checkouts create files through a symlinked folder (#114).
    let tree = target.tree()?;
    crate::worktree_guard::refuse_linked_folders::<BranchError>(
        repo,
        &[crate::worktree_guard::Incoming::Checkout(&tree)],
    )?;
    // A file with other hard links gets a name of its own before it is written (#420).
    let mut checkout = git2::build::CheckoutBuilder::new();
    crate::hardlink::on_checkout(repo, &mut checkout);
    // A file another program holds open: say so, not libgit2's raw error (#303).
    repo.checkout_tree(target.as_object(), Some(&mut checkout))
        .map_err(crate::worktree_guard::held_open::<BranchError>(
            repo.workdir(),
            crate::HeldDuring::Switch,
        ))?;
    repo.set_head_detached(target.id())?;
    Ok(())
}

pub fn rename_branch(repo: &Repository, old_name: &str, new_name: &str) -> Result<(), BranchError> {
    crate::repo::check_branch_not_rebased(repo, old_name)?;
    // A branch checked out elsewhere may be renamed (that worktree's HEAD moves along), but not
    // one a rebase or bisect returns to, as `git branch -m` refuses (#291).
    if let Some(held) = crate::repo::branch_held(repo, old_name, crate::repo::HoldRule::Rename)? {
        return Err(held.into());
    }
    let mut branch = repo.find_branch(old_name, BranchType::Local)?;
    branch.rename(new_name, false)?;
    Ok(())
}

pub fn delete_branch(repo: &Repository, name: &str, force: bool) -> Result<(), BranchError> {
    crate::repo::check_branch_not_rebased(repo, name)?;
    if let Some(held) = crate::repo::branch_held(repo, name, crate::repo::HoldRule::MoveOrDelete)? {
        return Err(held.into());
    }
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
