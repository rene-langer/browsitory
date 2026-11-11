//! #448: revert or apply a commit's file or hunk in the working tree, a patch's text, a patch
//! applied. The extension host shows the save and open dialogs and does the file I/O.
use repo_service::wire::ErrorDto;
use repo_service::wire::{
    CommitChangeRequestDto, PatchApplyDto, PatchApplyOutcomeDto, PatchContentDto, PatchPreviewDto,
    PatchSourceDto,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use super::{hook_relay, worker_handle, Repos};

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
    how: PatchApplyDto,
}

/// As commits, `git am`'s hooks are relayed as `hookProgress` with `source: "Patch"` (#747).
pub(super) fn apply_patch<W: std::io::Write + Send + 'static>(
    params: ApplyPatchParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let bytes = params.content.into_bytes()?;
    let events = hook_relay(&params.repo_path, out);
    let outcome = worker_handle(repos, &params.repo_path)?.apply_patch_reported(
        bytes,
        params.how.into(),
        events,
    )?;
    serde_json::to_value(PatchApplyOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PreviewPatchParams {
    repo_path: String,
    content: PatchContentDto,
}

pub(super) fn preview_patch(params: PreviewPatchParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let bytes = params.content.into_bytes()?;
    let preview = worker_handle(repos, &params.repo_path)?.preview_patch(bytes)?;
    serde_json::to_value(PatchPreviewDto::from(preview)).map_err(ErrorDto::from_error)
}
