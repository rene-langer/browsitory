use repo_service::wire::ErrorDto;
use repo_service::wire::{
    ConflictSegmentDto, ConflictSideDto, FileConflictChoiceDto, MarkResolvedOutcomeDto,
    MergeOptionsDto, MergeOutcomeDto,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{hook_relay, worker_handle, RepoFilePathParams, RepoPathParams, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MarkResolvedParams {
    repo_path: String,
    path: String,
    force: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResolveWholeFileParams {
    repo_path: String,
    path: String,
    side: ConflictSideDto,
}

pub(super) fn resolve_whole_file(
    params: ResolveWholeFileParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.resolve_whole_file(params.path, params.side.into())?;
    Ok(Value::Null)
}

pub(super) fn mark_resolved(params: MarkResolvedParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let outcome =
        worker_handle(repos, &params.repo_path)?.mark_resolved(params.path, params.force)?;
    serde_json::to_value(MarkResolvedOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartMergeParams {
    repo_path: String,
    branch_name: String,
    options: MergeOptionsDto,
}

/// A fast-forward's or squash's `post-merge` is relayed as `hookProgress` with
/// `source: "Merge"` (#185).
pub(super) fn start_merge<W: std::io::Write + Send + 'static>(
    params: StartMergeParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let outcome = worker_handle(repos, &params.repo_path)?.start_merge(
        params.branch_name,
        params.options.into(),
        events,
    )?;
    serde_json::to_value(MergeOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

/// The remembered merge options for the repo, else the defaults from its `merge.ff`.
pub(super) fn get_merge_options(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let options = match config::get_merge_preference(Path::new(&params.repo_path))
        .map_err(ErrorDto::from_error)?
    {
        Some(preference) => MergeOptionsDto::from(preference),
        None => MergeOptionsDto::from(worker_handle(repos, &params.repo_path)?.merge_defaults()?),
    };
    serde_json::to_value(options).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetMergeOptionsParams {
    repo_path: String,
    options: MergeOptionsDto,
}

pub(super) fn set_merge_options(params: SetMergeOptionsParams) -> Result<Value, ErrorDto> {
    config::set_merge_preference(
        Path::new(&params.repo_path),
        params.options.fast_forward.into(),
        params.options.squash,
    )
    .map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

pub(super) fn get_conflict_hunks(
    params: RepoFilePathParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let segments: Vec<ConflictSegmentDto> = worker_handle(repos, &params.repo_path)?
        .get_conflict_hunks(params.path)?
        .into_iter()
        .map(ConflictSegmentDto::from)
        .collect();
    serde_json::to_value(segments).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResolveConflictParams {
    repo_path: String,
    path: String,
    resolved_content: String,
}

pub(super) fn resolve_conflict(
    params: ResolveConflictParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?
        .resolve_conflict(params.path, params.resolved_content)?;
    Ok(Value::Null)
}

pub(super) fn abort_merge(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.abort_merge()?;
    Ok(Value::Null)
}

pub(super) fn get_merge_message(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let message = worker_handle(repos, &params.repo_path)?.get_merge_message()?;
    serde_json::to_value(message).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResolveAddDeleteConflictParams {
    repo_path: String,
    path: String,
    choice: FileConflictChoiceDto,
}

pub(super) fn resolve_add_delete_conflict(
    params: ResolveAddDeleteConflictParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?
        .resolve_add_delete_conflict(params.path, params.choice.into())?;
    Ok(Value::Null)
}
