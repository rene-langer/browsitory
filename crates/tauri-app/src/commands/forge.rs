use super::{
    repo_worker, AppState, CreatePullRequestDto, ForgeProviderDto, ForgeRepositoryDto,
    PullRequestDto, PullRequestListDto,
};
use tauri::State;

#[tauri::command]
pub async fn detect_forge_repository(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ForgeRepositoryDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.detect_forge_repository())
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.save_forge_token(provider.into(), account, token))
        .await
}

#[tauri::command]
pub async fn forget_forge_token(
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.forget_forge_token(provider.into(), account))
        .await
}

#[tauri::command]
pub async fn list_pull_requests(
    repo_path: String,
    remote_name: String,
    account: String,
    state: State<'_, AppState>,
) -> Result<PullRequestListDto, String> {
    Ok(PullRequestListDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.list_pull_requests(remote_name, account))
            .await?,
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
        repo_worker(&state, &repo_path)?
            .call(move |w| w.create_pull_request(remote_name, account, pull_request.into()))
            .await?,
    ))
}
