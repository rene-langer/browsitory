//! Fold staged changes into an older commit (#883), like `git commit --fixup <target>` and
//! `git rebase --autosquash` in one step: the staged changes become part of `target`, which keeps
//! its message and author; the commits after it are replayed.
//!
//! The staged changes are committed on the branch first (a `fixup!` commit, the rebase's
//! `original_tip`), then a rebase from the target's parent picks every commit and folds that one
//! into the target as a `Fixup`. `RebaseState::fold_base` remembers the tip before it, so Abort
//! (and a start that fails) put the branch back there with the changes staged again. Like a
//! rebase, it runs no hooks; only a signing program, when commits are signed.
use crate::hooks::HookOptions;
use crate::rebase::{RebaseAction, RebaseError, RebasePlanEntry, RebaseState, RebaseStepResult};
use git2::{Oid, Repository};

/// Folds the staged changes into `target`, a commit of the current branch's first-parent
/// history. Returns the paused rebase (`None` once done) and the step result. The latest commit
/// is simply amended. Refuses, changing nothing, when nothing is staged, a tracked file has
/// unstaged changes, an operation is in progress or HEAD is detached.
pub fn fold_staged_into(
    repo: &Repository,
    target: &str,
    mut hooks: HookOptions<'_>,
) -> Result<(Option<RebaseState>, RebaseStepResult), RebaseError> {
    crate::repo::check_no_terminal_rebase(repo)?;
    if let Some(kind) = crate::repo::paused_operation(repo) {
        return Err(RebaseError::OperationPaused(kind));
    }
    crate::pick::check_no_terminal_sequence(repo).map_err(RebaseError::TerminalSequence)?;
    if repo.head_detached()? {
        return Err(RebaseError::InvalidPlan(
            "cannot fold staged changes while HEAD is detached — check out a branch first"
                .to_string(),
        ));
    }
    let head_ref = repo.head()?;
    let branch_ref = head_ref.name()?.to_string();
    let head = head_ref.peel_to_commit()?;
    let target = repo.revparse_single(target)?.peel_to_commit()?;
    check_on_branch(&head, target.id())?;
    if target.parent_count() > 1 {
        return Err(RebaseError::InvalidPlan(
            "merge commits can't take staged changes".to_string(),
        ));
    }

    let mut index = repo.index()?;
    index.read(false)?;
    if index.has_conflicts() {
        return Err(RebaseError::OperationInProgress);
    }
    let staged = repo.find_tree(index.write_tree()?)?;
    if staged.id() == head.tree_id() {
        return Err(RebaseError::NothingStaged);
    }
    let unstaged = unstaged_changes(repo)?;
    if !unstaged.is_empty() {
        return Err(RebaseError::UnstagedChanges { paths: unstaged });
    }

    let committer = repo.signature()?;
    let message = String::from_utf8_lossy(target.message_bytes()).into_owned();
    if target.id() == head.id() {
        // The latest commit: amend it, keeping its message, author and parents.
        let parents: Vec<git2::Commit<'_>> = head.parents().collect();
        let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
        let amended = crate::commit::create_signed_reporting(
            repo,
            &mut hooks,
            None,
            &head.author(),
            &committer,
            &message,
            &staged,
            &parents,
        )?;
        move_branch(
            repo,
            &branch_ref,
            head.id(),
            amended,
            "commit (amend): fold staged changes",
        )?;
        return Ok((None, RebaseStepResult::Done));
    }

    let onto = target.parent(0).map_err(|_| {
        RebaseError::InvalidPlan(
            "the first commit of the history can't take staged changes".to_string(),
        )
    })?;
    let summary = String::from_utf8_lossy(target.summary_bytes().unwrap_or_default()).into_owned();
    let fixup = crate::commit::create_signed_reporting(
        repo,
        &mut hooks,
        None,
        &committer,
        &committer,
        &format!("fixup! {summary}"),
        &staged,
        &[&head],
    )?;
    move_branch(
        repo,
        &branch_ref,
        head.id(),
        fixup,
        "commit (fixup): fold staged changes",
    )?;

    let plan = fold_plan(repo, onto.id(), target.id(), fixup);
    let started = plan.and_then(|plan| {
        crate::rebase::start_rebase_with(repo, &onto.id().to_string(), plan, hooks)
    });
    match started {
        Ok((_, RebaseStepResult::Done)) => Ok((None, RebaseStepResult::Done)),
        Ok((mut state, step)) => {
            state.fold_base = Some(head.id());
            Ok((Some(state), step))
        }
        Err(error) => {
            // `start_rebase` rolled back to the branch at the fixup commit (or never left it):
            // take the commit off again, so the changes are staged as before.
            let _ = put_back(repo, &branch_ref, fixup, head.id());
            Err(error)
        }
    }
}

/// `target` is on `head`'s first-parent history.
fn check_on_branch(head: &git2::Commit<'_>, target: Oid) -> Result<(), RebaseError> {
    let mut commit = head.clone();
    loop {
        if commit.id() == target {
            return Ok(());
        }
        if commit.parent_count() > 1 {
            return Err(RebaseError::InvalidPlan(
                "a merge commit comes after the commit to fold into".to_string(),
            ));
        }
        commit = match commit.parent(0) {
            Ok(parent) => parent,
            Err(_) => {
                return Err(RebaseError::InvalidPlan(
                    "the commit to fold into isn't on the current branch".to_string(),
                ))
            }
        };
    }
}

/// Every commit after `onto` picked, with `fixup` (the branch's new tip) folded into `target`
/// right after it.
fn fold_plan(
    repo: &Repository,
    onto: Oid,
    target: Oid,
    fixup: Oid,
) -> Result<Vec<RebasePlanEntry>, RebaseError> {
    let entry = |id: Oid, action| RebasePlanEntry {
        commit_id: id.to_string(),
        action,
        combined_message: None,
    };
    let mut plan = Vec::new();
    for commit in crate::rebase::commits_since(repo, &onto.to_string())? {
        let id = Oid::from_str(&commit.id)?;
        if id == fixup {
            continue;
        }
        plan.push(entry(id, RebaseAction::Pick));
        if id == target {
            plan.push(entry(fixup, RebaseAction::Fixup));
        }
    }
    Ok(plan)
}

/// Moves `branch_ref` from `from` to `to` (compare-and-swap); HEAD follows, being on it.
fn move_branch(
    repo: &Repository,
    branch_ref: &str,
    from: Oid,
    to: Oid,
    log: &str,
) -> Result<(), RebaseError> {
    repo.reference_matching(branch_ref, to, true, from, log)?;
    Ok(())
}

/// Takes the fold's fixup commit off the branch again (like `git reset --soft HEAD~1`), so its
/// changes are staged: only while HEAD is on the branch and the branch is at that commit. The
/// index and work tree are the fixup commit's tree, so they stay as they are.
pub(crate) fn put_back(
    repo: &Repository,
    branch_ref: &str,
    fixup: Oid,
    base: Oid,
) -> Result<(), RebaseError> {
    let head = repo.head()?;
    if repo.head_detached()? || head.name().ok() != Some(branch_ref) {
        return Ok(());
    }
    move_branch(
        repo,
        branch_ref,
        fixup,
        base,
        "reset: fold staged changes undone",
    )
}

/// Unstaged changes to tracked files (modified, deleted, type change), sorted by path:
/// Abort and a failed start check the branch out with force, so they would be lost.
fn unstaged_changes(repo: &Repository) -> Result<Vec<String>, RebaseError> {
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(false)
        .include_ignored(false)
        .exclude_submodules(true);
    let unstaged = git2::Status::WT_MODIFIED
        | git2::Status::WT_DELETED
        | git2::Status::WT_TYPECHANGE
        | git2::Status::WT_RENAMED;
    let mut paths: Vec<String> = repo
        .statuses(Some(&mut options))?
        .iter()
        .filter(|entry| entry.status().intersects(unstaged))
        .map(|entry| String::from_utf8_lossy(entry.path_bytes()).into_owned())
        .collect();
    paths.sort();
    Ok(paths)
}
