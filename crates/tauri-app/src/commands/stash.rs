use tauri::State;

use super::{
    worker_handle, AppState, StashApplyOutcomeDto, StashEntryDto, StashFilesDto, StashOptionsDto,
};

#[tauri::command]
pub async fn list_stashes(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<StashEntryDto>, String> {
    let stashes = worker_handle(&state, &repo_path)?.list_stashes()?;
    Ok(stashes.into_iter().map(StashEntryDto::from).collect())
}

#[tauri::command]
pub async fn save_stash(
    repo_path: String,
    options: StashOptionsDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.save_stash(options.into())
}

#[tauri::command]
pub async fn apply_stash(
    repo_path: String,
    index: usize,
    restore_index: bool,
    state: State<'_, AppState>,
) -> Result<StashApplyOutcomeDto, String> {
    Ok(worker_handle(&state, &repo_path)?
        .apply_stash(index, restore_index)?
        .into())
}

#[tauri::command]
pub async fn pop_stash(
    repo_path: String,
    index: usize,
    restore_index: bool,
    state: State<'_, AppState>,
) -> Result<StashApplyOutcomeDto, String> {
    Ok(worker_handle(&state, &repo_path)?
        .pop_stash(index, restore_index)?
        .into())
}

#[tauri::command]
pub async fn get_stash_files(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<StashFilesDto, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_stash_files(commit_id)?
        .into())
}

#[tauri::command]
pub async fn drop_stash(
    repo_path: String,
    index: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.drop_stash(index)
}
