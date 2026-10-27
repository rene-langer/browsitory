use super::{worker_handle, AppState, SearchPageDto};
use tauri::State;

#[tauri::command]
pub async fn search_commits(
    repo_path: String,
    query: String,
    refs: Option<Vec<String>>,
    cursor: Option<String>,
    state: State<'_, AppState>,
) -> Result<SearchPageDto, String> {
    Ok(worker_handle(&state, &repo_path)?
        .search_commits(query, refs, cursor)?
        .into())
}

/// Stops the repo's in-flight search, if any. Never queued behind it — see
/// `WorkerHandle::cancel_search`.
#[tauri::command]
pub async fn cancel_search(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.cancel_search();
    Ok(())
}
