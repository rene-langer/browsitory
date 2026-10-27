use super::{worker_handle, AppState, HeadInfoDto, PickOutcomeDto, ResetModeDto};
use tauri::State;

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
    state: State<'_, AppState>,
) -> Result<(), String> {
    match worker_handle(&state, &repo_path)?.amend_commit(
        message,
        false,
        std::sync::mpsc::channel().0,
    )? {
        git_core::commit::CommitOutcome::Committed { .. } => Ok(()),
        git_core::commit::CommitOutcome::HookFailed { hook, .. } => {
            Err(format!("The {hook} hook failed."))
        }
    }
}

#[tauri::command]
pub async fn cherry_pick(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<PickOutcomeDto, String> {
    Ok(PickOutcomeDto::from(
        worker_handle(&state, &repo_path)?.cherry_pick(commit_id)?,
    ))
}

#[tauri::command]
pub async fn revert_commit(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<PickOutcomeDto, String> {
    Ok(PickOutcomeDto::from(
        worker_handle(&state, &repo_path)?.revert_commit(commit_id)?,
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
