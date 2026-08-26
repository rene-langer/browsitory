use std::path::PathBuf;

use tauri::State;

use super::{worker_handle, AppState, WorktreeInfoDto};

#[tauri::command]
pub async fn list_worktrees(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<WorktreeInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .list_worktrees()?
        .into_iter()
        .map(WorktreeInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn create_worktree(
    repo_path: String,
    name: String,
    path: String,
    branch: String,
    start_point: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.create_worktree(
        name,
        PathBuf::from(path),
        branch,
        start_point,
    )
}

#[tauri::command]
pub async fn remove_worktree(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.remove_worktree(name)
}

#[tauri::command]
pub async fn prune_worktrees(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.prune_worktrees()
}
