use repo_service::wire::ErrorDto;
use repo_service::wire::{GraphSelectionDto, SearchPageDto};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchCommitsParams {
    repo_path: String,
    query: String,
    selection: GraphSelectionDto,
    cursor: Option<String>,
}

pub(super) fn search_commits(
    params: SearchCommitsParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let page = worker_handle(repos, &params.repo_path)?.search_commits(
        params.query,
        params.selection.into(),
        params.cursor,
    )?;
    serde_json::to_value(SearchPageDto::from(page)).map_err(ErrorDto::from_error)
}

/// Stops the repo's in-flight search, if any. Every request runs on its own thread (see
/// `main.rs`), and the cancel goes through the worker's side channel, so it never waits for the
/// search it stops.
pub(super) fn cancel_search(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.cancel_search();
    Ok(Value::Null)
}
