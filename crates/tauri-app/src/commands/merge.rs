use super::{
    hook_event_channel, repo_worker, AppState, ConflictSegmentDto, ConflictSideDto,
    FileConflictChoiceDto, MarkResolvedOutcomeDto, MergeOptionsDto, MergeOutcomeDto,
};
use std::path::Path;
use tauri::{AppHandle, State};
/// `branch_name` is a local branch name or a full ref (`refs/remotes/...` from the pull dialog).
/// A fast-forward's or squash's `post-merge` is reported as `hook-progress` events with
/// `source: "Merge"` (#185).
#[tauri::command]
pub async fn start_merge(
    repo_path: String,
    branch_name: String,
    options: MergeOptionsDto,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<MergeOutcomeDto, String> {
    let events = hook_event_channel(app, &repo_path);
    Ok(MergeOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.start_merge(branch_name, options.into(), events))
            .await?,
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
        repo_worker(&state, &repo_path)?
            .call(move |w| w.merge_defaults())
            .await?,
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
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_conflict_hunks(path))
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.resolve_conflict(path, resolved_content))
        .await
}
#[tauri::command]
pub async fn abort_merge(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.abort_merge())
        .await
}
#[tauri::command]
pub async fn get_merge_message(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.get_merge_message())
        .await
}
#[tauri::command]
pub async fn resolve_add_delete_conflict(
    repo_path: String,
    path: String,
    choice: FileConflictChoiceDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.resolve_add_delete_conflict(path, choice.into()))
        .await
}

/// Stages a conflicted file. Without `force`, a file that still has conflict markers is left
/// alone and `MarkersRemain` is returned, so the UI can ask first.
#[tauri::command]
pub async fn resolve_whole_file(
    repo_path: String,
    path: String,
    side: ConflictSideDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.resolve_whole_file(path, side.into()))
        .await
}

#[tauri::command]
pub async fn mark_resolved(
    repo_path: String,
    path: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<MarkResolvedOutcomeDto, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.mark_resolved(path, force))
        .await?
        .into())
}
