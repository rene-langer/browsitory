use super::{
    emit_hook_events, worker_handle, AppState, BlameLineDto, CommitOutcomeDto, CommitSignatureDto,
    DiffHunkDto, DiffOptionsDto, FileHistoryEntryDto, GraphCommitDto, StatusEntryDto,
};
use std::path::Path;
use tauri::{AppHandle, State};

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
pub fn get_graph_remote_branch_selection(repo_path: String) -> Result<Option<Vec<String>>, String> {
    config::get_graph_remote_branch_selection(Path::new(&repo_path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_graph_remote_branch_selection(
    repo_path: String,
    selected_branches: Vec<String>,
) -> Result<(), String> {
    config::set_graph_remote_branch_selection(Path::new(&repo_path), &selected_branches)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_working_diff(
    repo_path: String,
    path: String,
    staged: bool,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_working_diff(path, staged, opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_commit_diff(
    repo_path: String,
    commit_id: String,
    path: String,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_commit_diff(commit_id, path, opts.into())?
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
pub async fn get_commit_message(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    worker_handle(&state, &repo_path)?.get_commit_message(commit_id)
}

#[tauri::command]
pub async fn get_commit_signature(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<CommitSignatureDto, String> {
    Ok(CommitSignatureDto::from(
        worker_handle(&state, &repo_path)?.get_commit_signature(commit_id)?,
    ))
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
pub async fn get_file_history(
    repo_path: String,
    path: String,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<FileHistoryEntryDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_file_history(path, limit)?
        .into_iter()
        .map(FileHistoryEntryDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_commit_rename_diff(
    repo_path: String,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_commit_rename_diff(commit_id, old_path, new_path, opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
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
    context_lines: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.stage_hunk(path, old_start, new_start, context_lines)
}

#[tauri::command]
pub async fn unstage_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.unstage_hunk(path, old_start, new_start, context_lines)
}

#[tauri::command]
pub async fn discard_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.discard_hunk(path, old_start, new_start, context_lines)
}

// Each argument is a top-level IPC field, so they can't be grouped without changing the wire.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn stage_lines(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.stage_lines(
        path,
        old_start,
        new_start,
        context_lines,
        line_indices,
        expected_lines,
    )
}

// Each argument is a top-level IPC field, so they can't be grouped without changing the wire.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn unstage_lines(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.unstage_lines(
        path,
        old_start,
        new_start,
        context_lines,
        line_indices,
        expected_lines,
    )
}

// Each argument is a top-level IPC field, so they can't be grouped without changing the wire.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn discard_lines(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.discard_lines(
        path,
        old_start,
        new_start,
        context_lines,
        line_indices,
        expected_lines,
    )
}

#[tauri::command]
pub async fn discard_file(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.discard_file(path)
}

#[tauri::command]
pub async fn discard_all_unstaged(
    repo_path: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.discard_all_unstaged(paths)
}

#[tauri::command]
pub async fn ignore_path(
    repo_path: String,
    pattern: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.ignore_path(pattern)
}

/// `trailers` (absent: none) are appended before the hooks run.
#[tauri::command]
pub async fn commit(
    repo_path: String,
    message: String,
    no_verify: bool,
    trailers: Option<Vec<super::TrailerDto>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CommitOutcomeDto, String> {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    emit_hook_events(app, repo_path.clone(), event_rx);
    Ok(CommitOutcomeDto::from(
        worker_handle(&state, &repo_path)?.commit(
            message,
            no_verify,
            super::trailers_from(trailers),
            event_tx,
        )?,
    ))
}
