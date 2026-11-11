use super::{hook_event_channel, repo_worker, AppState};
use repo_service::wire::BranchInfoDto;
use repo_service::wire::ErrorDto;
use repo_service::wire::StashSwitchOutcomeDto;
use tauri::{AppHandle, State};
#[tauri::command]
pub async fn list_branches(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BranchInfoDto>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .read(move |w| w.list_branches())
        .await?
        .into_iter()
        .map(BranchInfoDto::from)
        .collect())
}
/// With `checkout` (absent: true, as before #656) it switches to the new branch too.
#[tauri::command]
pub async fn create_branch(
    repo_path: String,
    name: String,
    start_point: String,
    checkout: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let checkout = checkout.unwrap_or(true);
    repo_worker(&state, &repo_path)?
        .call(move |w| w.create_branch(name, start_point, checkout))
        .await
}
/// The files whose local changes keep a switch to `name` from running (#653).
#[tauri::command]
pub async fn get_switch_conflicts(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, ErrorDto> {
    repo_worker(&state, &repo_path)?
        .read(move |w| w.switch_conflicts(name))
        .await
}
/// Stash and switch (#653); `post-checkout` is reported as for `switch_branch`.
#[tauri::command]
pub async fn stash_and_switch(
    repo_path: String,
    name: String,
    reapply: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<StashSwitchOutcomeDto, ErrorDto> {
    let events = hook_event_channel(app, &repo_path);
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.stash_and_switch(name, reapply, events))
        .await?
        .into())
}
/// `post-checkout` is reported as `hook-progress` events with `source: "Checkout"` (#185).
#[tauri::command]
pub async fn switch_branch(
    repo_path: String,
    name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let events = hook_event_channel(app, &repo_path);
    repo_worker(&state, &repo_path)?
        .call(move |w| w.switch_branch(name, events))
        .await
}
#[tauri::command]
pub async fn delete_branch(
    repo_path: String,
    name: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.delete_branch(name, force))
        .await
}
#[tauri::command]
pub async fn rename_branch(
    repo_path: String,
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.rename_branch(old_name, new_name))
        .await
}
