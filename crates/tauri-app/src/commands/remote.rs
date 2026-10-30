#[tauri::command]
pub async fn list_remotes(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<RemoteInfoDto>, String> {
    repo_worker(&state, &repo_path)?
        .call(move |worker| {
            worker
                .list_remotes()?
                .into_iter()
                .map(|remote| {
                    let profile = worker.get_remote_auth_mode(remote.name.clone())?;
                    Ok(RemoteInfoDto::from((remote, profile)))
                })
                .collect()
        })
        .await
}

use std::sync::mpsc;

use tauri::{AppHandle, State};

use super::{
    emit_transfer_events, repo_worker, AppState, PullOutcomeDto, RemoteAuthModeDto, RemoteInfoDto,
    UpstreamInfoDto,
};

#[tauri::command]
pub async fn list_remote_branches(
    repo_path: String,
    remote_name: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.list_remote_branches(remote_name))
        .await
}

#[tauri::command]
pub async fn get_current_upstream(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Option<UpstreamInfoDto>, String> {
    let upstream = repo_worker(&state, &repo_path)?
        .call(move |w| w.get_current_upstream())
        .await?;
    Ok(upstream.map(UpstreamInfoDto::from))
}
#[tauri::command]
pub async fn get_remote_upstreams(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<UpstreamInfoDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_remote_upstreams(name))
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.add_remote(name, fetch_url, push_url))
        .await
}

#[tauri::command]
pub async fn rename_remote(
    repo_path: String,
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.rename_remote(old_name, new_name))
        .await
}

#[tauri::command]
pub async fn update_remote_urls(
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.update_remote_urls(name, fetch_url, push_url))
        .await
}

#[tauri::command]
pub async fn remove_remote(
    repo_path: String,
    name: String,
    clear_upstreams: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.remove_remote(name, clear_upstreams))
        .await
}

#[tauri::command]
pub async fn save_https_credential(
    repo_path: String,
    remote_name: String,
    username: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.save_https_credential(remote_name, username, token))
        .await
}

#[tauri::command]
pub async fn forget_https_credential(
    repo_path: String,
    remote_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.forget_https_credential(remote_name))
        .await
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.set_remote_auth_mode(remote_name, mode))
        .await
}

#[tauri::command]
pub async fn set_current_upstream(
    repo_path: String,
    remote_name: String,
    remote_branch: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.set_current_upstream(remote_name, remote_branch))
        .await
}

#[tauri::command]
pub async fn clear_current_upstream(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.clear_current_upstream())
        .await
}

#[tauri::command]
pub async fn fetch_remote(
    repo_path: String,
    remote_name: String,
    prune: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = repo_worker(&state, &repo_path)?
        .call(move |w| w.fetch_remote(remote_name, prune, event_tx))
        .await?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn fetch_all_remotes(
    repo_path: String,
    prune: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = repo_worker(&state, &repo_path)?
        .call(move |w| w.fetch_all_remotes(prune, event_tx))
        .await?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn push_branch(
    repo_path: String,
    remote_name: String,
    local_branch: String,
    remote_branch: String,
    set_upstream: bool,
    no_verify: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = repo_worker(&state, &repo_path)?
        .call(move |w| {
            w.push_branch(
                remote_name,
                local_branch,
                remote_branch,
                set_upstream,
                no_verify,
                event_tx,
            )
        })
        .await?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn delete_remote_branch(
    repo_path: String,
    remote_name: String,
    branch_name: String,
    no_verify: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = repo_worker(&state, &repo_path)?
        .call(move |w| w.delete_remote_branch(remote_name, branch_name, no_verify, event_tx))
        .await?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

/// Asks the worker to abort the in-flight transfer `operation_id`.
///
/// Deliberately does no channel round trip: `WorkerHandle::cancel_transfer` writes into the
/// worker's shared cancel registry, which is the only route that reaches a worker thread already
/// blocked inside a `git2` transfer. Unknown or already-finished IDs are a no-op, so the UI can
/// fire this without first checking whether the transfer is still running.
#[tauri::command]
pub async fn cancel_transfer(
    repo_path: String,
    operation_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .signal()
        .cancel_transfer(&operation_id);
    Ok(())
}

#[tauri::command]
pub async fn push_current_branch(
    repo_path: String,
    remote_name: String,
    no_verify: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = repo_worker(&state, &repo_path)?
        .call(move |w| w.push_current_branch(remote_name, no_verify, event_tx))
        .await?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn force_push_current_branch(
    repo_path: String,
    remote_name: String,
    no_verify: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = repo_worker(&state, &repo_path)?
        .call(move |w| w.force_push_current_branch(remote_name, no_verify, event_tx))
        .await?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn push_tags(
    repo_path: String,
    remote_name: String,
    names: Vec<String>,
    no_verify: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = repo_worker(&state, &repo_path)?
        .call(move |w| w.push_tags(remote_name, names, no_verify, event_tx))
        .await?;
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
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.pull_current_upstream(event_tx))
        .await?
        .into())
}
