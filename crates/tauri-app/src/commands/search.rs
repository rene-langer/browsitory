use std::future::Future;

use git_core::search::SearchPage;
use tauri::State;

use super::{repo_worker, AppState, SearchPageDto};

#[tauri::command]
pub async fn search_commits(
    repo_path: String,
    query: String,
    refs: Option<Vec<String>>,
    cursor: Option<String>,
    state: State<'_, AppState>,
) -> Result<SearchPageDto, String> {
    Ok(queue_search(&state, &repo_path, query, refs, cursor)?
        .await?
        .into())
}

/// Claims the search's generation at once, before the call waits for the repository's gate, so
/// a cancel or a newer search also cancels a search still waiting there (#181).
pub(crate) fn queue_search(
    state: &AppState,
    repo_path: &str,
    query: String,
    refs: Option<Vec<String>>,
    cursor: Option<String>,
) -> Result<impl Future<Output = Result<SearchPage, String>> + Send + 'static, String> {
    let worker = repo_worker(state, repo_path)?;
    let generation = worker.signal().claim_search_generation();
    Ok(worker.call(move |w| w.search_commits_claimed(generation, query, refs, cursor)))
}

/// Stops the repo's in-flight search, if any. Never queued behind it — see
/// `WorkerHandle::cancel_search`.
#[tauri::command]
pub async fn cancel_search(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    repo_worker(&state, &repo_path)?.signal().cancel_search();
    Ok(())
}
