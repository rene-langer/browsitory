use repo_service::wire::{ChangedFileDto, CompareRangeDto, DiffHunkDto, DiffOptionsDto};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, GetCommitFilesParams, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetRangeFilesParams {
    repo_path: String,
    from_id: String,
    to_id: String,
}

pub(super) fn get_range_files(params: GetRangeFilesParams, repos: &Repos) -> Result<Value, String> {
    let files: Vec<ChangedFileDto> = worker_handle(repos, &params.repo_path)?
        .get_range_files(params.from_id, params.to_id)?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect();
    serde_json::to_value(files).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetRangeDiffParams {
    repo_path: String,
    from_id: String,
    to_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
}

pub(super) fn get_range_diff(params: GetRangeDiffParams, repos: &Repos) -> Result<Value, String> {
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_range_diff(
            params.from_id,
            params.to_id,
            params.path,
            params.old_path,
            params.opts.into(),
        )?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

pub(super) fn get_working_tree_compare_files(
    params: GetCommitFilesParams,
    repos: &Repos,
) -> Result<Value, String> {
    let files: Vec<ChangedFileDto> = worker_handle(repos, &params.repo_path)?
        .get_working_tree_compare_files(params.commit_id)?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect();
    serde_json::to_value(files).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetWorkingTreeCompareDiffParams {
    repo_path: String,
    commit_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
}

pub(super) fn get_working_tree_compare_diff(
    params: GetWorkingTreeCompareDiffParams,
    repos: &Repos,
) -> Result<Value, String> {
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_working_tree_compare_diff(
            params.commit_id,
            params.path,
            params.old_path,
            params.opts.into(),
        )?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetBranchCompareRangeParams {
    repo_path: String,
    ref_name: String,
}

pub(super) fn get_branch_compare_range(
    params: GetBranchCompareRangeParams,
    repos: &Repos,
) -> Result<Value, String> {
    let range =
        worker_handle(repos, &params.repo_path)?.get_branch_compare_range(params.ref_name)?;
    serde_json::to_value(CompareRangeDto::from(range)).map_err(|error| error.to_string())
}
