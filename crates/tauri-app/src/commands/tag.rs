use repo_service::wire::ErrorDto;
use tauri::State;

use super::{repo_worker, AppState};
use repo_service::wire::TagInfoDto;

#[tauri::command]
pub async fn list_tags(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<TagInfoDto>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .read(move |w| w.list_tags())
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
    target: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.create_tag(name, message, target))
        .await
}

#[tauri::command]
pub async fn delete_tag(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.delete_tag(name))
        .await
}
