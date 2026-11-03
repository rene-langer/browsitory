use git2::{Oid, Reference, Repository};
use thiserror::Error;

const RESTORE_REFLOG_MESSAGE: &str = "browsitory: restore reflog entry";

#[derive(Debug, Error)]
pub enum ReflogError {
    #[error("reference must be HEAD or a valid local branch")]
    InvalidReference,
    #[error("target object was not found")]
    TargetNotFound,
    #[error("target object is not in the selected reflog")]
    TargetNotInReflog,
    /// Another worktree holds the branch: checked out, or being rebased or bisected there
    /// (#287, #291); `git branch -f` refuses it too.
    #[error(transparent)]
    HeldElsewhere(#[from] crate::repo::BranchHeldElsewhere),
    /// A terminal rebase in this or another worktree is rebasing the branch (#287).
    #[error(transparent)]
    BeingRebased(#[from] crate::repo::BranchBeingRebased),
    /// Restoring this worktree's HEAD while a merge, cherry-pick, revert or rebase is paused,
    /// refused as Reset refuses it (#287).
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
    /// A terminal rebase or `git am` is paused in this worktree and HEAD would move (#287).
    #[error(transparent)]
    TerminalRebase(#[from] crate::repo::TerminalRebaseInProgress),
    /// The reference moved after it was read (another program, or another window).
    #[error("{0} changed while it was being restored. Refresh and try again.")]
    ReferenceMoved(String),
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReflogEntry {
    pub reference: String,
    pub old_id: String,
    pub new_id: String,
    pub committer_name: String,
    pub committer_email: String,
    pub timestamp: i64,
    pub message: String,
    pub summary: Option<String>,
}

pub fn list_reflog_refs(repo: &Repository) -> Result<Vec<String>, ReflogError> {
    let mut references = vec!["HEAD".to_string()];

    for reference in repo.references()? {
        let reference = reference?;
        let Ok(name) = reference.name() else {
            continue;
        };
        if is_local_reference(name) {
            references.push(name.to_string());
        }
    }

    references.sort();
    Ok(references)
}

pub fn read_reflog(repo: &Repository, reference: &str) -> Result<Vec<ReflogEntry>, ReflogError> {
    validate_reference(reference)?;

    if !repo.reference_has_log(reference)? {
        return Ok(Vec::new());
    }

    repo.reflog(reference)?
        .iter()
        .map(|entry| {
            let committer = entry.committer();
            let new_id = entry.id_new();
            Ok(ReflogEntry {
                reference: reference.to_string(),
                old_id: entry.id_old().to_string(),
                new_id: new_id.to_string(),
                committer_name: committer.name().ok().unwrap_or_default().to_string(),
                committer_email: committer.email().ok().unwrap_or_default().to_string(),
                timestamp: committer.when().seconds(),
                message: entry.message()?.unwrap_or_default().to_string(),
                summary: repo
                    .find_commit(new_id)
                    .ok()
                    .and_then(|commit| commit.summary().ok().flatten().map(str::to_string)),
            })
        })
        .collect()
}

pub fn restore_reflog_entry(
    repo: &Repository,
    reference: &str,
    new_id: &str,
) -> Result<(), ReflogError> {
    validate_reference(reference)?;
    let target = Oid::from_str(new_id).map_err(|_| ReflogError::TargetNotFound)?;
    repo.find_object(target, None)
        .map_err(|_| ReflogError::TargetNotFound)?;

    if !repo.reference_has_log(reference)? {
        return Err(ReflogError::TargetNotInReflog);
    }

    if !repo
        .reflog(reference)?
        .iter()
        .any(|entry| entry.id_new() == target)
    {
        return Err(ReflogError::TargetNotInReflog);
    }

    let selected = selected_reference(repo, reference)?;
    let name = selected.name()?.to_string();
    let head_target = repo
        .find_reference("HEAD")?
        .symbolic_target()?
        .map(str::to_string);
    if name == "HEAD" || head_target.as_deref() == Some(name.as_str()) {
        check_head_movable(repo)?;
    }
    if let Some(short) = name.strip_prefix("refs/heads/") {
        check_branch_movable(repo, short)?;
    }
    let Some(current) = selected.target() else {
        return Err(ReflogError::InvalidReference);
    };
    // Compare-and-swap: a ref moved since it was read here (by another program or window) is
    // left alone.
    repo.reference_matching(&name, target, true, current, RESTORE_REFLOG_MESSAGE)
        .map_err(|error| match error.code() {
            git2::ErrorCode::Modified => {
                ReflogError::ReferenceMoved(name.trim_start_matches("refs/heads/").to_string())
            }
            _ => error.into(),
        })?;
    Ok(())
}

/// Restoring this worktree's HEAD (detached, or the branch it is on) moves only the ref: the
/// work tree and index stay. That is refused while an operation is paused, as Reset refuses it
/// (#287): a paused terminal rebase would build on the restored commit and drop the steps it
/// already replayed, and a paused merge would get a different first parent.
fn check_head_movable(repo: &Repository) -> Result<(), ReflogError> {
    crate::repo::check_no_terminal_rebase(repo)?;
    if !crate::repo::no_operation(repo) {
        return Err(ReflogError::OperationInProgress);
    }
    Ok(())
}

/// Refuses to move branch `short` (without `refs/heads/`) under a worktree that has it: held by
/// another worktree (checked out, rebased or bisected there), or being rebased by a terminal
/// rebase in any worktree (#287, #291), as `git branch -f` refuses. The branch checked out in
/// this worktree is still moved: that is what restoring an entry of HEAD's reflog does.
fn check_branch_movable(repo: &Repository, short: &str) -> Result<(), ReflogError> {
    crate::repo::check_branch_not_rebased(repo, short)?;
    if let Some(held) = crate::repo::branch_held_elsewhere(repo, short, true)? {
        return Err(held.into());
    }
    Ok(())
}

fn selected_reference<'repo>(
    repo: &'repo Repository,
    reference: &str,
) -> Result<Reference<'repo>, ReflogError> {
    if reference != "HEAD" {
        return Ok(repo.find_reference(reference)?);
    }

    let head = repo.find_reference("HEAD")?;
    let symbolic_target = head.symbolic_target()?.map(str::to_string);
    match symbolic_target {
        Some(target) if is_local_reference(&target) => Ok(repo.find_reference(&target)?),
        Some(_) => Err(ReflogError::InvalidReference),
        None => Ok(head),
    }
}

fn validate_reference(reference: &str) -> Result<(), ReflogError> {
    if reference == "HEAD" || is_local_reference(reference) {
        Ok(())
    } else {
        Err(ReflogError::InvalidReference)
    }
}

fn is_local_reference(reference: &str) -> bool {
    reference.starts_with("refs/heads/") && Reference::is_valid_name(reference)
}
