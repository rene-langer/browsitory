use super::{hook_event_channel, repo_worker, AppState};
use repo_service::wire::ErrorDto;
use repo_service::wire::{
    EditRestDto, HistoryEditTargetDto, RebaseAbortDto, RebasePlanCommitDto, RebasePlanEntryDto,
    RebaseStepResultDto,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn commits_since(
    repo_path: String,
    onto: String,
    state: State<'_, AppState>,
) -> Result<Vec<RebasePlanCommitDto>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .read(move |w| w.commits_since(onto))
        .await?
        .into_iter()
        .map(RebasePlanCommitDto::from)
        .collect())
}

/// A step's signing program is reported as a `hook-progress` event (`hook: "signing"`,
/// `source: "Pick"`), so the hook strip can cancel it (#151).
#[tauri::command]
pub async fn start_rebase(
    repo_path: String,
    onto: String,
    plan: Vec<RebasePlanEntryDto>,
    edit: Option<HistoryEditTargetDto>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, ErrorDto> {
    let plan = plan.into_iter().map(Into::into).collect();
    let edit = edit.map(Into::into);
    let events = hook_event_channel(app, &repo_path);
    Ok(RebaseStepResultDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.start_rebase_reporting(onto, plan, edit, events))
            .await?,
    ))
}

/// The signing program as for `start_rebase`. `rest`: what an edit stop's changes not staged
/// become (#884); without it, they refuse Continue (`RestLeft`).
#[tauri::command]
pub async fn rebase_continue(
    repo_path: String,
    message: Option<String>,
    rest: Option<EditRestDto>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, ErrorDto> {
    let events = hook_event_channel(app, &repo_path);
    let rest = rest.map(Into::into);
    Ok(RebaseStepResultDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.rebase_continue_with_rest_reporting(message, rest, events))
            .await?,
    ))
}

/// Split a commit (#884): commits what is staged at the edit stop as a part; returns its id.
/// The signing program as for `start_rebase`.
#[tauri::command]
pub async fn rebase_commit_part(
    repo_path: String,
    message: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, ErrorDto> {
    let events = hook_event_channel(app, &repo_path);
    repo_worker(&state, &repo_path)?
        .call(move |w| w.rebase_commit_part_reporting(message, events))
        .await
}

/// The edit stop's changes not staged (#884), sorted by path.
#[tauri::command]
pub async fn get_edit_rest(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.get_edit_rest())
        .await
}

/// Folds the staged changes into `commit_id` (#883); the signing program as for
/// `start_rebase`.
#[tauri::command]
pub async fn fold_staged_into(
    repo_path: String,
    commit_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, ErrorDto> {
    let events = hook_event_channel(app, &repo_path);
    Ok(RebaseStepResultDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.fold_staged_reporting(commit_id, events))
            .await?,
    ))
}

#[tauri::command]
pub async fn abort_rebase(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<RebaseAbortDto, ErrorDto> {
    Ok(RebaseAbortDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.abort_rebase())
            .await?,
    ))
}
