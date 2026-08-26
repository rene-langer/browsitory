use tauri::State;

use super::{worker_handle, AppState, TagInfoDto};

#[tauri::command]
pub async fn list_tags(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<TagInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .list_tags()?
        .into_iter()
        .map(TagInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn create_tag(
    repo_path: String,
    name: String,
    message: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.create_tag(name, message)
}

#[tauri::command]
pub async fn delete_tag(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.delete_tag(name)
}
