use repo_service::wire::ErrorDto;
use repo_service::wire::TagInfoDto;
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

pub(super) fn list_tags(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let tags: Vec<TagInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_tags()?
        .into_iter()
        .map(TagInfoDto::from)
        .collect();
    serde_json::to_value(tags).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreateTagParams {
    repo_path: String,
    name: String,
    message: Option<String>,
    // The commit to tag (a full id); missing or `null` tags HEAD's commit.
    #[serde(default)]
    target: Option<String>,
}

pub(super) fn create_tag(params: CreateTagParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.create_tag(
        params.name,
        params.message,
        params.target,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeleteTagParams {
    repo_path: String,
    name: String,
}

pub(super) fn delete_tag(params: DeleteTagParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.delete_tag(params.name)?;
    Ok(Value::Null)
}
