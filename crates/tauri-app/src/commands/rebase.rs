use super::{repo_worker, AppState};
use repo_service::wire::ErrorDto;
use repo_service::wire::{
    RebaseAbortDto, RebasePlanCommitDto, RebasePlanEntryDto, RebaseStepResultDto,
};
use tauri::State;

#[tauri::command]
pub async fn commits_since(
    repo_path: String,
    onto: String,
    state: State<'_, AppState>,
) -> Result<Vec<RebasePlanCommitDto>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.commits_since(onto))
        .await?
        .into_iter()
        .map(RebasePlanCommitDto::from)
        .collect())
}

#[tauri::command]
pub async fn start_rebase(
    repo_path: String,
    onto: String,
    plan: Vec<RebasePlanEntryDto>,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, ErrorDto> {
    Ok(RebaseStepResultDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.start_rebase(onto, plan.into_iter().map(Into::into).collect()))
            .await?,
    ))
}

#[tauri::command]
pub async fn rebase_continue(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, ErrorDto> {
    Ok(RebaseStepResultDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.rebase_continue())
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
