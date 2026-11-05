use repo_service::wire::SubmoduleInfoDto;
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoFilePathParams, RepoPathParams, Repos};

pub(super) fn list_submodules(params: RepoPathParams, repos: &Repos) -> Result<Value, String> {
    let submodules: Vec<SubmoduleInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_submodules()?
        .into_iter()
        .map(SubmoduleInfoDto::from)
        .collect();
    serde_json::to_value(submodules).map_err(|error| error.to_string())
}

pub(super) fn init_submodule(params: RepoFilePathParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.init_submodule(params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpdateSubmoduleParams {
    repo_path: String,
    path: String,
    recursive: bool,
}

pub(super) fn update_submodule(
    params: UpdateSubmoduleParams,
    repos: &Repos,
) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.update_submodule(params.path, params.recursive)?;
    Ok(Value::Null)
}
