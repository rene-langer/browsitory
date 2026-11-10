//! "Collapse merges" (#617): the per-repository setting and the commits of an expanded merge.

use super::{repo_worker, AppState};
use repo_service::wire::{ErrorDto, GraphCommitDto};
use std::path::Path;
use tauri::State;

/// The commits merge `merge_id` brought in, at most `limit`: a collapsed merge expanded.
#[tauri::command]
pub async fn get_merged_commits(
    repo_path: String,
    merge_id: String,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<GraphCommitDto>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_merged_commits(merge_id, limit))
        .await?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect())
}

/// Whether the graph of `repo_path` collapses merges.
#[tauri::command]
pub fn get_graph_collapse_merges(repo_path: String) -> Result<bool, ErrorDto> {
    config::get_graph_collapse_merges(Path::new(&repo_path)).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn set_graph_collapse_merges(repo_path: String, collapse: bool) -> Result<(), ErrorDto> {
    let path = Path::new(&repo_path);
    config::set_graph_collapse_merges(path, collapse).map_err(ErrorDto::from_error)
}
