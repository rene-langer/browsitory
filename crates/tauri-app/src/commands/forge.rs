use super::{
    worker_handle, AppState, CreatePullRequestDto, ForgeProviderDto, ForgeRepositoryDto,
    PullRequestDto, PullRequestListDto,
};
use tauri::State;

#[tauri::command]
pub async fn detect_forge_repository(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ForgeRepositoryDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .detect_forge_repository()?
        .into_iter()
        .map(ForgeRepositoryDto::from)
        .collect())
}

#[tauri::command]
pub async fn save_forge_token(
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.save_forge_token(provider.into(), account, token)
}

#[tauri::command]
pub async fn forget_forge_token(
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.forget_forge_token(provider.into(), account)
}

#[tauri::command]
pub async fn list_pull_requests(
    repo_path: String,
    remote_name: String,
    account: String,
    state: State<'_, AppState>,
) -> Result<PullRequestListDto, String> {
    Ok(PullRequestListDto::from(
        worker_handle(&state, &repo_path)?.list_pull_requests(remote_name, account)?,
    ))
}

#[tauri::command]
pub async fn create_pull_request(
    repo_path: String,
    remote_name: String,
    account: String,
    pull_request: CreatePullRequestDto,
    state: State<'_, AppState>,
) -> Result<PullRequestDto, String> {
    Ok(PullRequestDto::from(
        worker_handle(&state, &repo_path)?.create_pull_request(
            remote_name,
            account,
            pull_request.into(),
        )?,
    ))
}
