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
    /// The target's tree holds a case pair HEAD's doesn't, while `core.ignorecase` is on: the
    /// checkout would keep only one of its names (#532).
    #[error(transparent)]
    CasePairBroughtIn(#[from] crate::CasePairBroughtIn),
    /// `core.ignorecase` can't be read, so the case-collision check can't run (#225).
    #[error(transparent)]
    CaseConfig(#[from] crate::CaseConfigUnreadable),
    /// Windows: a checkout stopped at a file another program holds open (#303).
    #[error(transparent)]
    HeldOpen(#[from] crate::HeldOpen),
    /// The branch is being rebased in a terminal, in this or another worktree (#263).
    #[error(transparent)]
    BeingRebased(#[from] crate::repo::BranchBeingRebased),
    /// The switch's safe checkout would overwrite local changes (or untracked files) that differ
    /// from the target; nothing changed. [`switch_conflicts`] names the files (#653).
    #[error(
        "Your local changes would be overwritten by switching to {0}. Commit or stash them first."
    )]
    LocalChanges(String),
    /// Stashing the local changes for [`stash_and_switch`] failed, or putting them back did.
    #[error(transparent)]
    Stash(#[from] crate::stash::StashError),
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
    list_branches_cached(repo, &mut crate::ahead_behind::AheadBehindCache::default())
}

/// [`list_branches`], with the ahead/behind counts kept in `cache` (#112): only a branch or
/// upstream that moved since the last call is counted again. Keeps only the counts of the
/// branches it lists.
pub fn list_branches_cached(
    repo: &Repository,
    cache: &mut crate::ahead_behind::AheadBehindCache,
) -> Result<Vec<BranchInfo>, BranchError> {
    let config = repo.config()?;
    cache.begin(repo);
    let mut pairs = Vec::new();
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        let configured = crate::remote::configured_upstream(&config, name)?;
        let (ahead, behind) = match &configured {
            Some((remote, remote_branch)) => {
                let pair = crate::remote::tracking_pair(repo, name, remote, remote_branch);
                pairs.extend(pair);
                crate::remote::pair_ahead_behind(repo, cache, pair)
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
    cache.retain(&pairs);
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

/// Creates `name` at `start_point` without checking it out, like `git branch <name> <start>`.
pub fn create_branch_only(
    repo: &Repository,
    name: &str,
    start_point: &str,
) -> Result<(), BranchError> {
    let commit = resolve_start_point(repo, start_point)?;
    repo.branch(name, &commit, false)?;
    Ok(())
}

/// Creates `name` at `start_point` and switches to it, like `git switch -c <name> <start>`.
pub fn create_branch(repo: &Repository, name: &str, start_point: &str) -> Result<(), BranchError> {
    let commit = resolve_start_point(repo, start_point)?;
    // Before the branch is created: a start point whose tree brings in a case pair would only
    // leave a branch the switch below refuses (#532).
    refuse_case_pair_brought_in(repo, &commit.tree()?)?;
    repo.branch(name, &commit, false)?;
    // The branch ref above is created regardless of what happens next — if switch_branch
    // fails (e.g. a dirty-tree conflict), the branch stays created rather than being rolled
    // back, matching `git checkout -b`'s own behavior. See the design spec's note on this.
    switch_branch(repo, name)
}

/// Refuses a switch or checkout whose `tree` holds a case pair (`A` and `a`) HEAD's tree doesn't,
/// while `core.ignorecase` is on (#532). libgit2's safe checkout keeps only one of the two names
/// in its ignore-case index and doesn't write the other's file, so the next commit would drop
/// it. Refused rather than written around: on the case-insensitive drives `core.ignorecase` is
/// meant for, the two files can't both exist anyway. Writes nothing.
fn refuse_case_pair_brought_in(
    repo: &Repository,
    tree: &git2::Tree<'_>,
) -> Result<(), BranchError> {
    let head_tree = repo.head().and_then(|head| head.peel_to_tree()).ok();
    crate::worktree_guard::refuse_case_pair_brought_in(
        repo,
        &crate::worktree_guard::GuardSource::Tree(tree),
        head_tree.as_ref(),
    )
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
    refuse_case_pair_brought_in(repo, &tree)?;
    // A file with other hard links gets a name of its own before it is written (#420).
    let mut checkout = git2::build::CheckoutBuilder::new();
    crate::hardlink::on_checkout(repo, &mut checkout);
    // A file another program holds open: say so, not libgit2's raw error (#303). Local changes
    // in the way are their own error, so the app can offer to stash them (#653).
    repo.checkout_tree(target.as_object(), Some(&mut checkout))
        .map_err(|error| {
            if error.code() == git2::ErrorCode::Conflict {
                BranchError::LocalChanges(name.to_string())
            } else {
                crate::worktree_guard::held_open::<BranchError>(
                    repo.workdir(),
                    crate::HeldDuring::Switch,
                )(error)
            }
        })?;
    repo.set_head(&branch_ref)?;
    Ok(())
}

/// The files whose local changes (or untracked files) keep a switch to `name` from running: every
/// changed, staged or untracked path that the switch would change too (HEAD's tree and the
/// target's differ there), which is what the safe checkout refuses to overwrite. Reads only.
/// Empty when no local change is in the way.
pub fn switch_conflicts(repo: &Repository, name: &str) -> Result<Vec<String>, BranchError> {
    let target = repo
        .find_reference(&format!("refs/heads/{name}"))?
        .peel_to_tree()?;
    let head = repo.head().and_then(|head| head.peel_to_tree()).ok();
    let diff = repo.diff_tree_to_tree(head.as_ref(), Some(&target), None)?;
    let mut switched = std::collections::BTreeSet::new();
    for delta in diff.deltas() {
        for path in [delta.old_file().path(), delta.new_file().path()]
            .into_iter()
            .flatten()
        {
            switched.insert(path.to_string_lossy().replace('\\', "/"));
        }
    }
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let mut paths = Vec::new();
    for entry in repo.statuses(Some(&mut options))?.iter() {
        let Ok(path) = entry.path() else { continue };
        if switched.contains(path) {
            paths.push(path.to_string());
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// What [`stash_and_switch`] did with the stashed changes once on the new branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StashSwitchOutcome {
    /// Left in the stash list, as asked (`stash@{0}`).
    Stashed,
    /// Applied on the new branch and dropped from the stash list.
    Reapplied,
    /// Applied with conflicts; the stash stays in the list, like `git stash pop`.
    Conflicted { files: Vec<String> },
    /// Couldn't be applied on the new branch (the text says why); the stash stays in the list.
    NotReapplied { reason: String },
}

/// "Stash and switch" (#653): stashes every local change (untracked files too), switches to
/// `name`, then, with `reapply`, pops the stash onto the new branch. A switch that is refused
/// after the stash was saved puts the changes back before returning its error, so a failure
/// leaves the repository as it was.
pub fn stash_and_switch(
    repo: &mut Repository,
    name: &str,
    reapply: bool,
) -> Result<StashSwitchOutcome, BranchError> {
    // The refusals that don't depend on the work tree, before anything is stashed.
    crate::repo::check_branch_not_rebased_elsewhere(repo, name)?;
    if let Some(held) = crate::repo::branch_held(
        repo,
        name,
        crate::repo::HoldRule::CheckOut {
            include_this: false,
        },
    )? {
        return Err(held.into());
    }
    repo.find_reference(&format!("refs/heads/{name}"))?;
    let from = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().ok().map(str::to_string))
        .unwrap_or_else(|| "HEAD".to_string());
    let options = crate::stash::StashOptions {
        message: Some(format!("Changes on {from} before switching to {name}")),
        keep_index: false,
        include_untracked: true,
        paths: Vec::new(),
    };
    crate::stash::save_stash(repo, &options)?;
    let stash_id = crate::stash::list_stashes(repo)?
        .into_iter()
        .next()
        .map(|entry| entry.commit_id);
    if let Err(error) = switch_branch(repo, name) {
        // Back to how it was: the switch moved nothing, so the stash applies cleanly.
        crate::stash::pop_stash(repo, 0, stash_id.as_deref(), true)?;
        return Err(error);
    }
    if !reapply {
        return Ok(StashSwitchOutcome::Stashed);
    }
    let popped = match crate::stash::pop_stash(repo, 0, stash_id.as_deref(), true) {
        // The staged part doesn't fit the new branch's index: apply it all unstaged instead.
        Err(crate::stash::StashError::IndexConflict) => {
            crate::stash::pop_stash(repo, 0, stash_id.as_deref(), false)
        }
        other => other,
    };
    Ok(match popped {
        Ok(crate::stash::StashApplyOutcome::Applied) => StashSwitchOutcome::Reapplied,
        Ok(crate::stash::StashApplyOutcome::Conflicted { files }) => {
            StashSwitchOutcome::Conflicted { files }
        }
        Err(error) => StashSwitchOutcome::NotReapplied {
            reason: error.to_string(),
        },
    })
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
    refuse_case_pair_brought_in(repo, &tree)?;
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
