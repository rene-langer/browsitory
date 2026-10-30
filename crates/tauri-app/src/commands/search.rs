use super::{repo_worker, AppState, SearchPageDto};
use tauri::State;

#[tauri::command]
pub async fn search_commits(
    repo_path: String,
    query: String,
    refs: Option<Vec<String>>,
    cursor: Option<String>,
    state: State<'_, AppState>,
) -> Result<SearchPageDto, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.search_commits(query, refs, cursor))
        .await?
        .into())
}

/// Stops the repo's in-flight search, if any. Never queued behind it — see
/// `WorkerHandle::cancel_search`.
#[tauri::command]
pub async fn cancel_search(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    repo_worker(&state, &repo_path)?.signal().cancel_search();
    Ok(())
}
