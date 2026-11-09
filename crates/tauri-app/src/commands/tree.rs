//! A commit's file tree (#446): browse it, read a file at that commit, save it elsewhere, restore
//! it to the work tree.

use super::{repo_worker, AppState};
use repo_service::wire::{ErrorDto, FileContentDto, TreeListingDto};
use tauri::State;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn get_commit_tree(
    repo_path: String,
    commit_id: String,
    dir: String,
    filter: Option<String>,
    state: State<'_, AppState>,
) -> Result<TreeListingDto, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_tree(commit_id, dir, filter))
        .await?
        .into())
}

#[tauri::command]
pub async fn get_file_at_commit(
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<FileContentDto, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_file_at_commit(commit_id, path))
        .await?
        .into())
}

#[tauri::command]
pub async fn restore_file_version(
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.restore_file_version(commit_id, path))
        .await
}

/// Asks where to save `path` as it is in `commit_id` (the file's name is suggested), then writes
/// it there. `None` when the user cancels the dialog; otherwise the path written.
#[tauri::command]
pub async fn save_file_version(
    app: tauri::AppHandle,
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, ErrorDto> {
    let worker = repo_worker(&state, &repo_path)?;
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    let Some(picked) = app.dialog().file().set_file_name(name).blocking_save_file() else {
        return Ok(None);
    };
    let destination = picked
        .into_path()
        .map_err(|error| ErrorDto::from(error.to_string()))?;
    let shown = destination.display().to_string();
    worker
        .call(move |w| w.save_file_version(commit_id, path, destination))
        .await?;
    Ok(Some(shown))
}
