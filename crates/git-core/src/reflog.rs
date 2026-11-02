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
    /// Another worktree has the branch checked out (#287); `git branch -f` refuses it too.
    #[error("branch '{0}' is checked out in another worktree")]
    CheckedOutInAnotherWorktree(String),
    /// A terminal rebase in this or another worktree is rebasing the branch (#287).
    #[error(transparent)]
    BeingRebased(#[from] crate::repo::BranchBeingRebased),
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

    let mut selected = selected_reference(repo, reference)?;
    if let Some(branch) = selected
        .name()
        .ok()
        .filter(|name| name.starts_with("refs/heads/"))
    {
        check_branch_movable(repo, branch)?;
    }
    selected.set_target(target, RESTORE_REFLOG_MESSAGE)?;
    Ok(())
}

/// Refuses to move `branch` (full name) under a worktree that has it: checked out in another
/// worktree, or being rebased by a terminal rebase in any worktree (#287), as `git branch -f`
/// refuses. The branch checked out in this worktree is still moved: that is what restoring an
/// entry of HEAD's reflog does (the work tree and index stay, like `git reset --soft`).
fn check_branch_movable(repo: &Repository, branch: &str) -> Result<(), ReflogError> {
    let short = branch.strip_prefix("refs/heads/").unwrap_or(branch);
    crate::repo::check_branch_not_rebased(repo, short)?;
    if crate::repo::branches_checked_out_elsewhere(repo)?.contains(branch) {
        return Err(ReflogError::CheckedOutInAnotherWorktree(short.to_string()));
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
