use super::{
    worker_handle, AppState, RebasePlanCommitDto, RebasePlanEntryDto, RebaseProgressDto,
    RebaseStepResultDto,
};
use tauri::State;

#[tauri::command]
pub async fn commits_since(
    repo_path: String,
    onto: String,
    state: State<'_, AppState>,
) -> Result<Vec<RebasePlanCommitDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .commits_since(onto)?
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
        worker_handle(&state, &repo_path)?
            .start_rebase(onto, plan.into_iter().map(Into::into).collect())?,
    ))
}

#[tauri::command]
pub async fn rebase_continue(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, String> {
    Ok(RebaseStepResultDto::from(
        worker_handle(&state, &repo_path)?.rebase_continue()?,
    ))
}

#[tauri::command]
pub async fn abort_rebase(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.abort_rebase()
}

#[tauri::command]
pub async fn get_rebase_progress(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Option<RebaseProgressDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_rebase_progress()?
        .map(|(current_step, total_steps)| RebaseProgressDto {
            current_step,
            total_steps,
        }))
}
