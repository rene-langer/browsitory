use repo_service::wire::{
    CreatePullRequestDto, ForgeProviderDto, ForgeRepositoryDto, PullRequestDto, PullRequestListDto,
};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

pub(super) fn detect_forge_repository(
    params: RepoPathParams,
    repos: &Repos,
) -> Result<Value, String> {
    let repositories: Vec<ForgeRepositoryDto> = worker_handle(repos, &params.repo_path)?
        .detect_forge_repository()?
        .into_iter()
        .map(ForgeRepositoryDto::from)
        .collect();
    serde_json::to_value(repositories).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ForgeTokenParams {
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
    token: String,
}

pub(super) fn save_forge_token(params: ForgeTokenParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.save_forge_token(
        params.provider.into(),
        params.account,
        params.token,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ForgetForgeTokenParams {
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
}

pub(super) fn forget_forge_token(
    params: ForgetForgeTokenParams,
    repos: &Repos,
) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?
        .forget_forge_token(params.provider.into(), params.account)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListPullRequestsParams {
    repo_path: String,
    remote_name: String,
    account: String,
}

pub(super) fn list_pull_requests(
    params: ListPullRequestsParams,
    repos: &Repos,
) -> Result<Value, String> {
    let list = worker_handle(repos, &params.repo_path)?
        .list_pull_requests(params.remote_name, params.account)?;
    serde_json::to_value(PullRequestListDto::from(list)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CreatePullRequestParams {
    repo_path: String,
    remote_name: String,
    account: String,
    pull_request: CreatePullRequestDto,
}

pub(super) fn create_pull_request(
    params: CreatePullRequestParams,
    repos: &Repos,
) -> Result<Value, String> {
    let created = worker_handle(repos, &params.repo_path)?.create_pull_request(
        params.remote_name,
        params.account,
        params.pull_request.into(),
    )?;
    serde_json::to_value(PullRequestDto::from(created)).map_err(|error| error.to_string())
}
