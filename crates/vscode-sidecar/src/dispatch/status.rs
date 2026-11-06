use repo_service::wire::{
    to_trailers, BlameLineDto, CommitOutcomeDto, CommitSignatureDto, DiffHunkDto, DiffOptionsDto,
    FileHistoryEntryDto, GraphCommitDto, StatusEntryDto,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};

use super::{
    spawn_hook_relay, worker_handle, CommitMessageParams, GetCommitFilesParams, RepoFilePathParams,
    RepoPathParams, Repos,
};

pub(super) fn get_status(params: RepoPathParams, repos: &Repos) -> Result<Value, String> {
    let entries: Vec<StatusEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_status()?
        .into_iter()
        .map(StatusEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCommitGraphParams {
    repo_path: String,
    limit: usize,
    selected_branches: Option<Vec<String>>,
}

pub(super) fn get_commit_graph(
    params: GetCommitGraphParams,
    repos: &Repos,
) -> Result<Value, String> {
    let commits: Vec<GraphCommitDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_graph(params.limit, params.selected_branches)?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetWorkingDiffParams {
    repo_path: String,
    path: String,
    staged: bool,
    opts: DiffOptionsDto,
    /// A staged rename's old path (optional, default `null`).
    #[serde(default)]
    old_path: Option<String>,
}

pub(super) fn get_working_diff(
    params: GetWorkingDiffParams,
    repos: &Repos,
) -> Result<Value, String> {
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_working_diff(
            params.path,
            params.staged,
            params.opts.into(),
            params.old_path,
        )?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCommitDiffParams {
    repo_path: String,
    commit_id: String,
    path: String,
    opts: DiffOptionsDto,
}

pub(super) fn get_commit_diff(params: GetCommitDiffParams, repos: &Repos) -> Result<Value, String> {
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_diff(params.commit_id, params.path, params.opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetFileHistoryParams {
    repo_path: String,
    path: String,
    limit: usize,
}

pub(super) fn get_file_history(
    params: GetFileHistoryParams,
    repos: &Repos,
) -> Result<Value, String> {
    let entries: Vec<FileHistoryEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_file_history(params.path, params.limit)?
        .into_iter()
        .map(FileHistoryEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCommitRenameDiffParams {
    repo_path: String,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: DiffOptionsDto,
}

pub(super) fn get_commit_rename_diff(
    params: GetCommitRenameDiffParams,
    repos: &Repos,
) -> Result<Value, String> {
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_rename_diff(
            params.commit_id,
            params.old_path,
            params.new_path,
            params.opts.into(),
        )?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

pub(super) fn get_graph_branch_selection(params: RepoPathParams) -> Result<Value, String> {
    let selection = config::get_graph_branch_selection(Path::new(&params.repo_path))
        .map_err(|error| error.to_string())?;
    serde_json::to_value(selection).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetGraphBranchSelectionParams {
    repo_path: String,
    selected_branches: Vec<String>,
}

pub(super) fn set_graph_branch_selection(
    params: SetGraphBranchSelectionParams,
) -> Result<Value, String> {
    config::set_graph_branch_selection(Path::new(&params.repo_path), &params.selected_branches)
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

pub(super) fn get_graph_remote_branch_selection(params: RepoPathParams) -> Result<Value, String> {
    let selection = config::get_graph_remote_branch_selection(Path::new(&params.repo_path))
        .map_err(|error| error.to_string())?;
    serde_json::to_value(selection).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetGraphRemoteBranchSelectionParams {
    repo_path: String,
    selected_branches: Vec<String>,
}

pub(super) fn set_graph_remote_branch_selection(
    params: SetGraphRemoteBranchSelectionParams,
) -> Result<Value, String> {
    config::set_graph_remote_branch_selection(
        Path::new(&params.repo_path),
        &params.selected_branches,
    )
    .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

pub(super) fn get_commit_files(
    params: GetCommitFilesParams,
    repos: &Repos,
) -> Result<Value, String> {
    let files = worker_handle(repos, &params.repo_path)?.get_commit_files(params.commit_id)?;
    serde_json::to_value(files).map_err(|error| error.to_string())
}

pub(super) fn get_commit_message(
    params: GetCommitFilesParams,
    repos: &Repos,
) -> Result<Value, String> {
    let message = worker_handle(repos, &params.repo_path)?.get_commit_message(params.commit_id)?;
    serde_json::to_value(message).map_err(|error| error.to_string())
}

pub(super) fn get_commit_signature(
    params: GetCommitFilesParams,
    repos: &Repos,
) -> Result<Value, String> {
    let signature =
        worker_handle(repos, &params.repo_path)?.get_commit_signature(params.commit_id)?;
    serde_json::to_value(CommitSignatureDto::from(signature)).map_err(|error| error.to_string())
}

pub(super) fn stage_file(params: RepoFilePathParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.stage_file(params.path)?;
    Ok(Value::Null)
}

pub(super) fn unstage_file(params: RepoFilePathParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.unstage_file(params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HunkParams {
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
}

pub(super) fn stage_hunk(params: HunkParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.stage_hunk(
        params.path,
        params.old_start,
        params.new_start,
        params.context_lines,
    )?;
    Ok(Value::Null)
}

pub(super) fn unstage_hunk(params: HunkParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.unstage_hunk(
        params.path,
        params.old_start,
        params.new_start,
        params.context_lines,
    )?;
    Ok(Value::Null)
}

pub(super) fn discard_hunk(params: HunkParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.discard_hunk(
        params.path,
        params.old_start,
        params.new_start,
        params.context_lines,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LinesParams {
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    /// Every line's `content` of the hunk as the webview displayed it (stale-view guard).
    expected_lines: Vec<String>,
}

pub(super) fn stage_lines(params: LinesParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.stage_lines(
        params.path,
        params.old_start,
        params.new_start,
        params.context_lines,
        params.line_indices,
        params.expected_lines,
    )?;
    Ok(Value::Null)
}

pub(super) fn unstage_lines(params: LinesParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.unstage_lines(
        params.path,
        params.old_start,
        params.new_start,
        params.context_lines,
        params.line_indices,
        params.expected_lines,
    )?;
    Ok(Value::Null)
}

pub(super) fn discard_lines(params: LinesParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.discard_lines(
        params.path,
        params.old_start,
        params.new_start,
        params.context_lines,
        params.line_indices,
        params.expected_lines,
    )?;
    Ok(Value::Null)
}

pub(super) fn discard_file(params: RepoFilePathParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.discard_file(params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DiscardAllParams {
    repo_path: String,
    /// Exactly the paths the confirm dialog listed.
    paths: Vec<String>,
}

pub(super) fn discard_all_unstaged(
    params: DiscardAllParams,
    repos: &Repos,
) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.discard_all_unstaged(params.paths)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IgnorePathParams {
    repo_path: String,
    pattern: String,
}

pub(super) fn ignore_path(params: IgnorePathParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.ignore_path(params.pattern)?;
    Ok(Value::Null)
}

pub(super) fn commit(
    params: CommitMessageParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(params.repo_path.clone(), event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.commit(
        params.message,
        params.no_verify,
        to_trailers(params.trailers),
        event_tx,
    )?;
    serde_json::to_value(CommitOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetBlameParams {
    repo_path: String,
    commit_id: String,
    path: String,
}

pub(super) fn get_blame(params: GetBlameParams, repos: &Repos) -> Result<Value, String> {
    let lines: Vec<BlameLineDto> = worker_handle(repos, &params.repo_path)?
        .get_blame(params.commit_id, params.path)?
        .into_iter()
        .map(BlameLineDto::from)
        .collect();
    serde_json::to_value(lines).map_err(|error| error.to_string())
}
