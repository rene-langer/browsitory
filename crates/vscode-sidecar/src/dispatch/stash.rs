use repo_service::wire::ErrorDto;
use repo_service::wire::{StashApplyOutcomeDto, StashEntryDto, StashFilesDto, StashOptionsDto};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

pub(super) fn list_stashes(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let stashes: Vec<StashEntryDto> = worker_handle(repos, &params.repo_path)?
        .list_stashes()?
        .into_iter()
        .map(StashEntryDto::from)
        .collect();
    serde_json::to_value(stashes).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SaveStashParams {
    repo_path: String,
    options: StashOptionsDto,
}

pub(super) fn save_stash(params: SaveStashParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.save_stash(params.options.into())?;
    Ok(Value::Null)
}

/// A stash by list index plus the commit id the client listed there; the backend refuses with
/// "The stash list changed" when `stash@{index}` is now another stash.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StashIndexParams {
    repo_path: String,
    index: usize,
    commit_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StashApplyParams {
    repo_path: String,
    index: usize,
    commit_id: String,
    restore_index: bool,
}

pub(super) fn apply_stash(params: StashApplyParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let outcome = worker_handle(repos, &params.repo_path)?.apply_stash(
        params.index,
        params.commit_id,
        params.restore_index,
    )?;
    serde_json::to_value(StashApplyOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

pub(super) fn pop_stash(params: StashApplyParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let outcome = worker_handle(repos, &params.repo_path)?.pop_stash(
        params.index,
        params.commit_id,
        params.restore_index,
    )?;
    serde_json::to_value(StashApplyOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StashFilesParams {
    repo_path: String,
    commit_id: String,
}

pub(super) fn get_stash_files(params: StashFilesParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let files = worker_handle(repos, &params.repo_path)?.get_stash_files(params.commit_id)?;
    serde_json::to_value(StashFilesDto::from(files)).map_err(ErrorDto::from_error)
}

pub(super) fn drop_stash(params: StashIndexParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.drop_stash(params.index, params.commit_id)?;
    Ok(Value::Null)
}
