use repo_service::wire::ErrorDto;
use repo_service::wire::{
    to_trailers, CommitOutcomeDto, ConfirmedAbortDto, HeadInfoDto, PickOutcomeDto, ResetModeDto,
    SequenceAbortDto, SequenceOutcomeDto, SignatureInputDto,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::{mpsc, Arc, Mutex};

use super::{
    hook_relay, spawn_hook_relay, worker_handle, CommitMessageParams, RepoPathParams, Repos,
};

pub(super) fn list_recent_authors(
    params: RepoPathParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let authors: Vec<SignatureInputDto> = worker_handle(repos, &params.repo_path)?
        .list_recent_authors()?
        .into_iter()
        .map(SignatureInputDto::from)
        .collect();
    serde_json::to_value(authors).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AbortSequenceParams {
    repo_path: String,
    #[serde(default)]
    confirmed: Option<ConfirmedAbortDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommitIdParams {
    repo_path: String,
    commit_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResetParams {
    repo_path: String,
    commit_id: String,
    mode: ResetModeDto,
}

pub(super) fn get_head_info(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let info = worker_handle(repos, &params.repo_path)?.get_head_info()?;
    serde_json::to_value(HeadInfoDto::from(info)).map_err(ErrorDto::from_error)
}

pub(super) fn amend_commit(
    params: CommitMessageParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(params.repo_path.clone(), event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.amend_commit(
        params.message,
        params.no_verify,
        params.author.map(|a| git_core::commit::SignatureInput {
            name: a.name,
            email: a.email,
        }),
        to_trailers(params.trailers),
        event_tx,
    )?;
    serde_json::to_value(CommitOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PickParams {
    repo_path: String,
    commit_id: String,
    /// 1-based parent for a merge commit; absent or `null` otherwise.
    #[serde(default)]
    mainline: Option<u32>,
}

/// A clean pick's `prepare-commit-msg`/`post-commit` are relayed as `hookProgress` with
/// `source: "Pick"` (#183).
pub(super) fn cherry_pick<W: std::io::Write + Send + 'static>(
    params: PickParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let outcome = worker_handle(repos, &params.repo_path)?.cherry_pick(
        params.commit_id,
        params.mainline,
        events,
    )?;
    serde_json::to_value(PickOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

pub(super) fn revert_commit<W: std::io::Write + Send + 'static>(
    params: PickParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let outcome = worker_handle(repos, &params.repo_path)?.revert_commit(
        params.commit_id,
        params.mainline,
        events,
    )?;
    serde_json::to_value(PickOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommitIdsParams {
    repo_path: String,
    /// Oldest-first.
    commit_ids: Vec<String>,
}

pub(super) fn cherry_pick_many<W: std::io::Write + Send + 'static>(
    params: CommitIdsParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let outcome =
        worker_handle(repos, &params.repo_path)?.cherry_pick_many(params.commit_ids, events)?;
    serde_json::to_value(SequenceOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

pub(super) fn revert_many<W: std::io::Write + Send + 'static>(
    params: CommitIdsParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    let outcome =
        worker_handle(repos, &params.repo_path)?.revert_many(params.commit_ids, events)?;
    serde_json::to_value(SequenceOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContinueSequenceParams {
    repo_path: String,
    /// "Continue without hooks" (#133); absent means run them.
    #[serde(default)]
    no_verify: bool,
}

/// The step Continue commits runs the commit hooks, relayed as `hookProgress` (#133).
pub(super) fn continue_sequence<W: std::io::Write + Send + 'static>(
    params: ContinueSequenceParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(params.repo_path.clone(), event_rx, Arc::clone(out));
    let outcome =
        worker_handle(repos, &params.repo_path)?.continue_sequence(params.no_verify, event_tx)?;
    serde_json::to_value(SequenceOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

pub(super) fn abort_sequence(
    params: AbortSequenceParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let confirmed = params.confirmed.map(git_core::pick::ConfirmedAbort::from);
    let abort = worker_handle(repos, &params.repo_path)?.abort_sequence(confirmed)?;
    serde_json::to_value(SequenceAbortDto::from(abort)).map_err(ErrorDto::from_error)
}

pub(super) fn reset_to_commit(params: ResetParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?
        .reset_to_commit(params.commit_id, params.mode.into())?;
    Ok(Value::Null)
}

/// `post-checkout` is relayed as `hookProgress` with `source: "Checkout"` (#185).
pub(super) fn checkout_commit<W: std::io::Write + Send + 'static>(
    params: CommitIdParams,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, ErrorDto> {
    let events = hook_relay(&params.repo_path, out);
    worker_handle(repos, &params.repo_path)?.checkout_commit(params.commit_id, events)?;
    Ok(Value::Null)
}
