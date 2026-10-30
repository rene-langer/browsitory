use super::{repo_worker, AppState, BranchInfoDto};
use tauri::State;
#[tauri::command]
pub async fn list_branches(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BranchInfoDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.list_branches())
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.create_branch(name, start_point))
        .await
}
#[tauri::command]
pub async fn switch_branch(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.switch_branch(name))
        .await
}
#[tauri::command]
pub async fn delete_branch(
    repo_path: String,
    name: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.delete_branch(name, force))
        .await
}
#[tauri::command]
pub async fn rename_branch(
    repo_path: String,
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.rename_branch(old_name, new_name))
        .await
}
