use tauri::State;

use super::{worker_handle, AppState, SubmoduleInfoDto};

#[tauri::command]
pub async fn list_submodules(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<SubmoduleInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .list_submodules()?
        .into_iter()
        .map(SubmoduleInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn init_submodule(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.init_submodule(path)
}

#[tauri::command]
pub async fn update_submodule(
    repo_path: String,
    path: String,
    recursive: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.update_submodule(path, recursive)
}
