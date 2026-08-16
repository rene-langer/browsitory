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

    if !repo
        .reflog(reference)?
        .iter()
        .any(|entry| entry.id_new() == target)
    {
        return Err(ReflogError::TargetNotInReflog);
    }

    let mut selected = if reference == "HEAD" {
        repo.head()?
    } else {
        repo.find_reference(reference)?
    };
    selected.set_target(target, RESTORE_REFLOG_MESSAGE)?;
    Ok(())
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
