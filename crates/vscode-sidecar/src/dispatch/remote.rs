use repo_service::wire::ErrorDto;
use repo_service::wire::{
    transfer_progress_dto, PullOutcomeDto, RemoteAuthModeDto, RemoteInfoDto, UpstreamInfoDto,
};
use repo_service::worker::TransferEvent;
use serde::Deserialize;
use serde_json::Value;
use std::sync::{mpsc, Arc, Mutex};

use super::{worker_handle, RepoPathParams, Repos};

pub(super) fn list_remotes(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let worker = worker_handle(repos, &params.repo_path)?;
    let remotes: Result<Vec<RemoteInfoDto>, ErrorDto> = worker
        .list_remotes()?
        .into_iter()
        .map(|remote| {
            let profile = worker.get_remote_auth_mode(remote.name.clone())?;
            Ok(RemoteInfoDto::from((remote, profile)))
        })
        .collect();
    serde_json::to_value(remotes?).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteNameParams {
    repo_path: String,
    remote_name: String,
}

pub(super) fn list_remote_branches(
    params: RemoteNameParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let branches =
        worker_handle(repos, &params.repo_path)?.list_remote_branches(params.remote_name)?;
    serde_json::to_value(branches).map_err(ErrorDto::from_error)
}

pub(super) fn get_current_upstream(
    params: RepoPathParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let upstream = worker_handle(repos, &params.repo_path)?
        .get_current_upstream()?
        .map(UpstreamInfoDto::from);
    serde_json::to_value(upstream).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetRemoteUpstreamsParams {
    repo_path: String,
    name: String,
}

pub(super) fn get_remote_upstreams(
    params: GetRemoteUpstreamsParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let upstreams: Vec<UpstreamInfoDto> = worker_handle(repos, &params.repo_path)?
        .get_remote_upstreams(params.name)?
        .into_iter()
        .map(UpstreamInfoDto::from)
        .collect();
    serde_json::to_value(upstreams).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AddRemoteParams {
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
}

pub(super) fn add_remote(params: AddRemoteParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.add_remote(
        params.name,
        params.fetch_url,
        params.push_url,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RenameRemoteParams {
    repo_path: String,
    old_name: String,
    new_name: String,
}

pub(super) fn rename_remote(params: RenameRemoteParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.rename_remote(params.old_name, params.new_name)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct UpdateRemoteUrlsParams {
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
}

pub(super) fn update_remote_urls(
    params: UpdateRemoteUrlsParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.update_remote_urls(
        params.name,
        params.fetch_url,
        params.push_url,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoveRemoteParams {
    repo_path: String,
    name: String,
    clear_upstreams: bool,
}

pub(super) fn remove_remote(params: RemoveRemoteParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.remove_remote(params.name, params.clear_upstreams)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SaveHttpsCredentialParams {
    repo_path: String,
    remote_name: String,
    username: String,
    token: String,
}

pub(super) fn save_https_credential(
    params: SaveHttpsCredentialParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.save_https_credential(
        params.remote_name,
        params.username,
        params.token,
    )?;
    Ok(Value::Null)
}

pub(super) fn forget_https_credential(
    params: RemoteNameParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.forget_https_credential(params.remote_name)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetRemoteAuthModeParams {
    repo_path: String,
    remote_name: String,
    mode: RemoteAuthModeDto,
    username: Option<String>,
}

pub(super) fn set_remote_auth_mode(
    params: SetRemoteAuthModeParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let mode = match params.mode {
        RemoteAuthModeDto::HttpsToken => git_core::remote::RemoteAuthMode::HttpsToken {
            username: params
                .username
                .filter(|username| !username.trim().is_empty())
                .ok_or_else(|| "HTTPS username is required".to_string())?,
        },
        RemoteAuthModeDto::SshAgent => git_core::remote::RemoteAuthMode::SshAgent,
    };
    worker_handle(repos, &params.repo_path)?.set_remote_auth_mode(params.remote_name, mode)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetCurrentUpstreamParams {
    repo_path: String,
    remote_name: String,
    remote_branch: String,
}

pub(super) fn set_current_upstream(
    params: SetCurrentUpstreamParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?
        .set_current_upstream(params.remote_name, params.remote_branch)?;
    Ok(Value::Null)
}

pub(super) fn clear_current_upstream(
    params: RepoPathParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.clear_current_upstream()?;
    Ok(Value::Null)
}

/// Drains `event_rx` on a dedicated thread, writing each event as a `transferProgress` JSON-RPC
/// notification (no `id`) to the shared `out`. Exits once every `Sender<TransferEvent>` clone
/// held inside `repo-service` for this operation is dropped — which happens when the worker
/// thread finishes the operation (or, if the request never reached a worker at all, when the
/// handler below returns and drops its own sender).
///
/// This per-operation thread is the one deliberate exception to the sidecar's otherwise
/// single-threaded design: it does no git work, it only forwards a channel to stdout. The lock is
/// taken for the whole write-plus-flush of one line, so notifications can never interleave with
/// the dispatch loop's responses mid-line.
pub(super) fn spawn_progress_relay<W: std::io::Write + Send + 'static>(
    event_rx: mpsc::Receiver<TransferEvent>,
    out: Arc<Mutex<W>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for event in event_rx {
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "transferProgress",
                "params": transfer_progress_dto(event),
            });
            let Ok(line) = serde_json::to_string(&notification) else {
                continue;
            };
            let mut out = out.lock().unwrap_or_else(|error| error.into_inner());
            if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                // The reading end has gone away; stop relaying rather than spinning on a broken
                // pipe. The main loop notices the same failure and exits the process.
                return;
            }
        }
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FetchRemoteParams {
    repo_path: String,
    remote_name: String,
    /// `null`/absent = git config decides (`remote.<name>.prune`, then `fetch.prune`).
    #[serde(default)]
    prune: Option<bool>,
}

pub(super) fn fetch_remote(
    params: FetchRemoteParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.fetch_remote(
        params.remote_name,
        params.prune,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FetchAllRemotesParams {
    repo_path: String,
    #[serde(default)]
    prune: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BackgroundFetchParams {
    repo_path: String,
    #[serde(default)]
    prune: Option<bool>,
    /// The "Fetch automatically" interval, which an unreachable remote's backoff doubles.
    interval_minutes: u32,
}

pub(super) fn fetch_all_remotes(
    params: FetchAllRemotesParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id =
        worker_handle(repos, &params.repo_path)?.fetch_all_remotes(params.prune, event_tx)?;
    Ok(Value::String(operation_id))
}

/// The "Fetch automatically" tick; `null` when the worker had another operation queued or
/// running and nothing was started (see `WorkerHandle::background_fetch`).
pub(super) fn background_fetch(
    params: BackgroundFetchParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.background_fetch(
        params.prune,
        params.interval_minutes,
        event_tx,
    )?;
    Ok(operation_id.map_or(Value::Null, Value::String))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PushBranchParams {
    repo_path: String,
    remote_name: String,
    local_branch: String,
    remote_branch: String,
    set_upstream: bool,
    #[serde(default)]
    no_verify: bool,
}

pub(super) fn push_branch(
    params: PushBranchParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.push_branch(
        params.remote_name,
        params.local_branch,
        params.remote_branch,
        params.set_upstream,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeleteRemoteBranchParams {
    repo_path: String,
    remote_name: String,
    branch_name: String,
    #[serde(default)]
    no_verify: bool,
}

pub(super) fn delete_remote_branch(
    params: DeleteRemoteBranchParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.delete_remote_branch(
        params.remote_name,
        params.branch_name,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DeleteRemoteTagParams {
    repo_path: String,
    remote_name: String,
    tag_name: String,
    #[serde(default)]
    no_verify: bool,
}

pub(super) fn delete_remote_tag(
    params: DeleteRemoteTagParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.delete_remote_tag(
        params.remote_name,
        params.tag_name,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CancelTransferParams {
    repo_path: String,
    operation_id: String,
}

/// Asks the worker to abort the in-flight transfer `operationId`.
///
/// Takes no `stdout`: it spawns no progress relay of its own, because the cancellation surfaces
/// on the relay the original fetch/push/pull already started — as that operation's terminal
/// `Completed` notification, carrying `errorKind: "Cancelled"`. Unknown or already-finished IDs
/// are a no-op, so the extension can fire this without racing the transfer it is cancelling.
pub(super) fn cancel_transfer(
    params: CancelTransferParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.cancel_transfer(&params.operation_id);
    Ok(Value::Null)
}

/// Params of a current-branch push; `noVerify` (skip `pre-push`) defaults to `false`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PushParams {
    repo_path: String,
    remote_name: String,
    #[serde(default)]
    no_verify: bool,
}

pub(super) fn push_current_branch(
    params: PushParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.push_current_branch(
        params.remote_name,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

pub(super) fn force_push_current_branch(
    params: PushParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.force_push_current_branch(
        params.remote_name,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PushTagsParams {
    repo_path: String,
    remote_name: String,
    names: Vec<String>,
    #[serde(default)]
    no_verify: bool,
}

pub(super) fn push_tags(
    params: PushTagsParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.push_tags(
        params.remote_name,
        params.names,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

pub(super) fn pull_current_upstream(
    params: RepoPathParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.pull_current_upstream(event_tx)?;
    serde_json::to_value(PullOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}
