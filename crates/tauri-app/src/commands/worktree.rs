use std::path::PathBuf;

use tauri::State;

use super::{repo_worker, AppState, WorktreeInfoDto};

#[tauri::command]
pub async fn list_worktrees(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<WorktreeInfoDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.list_worktrees())
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.create_worktree(name, PathBuf::from(path), branch, start_point))
        .await
}

#[tauri::command]
pub async fn remove_worktree(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.remove_worktree(name))
        .await
}

#[tauri::command]
pub async fn prune_worktrees(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.prune_worktrees())
        .await
}
