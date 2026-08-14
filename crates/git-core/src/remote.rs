use git2::{BranchType, ErrorCode, Repository};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: String,
    pub fetch_url: String,
    pub push_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamInfo {
    pub local_branch: String,
    pub remote_name: String,
    pub remote_branch: String,
}

#[derive(Debug, Error)]
pub enum RemoteError {
    #[error("git remote operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("remote '{name}' is the upstream for local branch(es): {branches:?}")]
    RemoteInUse { name: String, branches: Vec<String> },
}

pub fn list_remotes(repo: &Repository) -> Result<Vec<RemoteInfo>, RemoteError> {
    let mut remotes = Vec::new();
    for name in repo.remotes()?.iter().flatten().flatten() {
        let remote = repo.find_remote(name)?;
        remotes.push(RemoteInfo {
            name: name.to_string(),
            fetch_url: remote.url()?.to_string(),
            push_url: remote.pushurl()?.map(str::to_string),
        });
    }
    Ok(remotes)
}

pub fn add_remote(
    repo: &Repository,
    name: &str,
    fetch_url: &str,
    push_url: Option<&str>,
) -> Result<(), RemoteError> {
    repo.remote(name, fetch_url)?;
    if let Some(push_url) = push_url {
        repo.remote_set_pushurl(name, Some(push_url))?;
    }
    Ok(())
}

pub fn rename_remote(repo: &Repository, old_name: &str, new_name: &str) -> Result<(), RemoteError> {
    repo.remote_rename(old_name, new_name)?;
    Ok(())
}

pub fn update_remote_urls(
    repo: &Repository,
    name: &str,
    fetch_url: &str,
    push_url: Option<&str>,
) -> Result<(), RemoteError> {
    let remote = repo.find_remote(name)?;
    let has_push_url = remote.pushurl()?.is_some();
    drop(remote);
    repo.remote_set_url(name, fetch_url)?;
    if push_url.is_some() || has_push_url {
        repo.remote_set_pushurl(name, push_url)?;
    }
    Ok(())
}

pub fn current_upstream(repo: &Repository) -> Result<Option<UpstreamInfo>, RemoteError> {
    let local_branch = current_local_branch_name(repo)?;
    let config = repo.config()?;
    let remote_key = format!("branch.{local_branch}.remote");
    let remote_name = match config.get_string(&remote_key) {
        Ok(name) => name,
        Err(error) if error.code() == ErrorCode::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let merge_ref = config.get_string(&format!("branch.{local_branch}.merge"))?;
    let remote_branch = merge_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(&merge_ref)
        .to_string();

    Ok(Some(UpstreamInfo {
        local_branch,
        remote_name,
        remote_branch,
    }))
}

pub fn set_current_upstream(
    repo: &Repository,
    remote_name: &str,
    remote_branch: &str,
) -> Result<(), RemoteError> {
    let local_branch = current_local_branch_name(repo)?;
    let mut branch = repo.find_branch(&local_branch, BranchType::Local)?;
    let upstream = format!("{remote_name}/{remote_branch}");
    match branch.set_upstream(Some(&upstream)) {
        Ok(()) => {}
        Err(error) if error.code() == ErrorCode::NotFound => {
            let mut config = repo.config()?;
            config.set_str(&format!("branch.{local_branch}.remote"), remote_name)?;
            config.set_str(
                &format!("branch.{local_branch}.merge"),
                &format!("refs/heads/{remote_branch}"),
            )?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

pub fn clear_current_upstream(repo: &Repository) -> Result<(), RemoteError> {
    let local_branch = current_local_branch_name(repo)?;
    let mut branch = repo.find_branch(&local_branch, BranchType::Local)?;
    branch.set_upstream(None)?;
    Ok(())
}

pub fn remove_remote(repo: &Repository, name: &str) -> Result<(), RemoteError> {
    let branches = local_branches_using_remote(repo, name)?;
    if !branches.is_empty() {
        return Err(RemoteError::RemoteInUse {
            name: name.to_string(),
            branches,
        });
    }
    repo.remote_delete(name)?;
    Ok(())
}

fn current_local_branch_name(repo: &Repository) -> Result<String, RemoteError> {
    Ok(repo.head()?.shorthand()?.to_string())
}

fn local_branches_using_remote(
    repo: &Repository,
    remote_name: &str,
) -> Result<Vec<String>, RemoteError> {
    let config = repo.config()?;
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        let remote_key = format!("branch.{name}.remote");
        match config.get_string(&remote_key) {
            Ok(configured_remote) if configured_remote == remote_name => {
                branches.push(name.to_string())
            }
            Ok(_) => {}
            Err(error) if error.code() == ErrorCode::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(branches)
}
