use repo_service::wire::ErrorDto;
use repo_service::wire::WorktreeInfoDto;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

use super::{worker_handle, RepoPathParams, Repos};

pub(super) fn list_worktrees(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let worktrees: Vec<WorktreeInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_worktrees()?
        .into_iter()
        .map(WorktreeInfoDto::from)
        .collect();
    serde_json::to_value(worktrees).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateWorktreeParams {
    repo_path: String,
    name: String,
    path: String,
    branch: String,
    start_point: Option<String>,
}

pub(super) fn create_worktree(
    params: CreateWorktreeParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.create_worktree(
        params.name,
        PathBuf::from(params.path),
        params.branch,
        params.start_point,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WorktreeNameParams {
    repo_path: String,
    name: String,
}

pub(super) fn remove_worktree(
    params: WorktreeNameParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.remove_worktree(params.name)?;
    Ok(Value::Null)
}

pub(super) fn prune_worktrees(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.prune_worktrees()?;
    Ok(Value::Null)
}
