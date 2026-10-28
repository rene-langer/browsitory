use super::{
    emit_hook_events, worker_handle, AppState, CommitOutcomeDto, HeadInfoDto, PickOutcomeDto,
    ResetModeDto, SequenceAbortDto, SequenceOutcomeDto,
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
        worker_handle(&state, &repo_path)?.amend_commit(
            message,
            no_verify,
            None,
            Vec::new(),
            event_tx,
        )?,
    ))
}

#[tauri::command]
pub async fn cherry_pick(
    repo_path: String,
    commit_id: String,
    mainline: Option<u32>,
    state: State<'_, AppState>,
) -> Result<PickOutcomeDto, String> {
    Ok(PickOutcomeDto::from(
        worker_handle(&state, &repo_path)?.cherry_pick(commit_id, mainline)?,
    ))
}

#[tauri::command]
pub async fn revert_commit(
    repo_path: String,
    commit_id: String,
    mainline: Option<u32>,
    state: State<'_, AppState>,
) -> Result<PickOutcomeDto, String> {
    Ok(PickOutcomeDto::from(
        worker_handle(&state, &repo_path)?.revert_commit(commit_id, mainline)?,
    ))
}

/// `commit_ids` oldest-first; a conflict pauses the sequence until continue/abort.
#[tauri::command]
pub async fn cherry_pick_many(
    repo_path: String,
    commit_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<SequenceOutcomeDto, String> {
    Ok(SequenceOutcomeDto::from(
        worker_handle(&state, &repo_path)?.cherry_pick_many(commit_ids)?,
    ))
}

/// `commit_ids` oldest-first; they are reverted newest-first.
#[tauri::command]
pub async fn revert_many(
    repo_path: String,
    commit_ids: Vec<String>,
    state: State<'_, AppState>,
) -> Result<SequenceOutcomeDto, String> {
    Ok(SequenceOutcomeDto::from(
        worker_handle(&state, &repo_path)?.revert_many(commit_ids)?,
    ))
}

#[tauri::command]
pub async fn continue_sequence(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<SequenceOutcomeDto, String> {
    Ok(SequenceOutcomeDto::from(
        worker_handle(&state, &repo_path)?.continue_sequence()?,
    ))
}

#[tauri::command]
pub async fn abort_sequence(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<SequenceAbortDto, String> {
    Ok(SequenceAbortDto::from(
        worker_handle(&state, &repo_path)?.abort_sequence()?,
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
