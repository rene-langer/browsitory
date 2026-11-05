use repo_service::wire::ReflogEntryDto;
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

pub(super) fn list_reflog_refs(params: RepoPathParams, repos: &Repos) -> Result<Value, String> {
    let refs = worker_handle(repos, &params.repo_path)?.list_reflog_refs()?;
    serde_json::to_value(refs).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetReflogParams {
    repo_path: String,
    reference: String,
}

pub(super) fn get_reflog(params: GetReflogParams, repos: &Repos) -> Result<Value, String> {
    let entries: Vec<ReflogEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_reflog(params.reference)?
        .into_iter()
        .map(ReflogEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RestoreReflogEntryParams {
    repo_path: String,
    reference: String,
    new_id: String,
}

pub(super) fn restore_reflog_entry(
    params: RestoreReflogEntryParams,
    repos: &Repos,
) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?
        .restore_reflog_entry(params.reference, params.new_id)?;
    Ok(Value::Null)
}
