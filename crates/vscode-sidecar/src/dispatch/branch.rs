use repo_service::wire::BranchInfoDto;
use repo_service::wire::ErrorDto;
use repo_service::wire::StashSwitchOutcomeDto;
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use super::{hook_relay, worker_handle, RepoPathParams, Repos};

pub(super) fn list_branches(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let branches: Vec<BranchInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_branches()?
        .into_iter()
        .map(BranchInfoDto::from)
        .collect();
    serde_json::to_value(branches).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateBranchParams {
    repo_path: String,
    name: String,
    start_point: String,
    /// Absent: true, as before #656.
    checkout: Option<bool>,
}

pub(super) fn create_branch(params: CreateBranchParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.create_branch(
        params.name,
        params.start_point,
        params.checkout.unwrap_or(true),
    )?;
    Ok(Value::Null)
}

/// The files whose local changes keep a switch to `name` from running (#653).
pub(super) fn get_switch_conflicts(
    params: SwitchBranchParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let paths = worker_handle(repos, &params.repo_path)?.switch_conflicts(params.name)?;
    serde_json::to_value(paths).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StashAndSwitchParams {
    repo_path: String,
    name: String,
    reapply: bool,
}

/// Stash and switch (#653); `post-checkout` is relayed as for `switch_branch`.
pub(super) fn stash_and_switch<W: std::io::Write + Send + 'static>(
    params: StashAndSwitchParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let outcome = worker_handle(repos, &params.repo_path)?.stash_and_switch(
        params.name,
        params.reapply,
        events,
    )?;
    serde_json::to_value(StashSwitchOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SwitchBranchParams {
    repo_path: String,
    name: String,
}

/// `post-checkout` is relayed as `hookProgress` with `source: "Checkout"` (#185).
pub(super) fn switch_branch<W: std::io::Write + Send + 'static>(
    params: SwitchBranchParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    worker_handle(repos, &params.repo_path)?.switch_branch(params.name, events)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeleteBranchParams {
    repo_path: String,
    name: String,
    force: bool,
}

pub(super) fn delete_branch(params: DeleteBranchParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.delete_branch(params.name, params.force)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RenameBranchParams {
    repo_path: String,
    old_name: String,
    new_name: String,
}

pub(super) fn rename_branch(params: RenameBranchParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.rename_branch(params.old_name, params.new_name)?;
    Ok(Value::Null)
}
