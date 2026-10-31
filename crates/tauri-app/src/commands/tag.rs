use tauri::State;

use super::{repo_worker, AppState, TagInfoDto};

#[tauri::command]
pub async fn list_tags(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<TagInfoDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.list_tags())
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.create_tag(name, message))
        .await
}

#[tauri::command]
pub async fn delete_tag(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.delete_tag(name))
        .await
}
