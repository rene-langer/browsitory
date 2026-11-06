use repo_service::wire::SearchPageDto;
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchCommitsParams {
    repo_path: String,
    query: String,
    refs: Option<Vec<String>>,
    cursor: Option<String>,
}

pub(super) fn search_commits(params: SearchCommitsParams, repos: &Repos) -> Result<Value, String> {
    let page = worker_handle(repos, &params.repo_path)?.search_commits(
        params.query,
        params.refs,
        params.cursor,
    )?;
    serde_json::to_value(SearchPageDto::from(page)).map_err(|error| error.to_string())
}

/// Stops the repo's in-flight search, if any. Every request runs on its own thread (see
/// `main.rs`), and the cancel goes through the worker's side channel, so it never waits for the
/// search it stops.
pub(super) fn cancel_search(params: RepoPathParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.cancel_search();
    Ok(Value::Null)
}
