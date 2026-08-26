use super::{worker_handle, AppState, ConflictSegmentDto, FileConflictChoiceDto, MergeOutcomeDto};
use tauri::State;
#[tauri::command]
pub async fn start_merge(
    repo_path: String,
    branch_name: String,
    state: State<'_, AppState>,
) -> Result<MergeOutcomeDto, String> {
    Ok(MergeOutcomeDto::from(
        worker_handle(&state, &repo_path)?.start_merge(branch_name)?,
    ))
}
#[tauri::command]
pub async fn get_conflict_hunks(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ConflictSegmentDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_conflict_hunks(path)?
        .into_iter()
        .map(ConflictSegmentDto::from)
        .collect())
}
#[tauri::command]
pub async fn resolve_conflict(
    repo_path: String,
    path: String,
    resolved_content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.resolve_conflict(path, resolved_content)
}
#[tauri::command]
pub async fn abort_merge(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.abort_merge()
}
#[tauri::command]
pub async fn get_merge_message(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    worker_handle(&state, &repo_path)?.get_merge_message()
}
#[tauri::command]
pub async fn resolve_add_delete_conflict(
    repo_path: String,
    path: String,
    choice: FileConflictChoiceDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.resolve_add_delete_conflict(path, choice.into())
}
