use repo_service::wire::ErrorDto;
use repo_service::wire::{
    HistoryEditTargetDto, RebaseAbortDto, RebasePlanCommitDto, RebasePlanEntryDto,
    RebaseStepResultDto,
};
use serde::Deserialize;
use serde_json::Value;

use std::sync::{Arc, Mutex};

use super::{hook_relay, worker_handle, RepoPathParams, Repos};

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

/// A step's signing program is relayed as `hookProgress` (`hook: "signing"`, `source: "Pick"`),
/// so the hook strip can cancel it (#151).
pub(super) fn start_rebase<W: std::io::Write + Send + 'static>(
    params: StartRebaseParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let result = worker_handle(repos, &params.repo_path)?.start_rebase_reporting(
        params.onto,
        params.plan.into_iter().map(Into::into).collect(),
        params.edit.map(Into::into),
        events,
    )?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RebaseContinueParams {
    repo_path: String,
    /// Replaces the message of the step Continue lands (#882).
    #[serde(default)]
    message: Option<String>,
}

/// The signing program as for `start_rebase`.
pub(super) fn rebase_continue<W: std::io::Write + Send + 'static>(
    params: RebaseContinueParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let result = worker_handle(repos, &params.repo_path)?
        .rebase_continue_reporting(params.message, events)?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FoldStagedParams {
    repo_path: String,
    commit_id: String,
}

/// Folds the staged changes into `commitId` (#883); the signing program as for `start_rebase`.
pub(super) fn fold_staged_into<W: std::io::Write + Send + 'static>(
    params: FoldStagedParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let result =
        worker_handle(repos, &params.repo_path)?.fold_staged_reporting(params.commit_id, events)?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(ErrorDto::from_error)
}

pub(super) fn abort_rebase(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let abort = worker_handle(repos, &params.repo_path)?.abort_rebase()?;
    serde_json::to_value(RebaseAbortDto::from(abort)).map_err(ErrorDto::from_error)
}
