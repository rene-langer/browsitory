//! #448: revert or apply a commit's file or hunk in the working tree, a patch's text, a patch
//! applied. The extension host shows the save and open dialogs and does the file I/O.
use repo_service::wire::ErrorDto;
use repo_service::wire::{CommitChangeRequestDto, PatchContentDto, PatchSourceDto};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ApplyCommitChangeParams {
    repo_path: String,
    change: CommitChangeRequestDto,
}

pub(super) fn apply_commit_change(
    params: ApplyCommitChangeParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.apply_commit_change(params.change.into())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetPatchParams {
    repo_path: String,
    source: PatchSourceDto,
}

pub(super) fn get_patch(params: GetPatchParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let bytes = worker_handle(repos, &params.repo_path)?.get_patch(params.source.into())?;
    serde_json::to_value(PatchContentDto::from_bytes(&bytes)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ApplyPatchParams {
    repo_path: String,
    content: PatchContentDto,
}

pub(super) fn apply_patch(params: ApplyPatchParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let bytes = params.content.into_bytes()?;
    worker_handle(repos, &params.repo_path)?.apply_patch(bytes)?;
    Ok(Value::Null)
}
