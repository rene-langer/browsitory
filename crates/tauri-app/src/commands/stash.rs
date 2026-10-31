use tauri::State;

use super::{
    repo_worker, AppState, StashApplyOutcomeDto, StashEntryDto, StashFilesDto, StashOptionsDto,
};

#[tauri::command]
pub async fn list_stashes(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<StashEntryDto>, String> {
    let stashes = repo_worker(&state, &repo_path)?
        .call(move |w| w.list_stashes())
        .await?;
    Ok(stashes.into_iter().map(StashEntryDto::from).collect())
}

#[tauri::command]
pub async fn save_stash(
    repo_path: String,
    options: StashOptionsDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.save_stash(options.into()))
        .await
}

#[tauri::command]
pub async fn apply_stash(
    repo_path: String,
    index: usize,
    commit_id: String,
    restore_index: bool,
    state: State<'_, AppState>,
) -> Result<StashApplyOutcomeDto, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.apply_stash(index, commit_id, restore_index))
        .await?
        .into())
}

#[tauri::command]
pub async fn pop_stash(
    repo_path: String,
    index: usize,
    commit_id: String,
    restore_index: bool,
    state: State<'_, AppState>,
) -> Result<StashApplyOutcomeDto, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.pop_stash(index, commit_id, restore_index))
        .await?
        .into())
}

#[tauri::command]
pub async fn get_stash_files(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<StashFilesDto, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_stash_files(commit_id))
        .await?
        .into())
}

#[tauri::command]
pub async fn drop_stash(
    repo_path: String,
    index: usize,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.drop_stash(index, commit_id))
        .await
}
