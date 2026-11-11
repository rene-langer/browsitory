use super::{emit_hook_events, repo_worker, AppState};
use repo_service::wire::CommitDetailsDto;
use repo_service::wire::CommitStatsEntryDto;
use repo_service::wire::ErrorDto;
use repo_service::wire::{
    to_trailers, BlameLineDto, CommitLocationDto, CommitOutcomeDto, CommitSignatureDto,
    DiffHunkDto, DiffOptionsDto, FileHistoryEntryDto, GraphCommitDto, GraphSelectionDto,
    LineHistoryDto, StatusEntryDto, TrailerDto,
};
use std::path::Path;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_status(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<StatusEntryDto>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_status())
        .await?
        .into_iter()
        .map(StatusEntryDto::from)
        .collect())
}

/// The repository's change stamp (#650): the frontend reloads a tab when it differs from the
/// one its last refresh read.
#[tauri::command]
pub async fn get_change_stamp(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<String, ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.get_change_stamp())
        .await
}

#[tauri::command]
pub async fn get_commit_graph(
    repo_path: String,
    limit: usize,
    selection: GraphSelectionDto,
    state: State<'_, AppState>,
) -> Result<Vec<GraphCommitDto>, ErrorDto> {
    let selection = git_core::graph::GraphSelection::from(selection);
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_graph(limit, selection))
        .await?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect())
}

/// Where `revision`'s commit sits in the graph of the given selection (#441).
#[tauri::command]
pub async fn locate_commit(
    repo_path: String,
    revision: String,
    selection: GraphSelectionDto,
    state: State<'_, AppState>,
) -> Result<Option<CommitLocationDto>, ErrorDto> {
    let selection = git_core::graph::GraphSelection::from(selection);
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.locate_commit(revision, selection))
        .await?
        .map(CommitLocationDto::from))
}

#[tauri::command]
pub fn get_graph_branch_selection(repo_path: String) -> Result<Option<Vec<String>>, ErrorDto> {
    config::get_graph_branch_selection(Path::new(&repo_path)).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn set_graph_branch_selection(repo_path: String, local: Vec<String>) -> Result<(), ErrorDto> {
    config::set_graph_branch_selection(Path::new(&repo_path), &local).map_err(ErrorDto::from_error)
}

/// Forgets the saved selection, back to every local branch (#402).
#[tauri::command]
pub fn clear_graph_branch_selection(repo_path: String) -> Result<(), ErrorDto> {
    config::clear_graph_branch_selection(Path::new(&repo_path)).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn get_graph_remote_branch_selection(
    repo_path: String,
) -> Result<Option<Vec<String>>, ErrorDto> {
    config::get_graph_remote_branch_selection(Path::new(&repo_path)).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn set_graph_remote_branch_selection(
    repo_path: String,
    remote: Vec<String>,
) -> Result<(), ErrorDto> {
    config::set_graph_remote_branch_selection(Path::new(&repo_path), &remote)
        .map_err(ErrorDto::from_error)
}

/// Forgets the saved remote selection, back to no remote commits and every remote badge (#402).
#[tauri::command]
pub fn clear_graph_remote_branch_selection(repo_path: String) -> Result<(), ErrorDto> {
    config::clear_graph_remote_branch_selection(Path::new(&repo_path)).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub async fn get_working_diff(
    repo_path: String,
    path: String,
    staged: bool,
    opts: DiffOptionsDto,
    old_path: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, ErrorDto> {
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
) -> Result<Vec<DiffHunkDto>, ErrorDto> {
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
) -> Result<Vec<String>, ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_files(commit_id))
        .await
}

/// Line counts of `commit_ids` (#620). The commits the worker's time budget didn't reach are
/// left out; the caller asks for them again.
#[tauri::command]
pub async fn get_commit_stats(
    repo_path: String,
    commit_ids: Vec<String>,
    with_files: bool,
    state: State<'_, AppState>,
) -> Result<Vec<CommitStatsEntryDto>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_stats(commit_ids, with_files))
        .await?
        .into_iter()
        .map(CommitStatsEntryDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_commit_message(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<String, ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.get_commit_message(commit_id))
        .await
}

#[tauri::command]
pub async fn get_commit_signature(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<CommitSignatureDto, ErrorDto> {
    Ok(CommitSignatureDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.get_commit_signature(commit_id))
            .await?,
    ))
}

/// Author, committer and the branches and tags that contain `commit_id` (#641).
#[tauri::command]
pub async fn get_commit_details(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<CommitDetailsDto, ErrorDto> {
    Ok(CommitDetailsDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.get_commit_details(commit_id))
            .await?,
    ))
}

#[tauri::command]
pub async fn get_blame(
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BlameLineDto>, ErrorDto> {
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
) -> Result<Vec<FileHistoryEntryDto>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_file_history(path, limit))
        .await?
        .into_iter()
        .map(FileHistoryEntryDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_line_history(
    repo_path: String,
    revision: String,
    path: String,
    start_line: usize,
    end_line: usize,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<LineHistoryDto, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_line_history(revision, path, start_line, end_line, limit))
        .await?
        .into())
}

#[tauri::command]
pub async fn get_commit_rename_diff(
    repo_path: String,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, ErrorDto> {
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
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.stage_file(path))
        .await
}

#[tauri::command]
pub async fn unstage_file(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
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
    algorithm: repo_service::wire::DiffAlgorithmDto,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let hunk = repo_service::wire::hunk_options(context_lines, algorithm);
    repo_worker(&state, &repo_path)?
        .call(move |w| w.stage_hunk(path, old_start, new_start, hunk))
        .await
}

#[tauri::command]
pub async fn unstage_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    algorithm: repo_service::wire::DiffAlgorithmDto,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let hunk = repo_service::wire::hunk_options(context_lines, algorithm);
    repo_worker(&state, &repo_path)?
        .call(move |w| w.unstage_hunk(path, old_start, new_start, hunk))
        .await
}

#[tauri::command]
pub async fn discard_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    algorithm: repo_service::wire::DiffAlgorithmDto,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let hunk = repo_service::wire::hunk_options(context_lines, algorithm);
    repo_worker(&state, &repo_path)?
        .call(move |w| w.discard_hunk(path, old_start, new_start, hunk))
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
    algorithm: repo_service::wire::DiffAlgorithmDto,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let hunk = repo_service::wire::hunk_options(context_lines, algorithm);
    repo_worker(&state, &repo_path)?
        .call(move |w| {
            w.stage_lines(
                path,
                old_start,
                new_start,
                hunk,
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
    algorithm: repo_service::wire::DiffAlgorithmDto,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let hunk = repo_service::wire::hunk_options(context_lines, algorithm);
    repo_worker(&state, &repo_path)?
        .call(move |w| {
            w.unstage_lines(
                path,
                old_start,
                new_start,
                hunk,
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
    algorithm: repo_service::wire::DiffAlgorithmDto,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let hunk = repo_service::wire::hunk_options(context_lines, algorithm);
    repo_worker(&state, &repo_path)?
        .call(move |w| {
            w.discard_lines(
                path,
                old_start,
                new_start,
                hunk,
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
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.discard_file(path))
        .await
}

#[tauri::command]
pub async fn discard_all_unstaged(
    repo_path: String,
    paths: Vec<String>,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.discard_all_unstaged(paths))
        .await
}

#[tauri::command]
pub async fn ignore_path(
    repo_path: String,
    pattern: String,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.ignore_path(pattern))
        .await
}

/// Sets the git identity commits are made with (#689): in the repository's config, or with
/// `global` in the user's global git config.
#[tauri::command]
pub async fn set_identity(
    repo_path: String,
    name: String,
    email: String,
    global: bool,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.set_identity(name, email, global))
        .await
}

/// `trailers` (absent: none) are appended before the hooks run.
#[tauri::command]
pub async fn commit(
    repo_path: String,
    message: String,
    no_verify: bool,
    trailers: Option<Vec<TrailerDto>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CommitOutcomeDto, ErrorDto> {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    emit_hook_events(app, repo_path.clone(), event_rx);
    Ok(CommitOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| {
                w.commit(
                    message,
                    no_verify,
                    to_trailers(trailers.unwrap_or_default()),
                    event_tx,
                )
            })
            .await?,
    ))
}
