use tauri::State;

use super::{repo_worker, AppState, SubmoduleInfoDto};

#[tauri::command]
pub async fn list_submodules(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<SubmoduleInfoDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.list_submodules())
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.init_submodule(path))
        .await
}

#[tauri::command]
pub async fn update_submodule(
    repo_path: String,
    path: String,
    recursive: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.update_submodule(path, recursive))
        .await
}
