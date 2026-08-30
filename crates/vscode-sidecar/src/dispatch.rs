use std::collections::HashMap;
use std::path::{Path, PathBuf};

use git_core::diff::DiffHunk;
use git_core::graph::GraphCommit;
use git_core::status::StatusEntry;
use repo_service::worker::Worker;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn dispatch(
    method: &str,
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    match method {
        "open_repo" => open_repo(params, repos),
        "close_repo" => close_repo(params, repos),
        "get_status" => get_status(params, repos),
        "get_commit_graph" => get_commit_graph(params, repos),
        "get_working_diff" => get_working_diff(params, repos),
        "get_commit_diff" => get_commit_diff(params, repos),
        "list_recent_repos" => list_recent_repos(),
        "list_open_repos" => list_open_repos_handler(),
        "persist_open_repos" => persist_open_repos(params),
        "scan_repos_in_root" => scan_repos_in_root(params),
        "list_workspaces" => list_workspaces_handler(),
        "save_workspace" => save_workspace(params),
        "update_workspace" => update_workspace(params),
        "delete_workspace" => delete_workspace(params),
        "get_graph_branch_selection" => get_graph_branch_selection(params),
        "set_graph_branch_selection" => set_graph_branch_selection(params),
        "get_commit_files" => get_commit_files(params, repos),
        "stage_file" => stage_file(params, repos),
        "unstage_file" => unstage_file(params, repos),
        "stage_hunk" => stage_hunk(params, repos),
        "unstage_hunk" => unstage_hunk(params, repos),
        "discard_hunk" => discard_hunk(params, repos),
        "commit" => commit(params, repos),
        other => Err(format!("unknown method: {other}")),
    }
}

fn worker_handle(
    repos: &HashMap<String, Worker>,
    repo_path: &str,
) -> Result<repo_service::worker::WorkerHandle, String> {
    repos
        .get(repo_path)
        .map(Worker::handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))
}

#[derive(Deserialize)]
struct OpenRepoParams {
    path: String,
}

fn open_repo(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: OpenRepoParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    if let std::collections::hash_map::Entry::Vacant(entry) = repos.entry(params.path.clone()) {
        let worker = Worker::spawn(params.path.clone().into())?;
        entry.insert(worker);
    }
    let _ = config::add_recent_repo(Path::new(&params.path));
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoPathParams {
    repo_path: String,
}

fn close_repo(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    repos.remove(&params.repo_path);
    Ok(Value::Null)
}

#[derive(Serialize)]
struct StatusEntryDto {
    path: String,
    staged: bool,
    kind: String,
}

impl From<StatusEntry> for StatusEntryDto {
    fn from(entry: StatusEntry) -> Self {
        Self {
            path: entry.path,
            staged: entry.staged,
            kind: format!("{:?}", entry.kind),
        }
    }
}

fn get_status(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<StatusEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_status()?
        .into_iter()
        .map(StatusEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitGraphParams {
    repo_path: String,
    limit: usize,
    selected_branches: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphCommitDto {
    id: String,
    short_id: String,
    summary: String,
    author_name: String,
    author_email: String,
    timestamp: i64,
    parent_ids: Vec<String>,
    branch_refs: Vec<String>,
}

impl From<GraphCommit> for GraphCommitDto {
    fn from(c: GraphCommit) -> Self {
        Self {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            author_email: c.author_email,
            timestamp: c.timestamp,
            parent_ids: c.parent_ids,
            branch_refs: c.branch_refs,
        }
    }
}

fn get_commit_graph(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetCommitGraphParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let commits: Vec<GraphCommitDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_graph(params.limit, params.selected_branches)?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(|error| error.to_string())
}

#[derive(Serialize)]
struct DiffLineDto {
    origin: String,
    content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiffHunkDto {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<DiffLineDto>,
}

impl From<DiffHunk> for DiffHunkDto {
    fn from(hunk: DiffHunk) -> Self {
        Self {
            old_start: hunk.old_start,
            old_lines: hunk.old_lines,
            new_start: hunk.new_start,
            new_lines: hunk.new_lines,
            lines: hunk
                .lines
                .into_iter()
                .map(|line| DiffLineDto {
                    origin: format!("{:?}", line.origin),
                    content: line.content,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetWorkingDiffParams {
    repo_path: String,
    path: String,
    staged: bool,
}

fn get_working_diff(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetWorkingDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_working_diff(params.path, params.staged)?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitDiffParams {
    repo_path: String,
    commit_id: String,
    path: String,
}

fn get_commit_diff(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetCommitDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_diff(params.commit_id, params.path)?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

fn list_recent_repos() -> Result<Value, String> {
    let paths = config::list_recent_repos().map_err(|error| error.to_string())?;
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    serde_json::to_value(paths).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenRepoEntryDto {
    path: String,
    workspace_id: Option<String>,
}

impl From<config::OpenRepoEntry> for OpenRepoEntryDto {
    fn from(entry: config::OpenRepoEntry) -> Self {
        Self {
            path: entry.path.to_string_lossy().into_owned(),
            workspace_id: entry.workspace_id,
        }
    }
}

fn list_open_repos_handler() -> Result<Value, String> {
    let (entries, active) = config::list_open_repos().map_err(|error| error.to_string())?;
    let entries: Vec<OpenRepoEntryDto> = entries.into_iter().map(OpenRepoEntryDto::from).collect();
    let active = active.map(|p| p.to_string_lossy().into_owned());
    serde_json::to_value(serde_json::json!({ "entries": entries, "activePath": active }))
        .map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenRepoEntryInput {
    path: String,
    workspace_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistOpenReposParams {
    entries: Vec<OpenRepoEntryInput>,
    active_path: Option<String>,
}

fn persist_open_repos(params: Value) -> Result<Value, String> {
    let params: PersistOpenReposParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<config::OpenRepoEntry> = params
        .entries
        .into_iter()
        .map(|entry| config::OpenRepoEntry {
            path: PathBuf::from(entry.path),
            workspace_id: entry.workspace_id,
        })
        .collect();
    config::set_open_repos(&entries, params.active_path.as_deref().map(Path::new))
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct ScanReposInRootParams {
    root: String,
}

fn scan_repos_in_root(params: Value) -> Result<Value, String> {
    let params: ScanReposInRootParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let paths =
        config::scan_repos_in_root(Path::new(&params.root)).map_err(|error| error.to_string())?;
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    serde_json::to_value(paths).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceDto {
    id: String,
    name: String,
    root_path: String,
    member_paths: Vec<String>,
}

impl From<config::Workspace> for WorkspaceDto {
    fn from(workspace: config::Workspace) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name,
            root_path: workspace.root_path.to_string_lossy().into_owned(),
            member_paths: workspace
                .member_paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        }
    }
}

fn list_workspaces_handler() -> Result<Value, String> {
    let workspaces = config::list_workspaces().map_err(|error| error.to_string())?;
    let workspaces: Vec<WorkspaceDto> = workspaces.into_iter().map(WorkspaceDto::from).collect();
    serde_json::to_value(workspaces).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
struct SaveWorkspaceParams {
    name: String,
    root: String,
    members: Vec<String>,
}

fn save_workspace(params: Value) -> Result<Value, String> {
    let params: SaveWorkspaceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let members: Vec<PathBuf> = params.members.into_iter().map(PathBuf::from).collect();
    let id = config::save_workspace(&params.name, Path::new(&params.root), &members)
        .map_err(|error| error.to_string())?;
    Ok(Value::String(id))
}

#[derive(Deserialize)]
struct UpdateWorkspaceParams {
    id: String,
    name: String,
    members: Vec<String>,
}

fn update_workspace(params: Value) -> Result<Value, String> {
    let params: UpdateWorkspaceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let members: Vec<PathBuf> = params.members.into_iter().map(PathBuf::from).collect();
    config::update_workspace(&params.id, &params.name, &members)
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct DeleteWorkspaceParams {
    id: String,
}

fn delete_workspace(params: Value) -> Result<Value, String> {
    let params: DeleteWorkspaceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::delete_workspace(&params.id).map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

fn get_graph_branch_selection(params: Value) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let selection = config::get_graph_branch_selection(Path::new(&params.repo_path))
        .map_err(|error| error.to_string())?;
    serde_json::to_value(selection).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetGraphBranchSelectionParams {
    repo_path: String,
    selected_branches: Vec<String>,
}

fn set_graph_branch_selection(params: Value) -> Result<Value, String> {
    let params: SetGraphBranchSelectionParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::set_graph_branch_selection(Path::new(&params.repo_path), &params.selected_branches)
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitFilesParams {
    repo_path: String,
    commit_id: String,
}

fn get_commit_files(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetCommitFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files = worker_handle(repos, &params.repo_path)?.get_commit_files(params.commit_id)?;
    serde_json::to_value(files).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoFilePathParams {
    repo_path: String,
    path: String,
}

fn stage_file(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.stage_file(params.path)?;
    Ok(Value::Null)
}

fn unstage_file(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.unstage_file(params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HunkParams {
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
}

fn stage_hunk(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.stage_hunk(
        params.path,
        params.old_start,
        params.new_start,
    )?;
    Ok(Value::Null)
}

fn unstage_hunk(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.unstage_hunk(
        params.path,
        params.old_start,
        params.new_start,
    )?;
    Ok(Value::Null)
}

fn discard_hunk(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.discard_hunk(
        params.path,
        params.old_start,
        params.new_start,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitParams {
    repo_path: String,
    message: String,
}

fn commit(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CommitParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let commit_id = worker_handle(repos, &params.repo_path)?.commit(params.message)?;
    Ok(Value::String(commit_id))
}
