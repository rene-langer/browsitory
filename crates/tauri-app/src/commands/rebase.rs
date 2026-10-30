use super::{
    repo_worker, AppState, RebaseAbortDto, RebasePlanCommitDto, RebasePlanEntryDto,
    RebaseStepResultDto,
};
use tauri::State;

#[tauri::command]
pub async fn commits_since(
    repo_path: String,
    onto: String,
    state: State<'_, AppState>,
) -> Result<Vec<RebasePlanCommitDto>, String> {
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
) -> Result<RebaseStepResultDto, String> {
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
) -> Result<RebaseStepResultDto, String> {
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
) -> Result<RebaseAbortDto, String> {
    Ok(RebaseAbortDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.abort_rebase())
            .await?,
    ))
}
