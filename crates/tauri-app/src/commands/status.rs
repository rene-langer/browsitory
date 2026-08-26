use super::{worker_handle, AppState, BlameLineDto, DiffHunkDto, GraphCommitDto, StatusEntryDto};
use std::path::Path;
use tauri::State;

#[tauri::command]
pub async fn get_status(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<StatusEntryDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_status()?
        .into_iter()
        .map(|entry| StatusEntryDto {
            path: entry.path,
            staged: entry.staged,
            kind: format!("{:?}", entry.kind),
        })
        .collect())
}

#[tauri::command]
pub async fn get_commit_graph(
    repo_path: String,
    limit: usize,
    selected_branches: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<Vec<GraphCommitDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_commit_graph(limit, selected_branches)?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect())
}

#[tauri::command]
pub fn get_graph_branch_selection(repo_path: String) -> Result<Option<Vec<String>>, String> {
    config::get_graph_branch_selection(Path::new(&repo_path)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_graph_branch_selection(
    repo_path: String,
    selected_branches: Vec<String>,
) -> Result<(), String> {
    config::set_graph_branch_selection(Path::new(&repo_path), &selected_branches)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_working_diff(
    repo_path: String,
    path: String,
    staged: bool,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_working_diff(path, staged)?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_commit_diff(
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_commit_diff(commit_id, path)?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_commit_files(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    worker_handle(&state, &repo_path)?.get_commit_files(commit_id)
}

#[tauri::command]
pub async fn get_blame(
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BlameLineDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_blame(commit_id, path)?
        .into_iter()
        .map(BlameLineDto::from)
        .collect())
}

#[tauri::command]
pub async fn stage_file(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.stage_file(path)
}

#[tauri::command]
pub async fn unstage_file(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.unstage_file(path)
}

#[tauri::command]
pub async fn stage_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.stage_hunk(path, old_start, new_start)
}

#[tauri::command]
pub async fn unstage_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.unstage_hunk(path, old_start, new_start)
}

#[tauri::command]
pub async fn discard_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.discard_hunk(path, old_start, new_start)
}

#[tauri::command]
pub async fn commit(
    repo_path: String,
    message: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    worker_handle(&state, &repo_path)?.commit(message)
}
