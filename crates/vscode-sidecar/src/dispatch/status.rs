use repo_service::wire::CommitDetailsDto;
use repo_service::wire::CommitStatsEntryDto;
use repo_service::wire::ErrorDto;
use repo_service::wire::{
    to_trailers, BlameLineDto, CommitLocationDto, CommitOutcomeDto, CommitSignatureDto,
    DiffHunkDto, DiffOptionsDto, FileHistoryEntryDto, GraphCommitDto, GraphSelectionDto,
    LineHistoryDto, StatusEntryDto,
};
use serde::Deserialize;
use serde_json::Value;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};

use super::{
    spawn_hook_relay, worker_handle, CommitMessageParams, GetCommitFilesParams, RepoFilePathParams,
    RepoPathParams, Repos,
};

/// The repository's change stamp (#650), a string.
pub(super) fn get_change_stamp(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let stamp = worker_handle(repos, &params.repo_path)?.get_change_stamp()?;
    Ok(Value::String(stamp))
}

pub(super) fn get_status(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let entries: Vec<StatusEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_status()?
        .into_iter()
        .map(StatusEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCommitGraphParams {
    repo_path: String,
    limit: usize,
    /// `selection.remote` is required, so a caller that doesn't know about remote branches
    /// fails loudly instead of silently dropping them from the graph (#385).
    selection: GraphSelectionDto,
}

pub(super) fn get_commit_graph(
    params: GetCommitGraphParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let commits: Vec<GraphCommitDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_graph(params.limit, params.selection.into())?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LocateCommitParams {
    repo_path: String,
    revision: String,
    selection: GraphSelectionDto,
}

/// Where `revision`'s commit sits in the graph of the given selection (#441).
pub(super) fn locate_commit(params: LocateCommitParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let location = worker_handle(repos, &params.repo_path)?
        .locate_commit(params.revision, params.selection.into())?
        .map(CommitLocationDto::from);
    serde_json::to_value(location).map_err(ErrorDto::from_error)
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
) -> Result<Value, ErrorDto> {
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
    serde_json::to_value(hunks).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCommitDiffParams {
    repo_path: String,
    commit_id: String,
    path: String,
    opts: DiffOptionsDto,
}

pub(super) fn get_commit_diff(
    params: GetCommitDiffParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_diff(params.commit_id, params.path, params.opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(ErrorDto::from_error)
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
) -> Result<Value, ErrorDto> {
    let entries: Vec<FileHistoryEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_file_history(params.path, params.limit)?
        .into_iter()
        .map(FileHistoryEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetLineHistoryParams {
    repo_path: String,
    revision: String,
    path: String,
    start_line: usize,
    end_line: usize,
    limit: usize,
}

pub(super) fn get_line_history(
    params: GetLineHistoryParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let history: LineHistoryDto = worker_handle(repos, &params.repo_path)?
        .get_line_history(
            params.revision,
            params.path,
            params.start_line,
            params.end_line,
            params.limit,
        )?
        .into();
    serde_json::to_value(history).map_err(ErrorDto::from_error)
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
) -> Result<Value, ErrorDto> {
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
    serde_json::to_value(hunks).map_err(ErrorDto::from_error)
}

pub(super) fn get_graph_branch_selection(params: RepoPathParams) -> Result<Value, ErrorDto> {
    let selection = config::get_graph_branch_selection(Path::new(&params.repo_path))
        .map_err(ErrorDto::from_error)?;
    serde_json::to_value(selection).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetGraphBranchSelectionParams {
    repo_path: String,
    local: Vec<String>,
}

pub(super) fn set_graph_branch_selection(
    params: SetGraphBranchSelectionParams,
) -> Result<Value, ErrorDto> {
    config::set_graph_branch_selection(Path::new(&params.repo_path), &params.local)
        .map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

/// Forgets the saved selection, back to every local branch (#402).
pub(super) fn clear_graph_branch_selection(params: RepoPathParams) -> Result<Value, ErrorDto> {
    config::clear_graph_branch_selection(Path::new(&params.repo_path))
        .map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

pub(super) fn get_graph_remote_branch_selection(params: RepoPathParams) -> Result<Value, ErrorDto> {
    let selection = config::get_graph_remote_branch_selection(Path::new(&params.repo_path))
        .map_err(ErrorDto::from_error)?;
    serde_json::to_value(selection).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetGraphRemoteBranchSelectionParams {
    repo_path: String,
    remote: Vec<String>,
}

pub(super) fn set_graph_remote_branch_selection(
    params: SetGraphRemoteBranchSelectionParams,
) -> Result<Value, ErrorDto> {
    config::set_graph_remote_branch_selection(Path::new(&params.repo_path), &params.remote)
        .map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

/// Forgets the saved remote selection, back to no remote commits and every remote badge (#402).
pub(super) fn clear_graph_remote_branch_selection(
    params: RepoPathParams,
) -> Result<Value, ErrorDto> {
    config::clear_graph_remote_branch_selection(Path::new(&params.repo_path))
        .map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

pub(super) fn get_commit_files(
    params: GetCommitFilesParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let files = worker_handle(repos, &params.repo_path)?.get_commit_files(params.commit_id)?;
    serde_json::to_value(files).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCommitStatsParams {
    repo_path: String,
    commit_ids: Vec<String>,
    with_files: bool,
}

/// Line counts of `commit_ids` (#620). The commits the worker's time budget didn't reach are
/// left out; the caller asks for them again.
pub(super) fn get_commit_stats(
    params: GetCommitStatsParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let entries: Vec<CommitStatsEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_stats(params.commit_ids, params.with_files)?
        .into_iter()
        .map(CommitStatsEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(ErrorDto::from_error)
}

pub(super) fn get_commit_message(
    params: GetCommitFilesParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let message = worker_handle(repos, &params.repo_path)?.get_commit_message(params.commit_id)?;
    serde_json::to_value(message).map_err(ErrorDto::from_error)
}

pub(super) fn get_commit_signature(
    params: GetCommitFilesParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let signature =
        worker_handle(repos, &params.repo_path)?.get_commit_signature(params.commit_id)?;
    serde_json::to_value(CommitSignatureDto::from(signature)).map_err(ErrorDto::from_error)
}

/// Author, committer and the branches and tags that contain `commit_id` (#641).
pub(super) fn get_commit_details(
    params: GetCommitFilesParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let details = worker_handle(repos, &params.repo_path)?.get_commit_details(params.commit_id)?;
    serde_json::to_value(CommitDetailsDto::from(details)).map_err(ErrorDto::from_error)
}

pub(super) fn stage_file(params: RepoFilePathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.stage_file(params.path)?;
    Ok(Value::Null)
}

pub(super) fn unstage_file(params: RepoFilePathParams, repos: &Repos) -> Result<Value, ErrorDto> {
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
    algorithm: repo_service::wire::DiffAlgorithmDto,
}

pub(super) fn stage_hunk(params: HunkParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.stage_hunk(
        params.path,
        params.old_start,
        params.new_start,
        repo_service::wire::hunk_options(params.context_lines, params.algorithm),
    )?;
    Ok(Value::Null)
}

pub(super) fn unstage_hunk(params: HunkParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.unstage_hunk(
        params.path,
        params.old_start,
        params.new_start,
        repo_service::wire::hunk_options(params.context_lines, params.algorithm),
    )?;
    Ok(Value::Null)
}

pub(super) fn discard_hunk(params: HunkParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.discard_hunk(
        params.path,
        params.old_start,
        params.new_start,
        repo_service::wire::hunk_options(params.context_lines, params.algorithm),
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
    algorithm: repo_service::wire::DiffAlgorithmDto,
    line_indices: Vec<u32>,
    /// Every line's `content` of the hunk as the webview displayed it (stale-view guard).
    expected_lines: Vec<String>,
}

pub(super) fn stage_lines(params: LinesParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.stage_lines(
        params.path,
        params.old_start,
        params.new_start,
        repo_service::wire::hunk_options(params.context_lines, params.algorithm),
        params.line_indices,
        params.expected_lines,
    )?;
    Ok(Value::Null)
}

pub(super) fn unstage_lines(params: LinesParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.unstage_lines(
        params.path,
        params.old_start,
        params.new_start,
        repo_service::wire::hunk_options(params.context_lines, params.algorithm),
        params.line_indices,
        params.expected_lines,
    )?;
    Ok(Value::Null)
}

pub(super) fn discard_lines(params: LinesParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.discard_lines(
        params.path,
        params.old_start,
        params.new_start,
        repo_service::wire::hunk_options(params.context_lines, params.algorithm),
        params.line_indices,
        params.expected_lines,
    )?;
    Ok(Value::Null)
}

pub(super) fn discard_file(params: RepoFilePathParams, repos: &Repos) -> Result<Value, ErrorDto> {
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
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.discard_all_unstaged(params.paths)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct IgnorePathParams {
    repo_path: String,
    pattern: String,
}

pub(super) fn ignore_path(params: IgnorePathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.ignore_path(params.pattern)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetIdentityParams {
    repo_path: String,
    name: String,
    email: String,
    global: bool,
}

/// Sets the git identity commits are made with (#689).
pub(super) fn set_identity(params: SetIdentityParams, repos: &Repos) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.set_identity(
        params.name,
        params.email,
        params.global,
    )?;
    Ok(Value::Null)
}

pub(super) fn commit(
    params: CommitMessageParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(params.repo_path.clone(), event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.commit(
        params.message,
        params.no_verify,
        to_trailers(params.trailers),
        event_tx,
    )?;
    serde_json::to_value(CommitOutcomeDto::from(outcome)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetBlameParams {
    repo_path: String,
    commit_id: String,
    path: String,
}

pub(super) fn get_blame(params: GetBlameParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let lines: Vec<BlameLineDto> = worker_handle(repos, &params.repo_path)?
        .get_blame(params.commit_id, params.path)?
        .into_iter()
        .map(BlameLineDto::from)
        .collect();
    serde_json::to_value(lines).map_err(ErrorDto::from_error)
}
