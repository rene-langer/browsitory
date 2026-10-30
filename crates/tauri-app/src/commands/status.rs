use super::{
    emit_hook_events, repo_worker, AppState, BlameLineDto, CommitOutcomeDto, CommitSignatureDto,
    DiffHunkDto, DiffOptionsDto, FileHistoryEntryDto, GraphCommitDto, StatusEntryDto,
};
use std::path::Path;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_status(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<StatusEntryDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_status())
        .await?
        .into_iter()
        .map(StatusEntryDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_commit_graph(
    repo_path: String,
    limit: usize,
    selected_branches: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<Vec<GraphCommitDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_graph(limit, selected_branches))
        .await?
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
    old_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_working_diff(path, staged, opts.into(), old_path))
        .await?
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
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_diff(commit_id, path, opts.into()))
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_files(commit_id))
        .await
}

#[tauri::command]
pub async fn get_commit_message(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_message(commit_id))
        .await
}

#[tauri::command]
pub async fn get_commit_signature(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<CommitSignatureDto, String> {
    Ok(CommitSignatureDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.get_commit_signature(commit_id))
            .await?,
    ))
}

#[tauri::command]
pub async fn get_blame(
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BlameLineDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_blame(commit_id, path))
        .await?
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
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_file_history(path, limit))
        .await?
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
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_rename_diff(commit_id, old_path, new_path, opts.into()))
        .await?
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.stage_file(path))
        .await
}

#[tauri::command]
pub async fn unstage_file(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.unstage_file(path))
        .await
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.stage_hunk(path, old_start, new_start, context_lines))
        .await
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.unstage_hunk(path, old_start, new_start, context_lines))
        .await
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
    repo_worker(&state, &repo_path)?
        .call(move |w| w.discard_hunk(path, old_start, new_start, context_lines))
        .await
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
    repo_worker(&state, &repo_path)?
        .call(move |w| {
            w.stage_lines(
                path,
                old_start,
                new_start,
                context_lines,
                line_indices,
                expected_lines,
            )
        })
        .await
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
    repo_worker(&state, &repo_path)?
        .call(move |w| {
            w.unstage_lines(
                path,
                old_start,
                new_start,
                context_lines,
                line_indices,
                expected_lines,
            )
        })
        .await
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
    repo_worker(&state, &repo_path)?
        .call(move |w| {
            w.discard_lines(
                path,
                old_start,
                new_start,
                context_lines,
                line_indices,
                expected_lines,
            )
        })
        .await
}

#[tauri::command]
pub async fn discard_file(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.discard_file(path))
        .await
}

#[tauri::command]
pub async fn discard_all_unstaged(
    repo_path: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.discard_all_unstaged(paths))
        .await
}

#[tauri::command]
pub async fn ignore_path(
    repo_path: String,
    pattern: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.ignore_path(pattern))
        .await
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
        repo_worker(&state, &repo_path)?
            .call(move |w| w.commit(message, no_verify, super::trailers_from(trailers), event_tx))
            .await?,
    ))
}
