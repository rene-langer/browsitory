use tauri::State;

use super::{worker_handle, AppState, StashEntryDto};

#[tauri::command]
pub async fn list_stashes(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<StashEntryDto>, String> {
    let stashes = worker_handle(&state, &repo_path)?.list_stashes()?;
    Ok(stashes.into_iter().map(StashEntryDto::from).collect())
}

#[tauri::command]
pub async fn save_stash(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.save_stash()
}

#[tauri::command]
pub async fn apply_stash(
    repo_path: String,
    index: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.apply_stash(index)
}

#[tauri::command]
pub async fn drop_stash(
    repo_path: String,
    index: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.drop_stash(index)
}
