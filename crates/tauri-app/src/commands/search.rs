use repo_service::wire::ErrorDto;
use std::future::Future;

use git_core::graph::GraphSelection;
use git_core::search::SearchPage;
use tauri::State;

use super::{repo_worker, AppState};
use repo_service::wire::SearchPageDto;

#[tauri::command]
pub async fn search_commits(
    repo_path: String,
    query: String,
    refs: Option<Vec<String>>,
    remote_branches: Vec<String>,
    cursor: Option<String>,
    state: State<'_, AppState>,
) -> Result<SearchPageDto, ErrorDto> {
    let selection = GraphSelection {
        local: refs,
        remote: remote_branches,
    };
    Ok(queue_search(&state, &repo_path, query, selection, cursor)?
        .await?
        .into())
}

/// Claims the search's generation at once, before the call waits for the repository's gate, so
/// a cancel or a newer search also cancels a search still waiting there (#181).
pub(crate) fn queue_search(
    state: &AppState,
    repo_path: &str,
    query: String,
    selection: GraphSelection,
    cursor: Option<String>,
) -> Result<impl Future<Output = Result<SearchPage, ErrorDto>> + Send + 'static, String> {
    let worker = repo_worker(state, repo_path)?;
    let generation = worker.signal().claim_search_generation();
    Ok(worker.call(move |w| w.search_commits_claimed(generation, query, selection, cursor)))
}

/// Stops the repo's in-flight search, if any. Never queued behind it — see
/// `WorkerHandle::cancel_search`.
#[tauri::command]
pub async fn cancel_search(repo_path: String, state: State<'_, AppState>) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?.signal().cancel_search();
    Ok(())
}
