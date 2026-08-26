use tauri::State;

use super::{worker_handle, AppState, ReflogEntryDto};

#[tauri::command]
pub async fn list_reflog_refs(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    worker_handle(&state, &repo_path)?.list_reflog_refs()
}

#[tauri::command]
pub async fn get_reflog(
    repo_path: String,
    reference: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReflogEntryDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_reflog(reference)?
        .into_iter()
        .map(ReflogEntryDto::from)
        .collect())
}

#[tauri::command]
pub async fn restore_reflog_entry(
    repo_path: String,
    reference: String,
    new_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.restore_reflog_entry(reference, new_id)
}
