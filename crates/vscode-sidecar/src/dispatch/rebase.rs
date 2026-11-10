use repo_service::wire::ErrorDto;
use repo_service::wire::{
    HistoryEditTargetDto, RebaseAbortDto, RebasePlanCommitDto, RebasePlanEntryDto,
    RebaseStepResultDto,
};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommitsSinceParams {
    repo_path: String,
    onto: String,
}

pub(super) fn commits_since(params: CommitsSinceParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let commits: Vec<RebasePlanCommitDto> = worker_handle(repos, &params.repo_path)?
        .commits_since(params.onto)?
        .into_iter()
        .map(RebasePlanCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StartRebaseParams {
    repo_path: String,
    onto: String,
    plan: Vec<RebasePlanEntryDto>,
    #[serde(default)]
    edit: Option<HistoryEditTargetDto>,
}

pub(super) fn start_rebase(params: StartRebaseParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let result = worker_handle(repos, &params.repo_path)?.start_rebase(
        params.onto,
        params.plan.into_iter().map(Into::into).collect(),
        params.edit.map(Into::into),
    )?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(ErrorDto::from_error)
}

pub(super) fn rebase_continue(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let result = worker_handle(repos, &params.repo_path)?.rebase_continue()?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(ErrorDto::from_error)
}

pub(super) fn abort_rebase(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let abort = worker_handle(repos, &params.repo_path)?.abort_rebase()?;
    serde_json::to_value(RebaseAbortDto::from(abort)).map_err(ErrorDto::from_error)
}
