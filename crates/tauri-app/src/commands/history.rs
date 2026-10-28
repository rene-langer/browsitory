use super::{
    emit_hook_events, worker_handle, AppState, CommitOutcomeDto, HeadInfoDto, PickOutcomeDto,
    ResetModeDto,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_head_info(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<HeadInfoDto, String> {
    Ok(HeadInfoDto::from(
        worker_handle(&state, &repo_path)?.get_head_info()?,
    ))
}

#[tauri::command]
pub async fn amend_commit(
    repo_path: String,
    message: String,
    no_verify: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CommitOutcomeDto, String> {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    emit_hook_events(app, repo_path.clone(), event_rx);
    Ok(CommitOutcomeDto::from(
        worker_handle(&state, &repo_path)?.amend_commit(message, no_verify, event_tx)?,
    ))
}

#[tauri::command]
pub async fn cherry_pick(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<PickOutcomeDto, String> {
    Ok(PickOutcomeDto::from(
        worker_handle(&state, &repo_path)?.cherry_pick(commit_id, None)?,
    ))
}

#[tauri::command]
pub async fn revert_commit(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<PickOutcomeDto, String> {
    Ok(PickOutcomeDto::from(
        worker_handle(&state, &repo_path)?.revert_commit(commit_id, None)?,
    ))
}

#[tauri::command]
pub async fn reset_to_commit(
    repo_path: String,
    commit_id: String,
    mode: ResetModeDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.reset_to_commit(commit_id, mode.into())
}

#[tauri::command]
pub async fn checkout_commit(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.checkout_commit(commit_id)
}
