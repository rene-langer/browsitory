use super::{
    worker_handle, AppState, ConflictSegmentDto, FileConflictChoiceDto, MergeOptionsDto,
    MergeOutcomeDto,
};
use std::path::Path;
use tauri::State;
/// `branch_name` is a local branch name or a full ref (`refs/remotes/...` from the pull dialog).
#[tauri::command]
pub async fn start_merge(
    repo_path: String,
    branch_name: String,
    options: MergeOptionsDto,
    state: State<'_, AppState>,
) -> Result<MergeOutcomeDto, String> {
    Ok(MergeOutcomeDto::from(
        worker_handle(&state, &repo_path)?.start_merge(branch_name, options.into())?,
    ))
}
/// The merge dialog's preselected options: the choice remembered for this repository, else the
/// default from its `merge.ff`.
#[tauri::command]
pub async fn get_merge_options(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<MergeOptionsDto, String> {
    if let Some(preference) =
        config::get_merge_preference(Path::new(&repo_path)).map_err(|error| error.to_string())?
    {
        return Ok(MergeOptionsDto::from(preference));
    }
    Ok(MergeOptionsDto::from(
        worker_handle(&state, &repo_path)?.merge_defaults()?,
    ))
}
#[tauri::command]
pub async fn set_merge_options(repo_path: String, options: MergeOptionsDto) -> Result<(), String> {
    config::set_merge_preference(
        Path::new(&repo_path),
        options.fast_forward.into(),
        options.squash,
    )
    .map_err(|error| error.to_string())
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
