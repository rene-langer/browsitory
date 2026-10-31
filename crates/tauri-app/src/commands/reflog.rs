use tauri::State;

use super::{repo_worker, AppState, ReflogEntryDto};

#[tauri::command]
pub async fn list_reflog_refs(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.list_reflog_refs())
        .await
}

#[tauri::command]
pub async fn get_reflog(
    repo_path: String,
    reference: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReflogEntryDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_reflog(reference))
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.restore_reflog_entry(reference, new_id))
        .await
}
