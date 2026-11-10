//! "Collapse merges" (#617): the per-repository setting and the commits of an expanded merge.

use repo_service::wire::{ErrorDto, GraphCommitDto};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;

use super::{worker_handle, RepoPathParams, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetMergedCommitsParams {
    repo_path: String,
    merge_id: String,
    limit: usize,
}

/// The commits merge `merge_id` brought in, at most `limit`: a collapsed merge expanded.
pub(super) fn get_merged_commits(
    params: GetMergedCommitsParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let commits: Vec<GraphCommitDto> = worker_handle(repos, &params.repo_path)?
        .get_merged_commits(params.merge_id, params.limit)?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(ErrorDto::from_error)
}

/// Whether the graph of `repoPath` collapses merges.
pub(super) fn get_graph_collapse_merges(params: RepoPathParams) -> Result<Value, ErrorDto> {
    let collapse = config::get_graph_collapse_merges(Path::new(&params.repo_path))
        .map_err(ErrorDto::from_error)?;
    Ok(Value::Bool(collapse))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetGraphCollapseMergesParams {
    repo_path: String,
    collapse: bool,
}

pub(super) fn set_graph_collapse_merges(
    params: SetGraphCollapseMergesParams,
) -> Result<Value, ErrorDto> {
    config::set_graph_collapse_merges(Path::new(&params.repo_path), params.collapse)
        .map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}
