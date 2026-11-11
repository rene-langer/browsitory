use super::{hook_event_channel, repo_worker, AppState};
use repo_service::wire::ErrorDto;
use repo_service::wire::{
    HistoryEditTargetDto, RebaseAbortDto, RebasePlanCommitDto, RebasePlanEntryDto,
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

/// The signing program as for `start_rebase`.
#[tauri::command]
pub async fn rebase_continue(
    repo_path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, ErrorDto> {
    let events = hook_event_channel(app, &repo_path);
    Ok(RebaseStepResultDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.rebase_continue_reporting(events))
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
