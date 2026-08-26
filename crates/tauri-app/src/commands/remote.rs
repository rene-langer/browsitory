#[tauri::command]
pub async fn list_remotes(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<RemoteInfoDto>, String> {
    let worker = worker_handle(&state, &repo_path)?;
    worker
        .list_remotes()?
        .into_iter()
        .map(|remote| {
            let profile = worker.get_remote_auth_mode(remote.name.clone())?;
            Ok(RemoteInfoDto::from((remote, profile)))
        })
        .collect()
}

use std::sync::mpsc;

use tauri::{AppHandle, State};

use super::{
    emit_transfer_events, worker_handle, AppState, PullOutcomeDto, RemoteAuthModeDto,
    RemoteInfoDto, UpstreamInfoDto,
};

#[tauri::command]
pub async fn list_remote_branches(
    repo_path: String,
    remote_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    worker_handle(&state, &repo_path)?.list_remote_branches(remote_name)
}

#[tauri::command]
pub async fn get_current_upstream(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Option<UpstreamInfoDto>, String> {
    let upstream = worker_handle(&state, &repo_path)?.get_current_upstream()?;
    Ok(upstream.map(UpstreamInfoDto::from))
}
#[tauri::command]
pub async fn get_remote_upstreams(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<UpstreamInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_remote_upstreams(name)?
        .into_iter()
        .map(UpstreamInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn add_remote(
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.add_remote(name, fetch_url, push_url)
}

#[tauri::command]
pub async fn rename_remote(
    repo_path: String,
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.rename_remote(old_name, new_name)
}

#[tauri::command]
pub async fn update_remote_urls(
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.update_remote_urls(name, fetch_url, push_url)
}

#[tauri::command]
pub async fn remove_remote(
    repo_path: String,
    name: String,
    clear_upstreams: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.remove_remote(name, clear_upstreams)
}

#[tauri::command]
pub async fn save_https_credential(
    repo_path: String,
    remote_name: String,
    username: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.save_https_credential(remote_name, username, token)
}

#[tauri::command]
pub async fn forget_https_credential(
    repo_path: String,
    remote_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.forget_https_credential(remote_name)
}

#[tauri::command]
pub async fn set_remote_auth_mode(
    repo_path: String,
    remote_name: String,
    mode: RemoteAuthModeDto,
    username: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mode = match mode {
        RemoteAuthModeDto::HttpsToken => git_core::remote::RemoteAuthMode::HttpsToken {
            username: username
                .filter(|username| !username.trim().is_empty())
                .ok_or_else(|| "HTTPS username is required".to_string())?,
        },
        RemoteAuthModeDto::SshAgent => git_core::remote::RemoteAuthMode::SshAgent,
    };
    worker_handle(&state, &repo_path)?.set_remote_auth_mode(remote_name, mode)
}

#[tauri::command]
pub async fn set_current_upstream(
    repo_path: String,
    remote_name: String,
    remote_branch: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.set_current_upstream(remote_name, remote_branch)
}

#[tauri::command]
pub async fn clear_current_upstream(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.clear_current_upstream()
}

#[tauri::command]
pub async fn fetch_remote(
    repo_path: String,
    remote_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state, &repo_path)?.fetch_remote(remote_name, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn push_current_branch(
    repo_path: String,
    remote_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id =
        worker_handle(&state, &repo_path)?.push_current_branch(remote_name, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn push_tags(
    repo_path: String,
    remote_name: String,
    names: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id =
        worker_handle(&state, &repo_path)?.push_tags(remote_name, names, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn pull_current_upstream(
    repo_path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PullOutcomeDto, String> {
    let (event_tx, event_rx) = mpsc::channel();
    emit_transfer_events(app, event_rx);
    let handle = worker_handle(&state, &repo_path)?;
    Ok(
        tauri::async_runtime::spawn_blocking(move || handle.pull_current_upstream(event_tx))
            .await
            .map_err(|_| "pull worker task stopped".to_string())??
            .into(),
    )
}
