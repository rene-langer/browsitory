use super::{worker_handle, AppState, BranchInfoDto};
use tauri::State;
#[tauri::command]
pub async fn list_branches(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BranchInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .list_branches()?
        .into_iter()
        .map(BranchInfoDto::from)
        .collect())
}
#[tauri::command]
pub async fn create_branch(
    repo_path: String,
    name: String,
    start_point: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.create_branch(name, start_point)
}
#[tauri::command]
pub async fn switch_branch(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.switch_branch(name)
}
#[tauri::command]
pub async fn delete_branch(
    repo_path: String,
    name: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.delete_branch(name, force)
}
#[tauri::command]
pub async fn rename_branch(
    repo_path: String,
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.rename_branch(old_name, new_name)
}
