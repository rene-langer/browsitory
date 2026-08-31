use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use git_core::blame::BlameLine;
use git_core::branch::BranchInfo;
use git_core::diff::DiffHunk;
use git_core::graph::GraphCommit;
use git_core::merge::{ConflictSegment, FileConflictChoice, MergeOutcome};
use git_core::reflog::ReflogEntry;
use git_core::remote::TagInfo;
use git_core::stash::StashEntry;
use git_core::status::StatusEntry;
use git_core::submodule::SubmoduleInfo;
use git_core::worktree::WorktreeInfo;
use repo_service::worker::{TransferEvent, Worker};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn dispatch(
    method: &str,
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
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
        "list_branches" => list_branches(params, repos),
        "create_branch" => create_branch(params, repos),
        "switch_branch" => switch_branch(params, repos),
        "delete_branch" => delete_branch(params, repos),
        "rename_branch" => rename_branch(params, repos),
        "list_worktrees" => list_worktrees(params, repos),
        "create_worktree" => create_worktree(params, repos),
        "remove_worktree" => remove_worktree(params, repos),
        "prune_worktrees" => prune_worktrees(params, repos),
        "list_submodules" => list_submodules(params, repos),
        "init_submodule" => init_submodule(params, repos),
        "update_submodule" => update_submodule(params, repos),
        "list_reflog_refs" => list_reflog_refs(params, repos),
        "get_reflog" => get_reflog(params, repos),
        "restore_reflog_entry" => restore_reflog_entry(params, repos),
        "list_remotes" => list_remotes(params, repos),
        "list_remote_branches" => list_remote_branches(params, repos),
        "get_current_upstream" => get_current_upstream(params, repos),
        "get_remote_upstreams" => get_remote_upstreams(params, repos),
        "add_remote" => add_remote(params, repos),
        "rename_remote" => rename_remote(params, repos),
        "update_remote_urls" => update_remote_urls(params, repos),
        "remove_remote" => remove_remote(params, repos),
        "save_https_credential" => save_https_credential(params, repos),
        "forget_https_credential" => forget_https_credential(params, repos),
        "set_remote_auth_mode" => set_remote_auth_mode(params, repos),
        "set_current_upstream" => set_current_upstream(params, repos),
        "clear_current_upstream" => clear_current_upstream(params, repos),
        "list_tags" => list_tags(params, repos),
        "create_tag" => create_tag(params, repos),
        "delete_tag" => delete_tag(params, repos),
        "fetch_remote" => fetch_remote(params, repos, stdout),
        "push_current_branch" => push_current_branch(params, repos, stdout),
        "push_tags" => push_tags(params, repos, stdout),
        "pull_current_upstream" => pull_current_upstream(params, repos, stdout),
        "list_stashes" => list_stashes(params, repos),
        "save_stash" => save_stash(params, repos),
        "apply_stash" => apply_stash(params, repos),
        "drop_stash" => drop_stash(params, repos),
        "get_blame" => get_blame(params, repos),
        "start_merge" => start_merge(params, repos),
        "get_conflict_hunks" => get_conflict_hunks(params, repos),
        "resolve_conflict" => resolve_conflict(params, repos),
        "abort_merge" => abort_merge(params, repos),
        "get_merge_message" => get_merge_message(params, repos),
        "resolve_add_delete_conflict" => resolve_add_delete_conflict(params, repos),
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchInfoDto {
    name: String,
    is_current: bool,
}

impl From<BranchInfo> for BranchInfoDto {
    fn from(branch: BranchInfo) -> Self {
        Self {
            name: branch.name,
            is_current: branch.is_current,
        }
    }
}

fn list_branches(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let branches: Vec<BranchInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_branches()?
        .into_iter()
        .map(BranchInfoDto::from)
        .collect();
    serde_json::to_value(branches).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBranchParams {
    repo_path: String,
    name: String,
    start_point: String,
}

fn create_branch(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CreateBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.create_branch(params.name, params.start_point)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwitchBranchParams {
    repo_path: String,
    name: String,
}

fn switch_branch(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: SwitchBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.switch_branch(params.name)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteBranchParams {
    repo_path: String,
    name: String,
    force: bool,
}

fn delete_branch(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: DeleteBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.delete_branch(params.name, params.force)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameBranchParams {
    repo_path: String,
    old_name: String,
    new_name: String,
}

fn rename_branch(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RenameBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.rename_branch(params.old_name, params.new_name)?;
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorktreeInfoDto {
    name: String,
    path: String,
    head: Option<String>,
    is_main: bool,
    is_locked: bool,
    is_prunable: bool,
}

impl From<WorktreeInfo> for WorktreeInfoDto {
    fn from(worktree: WorktreeInfo) -> Self {
        Self {
            name: worktree.name,
            path: worktree.path.to_string_lossy().into_owned(),
            head: (!worktree.head.is_empty()).then_some(worktree.head),
            is_main: worktree.is_main,
            is_locked: worktree.is_locked,
            is_prunable: worktree.is_prunable,
        }
    }
}

fn list_worktrees(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let worktrees: Vec<WorktreeInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_worktrees()?
        .into_iter()
        .map(WorktreeInfoDto::from)
        .collect();
    serde_json::to_value(worktrees).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorktreeParams {
    repo_path: String,
    name: String,
    path: String,
    branch: String,
    start_point: Option<String>,
}

fn create_worktree(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CreateWorktreeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.create_worktree(
        params.name,
        PathBuf::from(params.path),
        params.branch,
        params.start_point,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorktreeNameParams {
    repo_path: String,
    name: String,
}

fn remove_worktree(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: WorktreeNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.remove_worktree(params.name)?;
    Ok(Value::Null)
}

fn prune_worktrees(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.prune_worktrees()?;
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmoduleInfoDto {
    path: String,
    url: Option<String>,
    gitlink_id: Option<String>,
    initialized: bool,
    head_id: Option<String>,
}

impl From<SubmoduleInfo> for SubmoduleInfoDto {
    fn from(submodule: SubmoduleInfo) -> Self {
        Self {
            path: submodule.path,
            url: submodule.url,
            gitlink_id: submodule.gitlink_id,
            initialized: submodule.initialized,
            head_id: submodule.head_id,
        }
    }
}

fn list_submodules(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let submodules: Vec<SubmoduleInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_submodules()?
        .into_iter()
        .map(SubmoduleInfoDto::from)
        .collect();
    serde_json::to_value(submodules).map_err(|error| error.to_string())
}

fn init_submodule(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.init_submodule(params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSubmoduleParams {
    repo_path: String,
    path: String,
    recursive: bool,
}

fn update_submodule(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: UpdateSubmoduleParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.update_submodule(params.path, params.recursive)?;
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReflogEntryDto {
    reference: String,
    old_id: String,
    new_id: String,
    committer_name: String,
    committer_email: String,
    timestamp: i64,
    message: String,
    summary: Option<String>,
}

impl From<ReflogEntry> for ReflogEntryDto {
    fn from(entry: ReflogEntry) -> Self {
        Self {
            reference: entry.reference,
            old_id: entry.old_id,
            new_id: entry.new_id,
            committer_name: entry.committer_name,
            committer_email: entry.committer_email,
            timestamp: entry.timestamp,
            message: entry.message,
            summary: entry.summary,
        }
    }
}

fn list_reflog_refs(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let refs = worker_handle(repos, &params.repo_path)?.list_reflog_refs()?;
    serde_json::to_value(refs).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetReflogParams {
    repo_path: String,
    reference: String,
}

fn get_reflog(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetReflogParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<ReflogEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_reflog(params.reference)?
        .into_iter()
        .map(ReflogEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreReflogEntryParams {
    repo_path: String,
    reference: String,
    new_id: String,
}

fn restore_reflog_entry(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: RestoreReflogEntryParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .restore_reflog_entry(params.reference, params.new_id)?;
    Ok(Value::Null)
}

#[derive(Clone, Serialize, Deserialize)]
enum RemoteAuthModeDto {
    HttpsToken,
    SshAgent,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteInfoDto {
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    auth_mode: Option<RemoteAuthModeDto>,
    auth_username: Option<String>,
}

impl
    From<(
        git_core::remote::RemoteInfo,
        Option<git_core::remote::RemoteAuthMode>,
    )> for RemoteInfoDto
{
    fn from(
        (remote, profile): (
            git_core::remote::RemoteInfo,
            Option<git_core::remote::RemoteAuthMode>,
        ),
    ) -> Self {
        let (auth_mode, auth_username) = match profile {
            Some(git_core::remote::RemoteAuthMode::HttpsToken { username }) => {
                (Some(RemoteAuthModeDto::HttpsToken), Some(username))
            }
            Some(git_core::remote::RemoteAuthMode::SshAgent) => {
                (Some(RemoteAuthModeDto::SshAgent), None)
            }
            None => (None, None),
        };
        Self {
            name: remote.name,
            fetch_url: remote.fetch_url,
            push_url: remote.push_url,
            auth_mode,
            auth_username,
        }
    }
}

fn list_remotes(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let worker = worker_handle(repos, &params.repo_path)?;
    let remotes: Result<Vec<RemoteInfoDto>, String> = worker
        .list_remotes()?
        .into_iter()
        .map(|remote| {
            let profile = worker.get_remote_auth_mode(remote.name.clone())?;
            Ok(RemoteInfoDto::from((remote, profile)))
        })
        .collect();
    serde_json::to_value(remotes?).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteNameParams {
    repo_path: String,
    remote_name: String,
}

fn list_remote_branches(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: RemoteNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let branches =
        worker_handle(repos, &params.repo_path)?.list_remote_branches(params.remote_name)?;
    serde_json::to_value(branches).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamInfoDto {
    local_branch: String,
    remote_name: String,
    remote_branch: String,
}

impl From<git_core::remote::UpstreamInfo> for UpstreamInfoDto {
    fn from(upstream: git_core::remote::UpstreamInfo) -> Self {
        Self {
            local_branch: upstream.local_branch,
            remote_name: upstream.remote_name,
            remote_branch: upstream.remote_branch,
        }
    }
}

fn get_current_upstream(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let upstream = worker_handle(repos, &params.repo_path)?
        .get_current_upstream()?
        .map(UpstreamInfoDto::from);
    serde_json::to_value(upstream).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetRemoteUpstreamsParams {
    repo_path: String,
    name: String,
}

fn get_remote_upstreams(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: GetRemoteUpstreamsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let upstreams: Vec<UpstreamInfoDto> = worker_handle(repos, &params.repo_path)?
        .get_remote_upstreams(params.name)?
        .into_iter()
        .map(UpstreamInfoDto::from)
        .collect();
    serde_json::to_value(upstreams).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddRemoteParams {
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
}

fn add_remote(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: AddRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.add_remote(
        params.name,
        params.fetch_url,
        params.push_url,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameRemoteParams {
    repo_path: String,
    old_name: String,
    new_name: String,
}

fn rename_remote(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RenameRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.rename_remote(params.old_name, params.new_name)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRemoteUrlsParams {
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
}

fn update_remote_urls(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: UpdateRemoteUrlsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.update_remote_urls(
        params.name,
        params.fetch_url,
        params.push_url,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoveRemoteParams {
    repo_path: String,
    name: String,
    clear_upstreams: bool,
}

fn remove_remote(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RemoveRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.remove_remote(params.name, params.clear_upstreams)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveHttpsCredentialParams {
    repo_path: String,
    remote_name: String,
    username: String,
    token: String,
}

fn save_https_credential(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: SaveHttpsCredentialParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.save_https_credential(
        params.remote_name,
        params.username,
        params.token,
    )?;
    Ok(Value::Null)
}

fn forget_https_credential(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: RemoteNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.forget_https_credential(params.remote_name)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetRemoteAuthModeParams {
    repo_path: String,
    remote_name: String,
    mode: RemoteAuthModeDto,
    username: Option<String>,
}

fn set_remote_auth_mode(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: SetRemoteAuthModeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let mode = match params.mode {
        RemoteAuthModeDto::HttpsToken => git_core::remote::RemoteAuthMode::HttpsToken {
            username: params
                .username
                .filter(|username| !username.trim().is_empty())
                .ok_or_else(|| "HTTPS username is required".to_string())?,
        },
        RemoteAuthModeDto::SshAgent => git_core::remote::RemoteAuthMode::SshAgent,
    };
    worker_handle(repos, &params.repo_path)?.set_remote_auth_mode(params.remote_name, mode)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetCurrentUpstreamParams {
    repo_path: String,
    remote_name: String,
    remote_branch: String,
}

fn set_current_upstream(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: SetCurrentUpstreamParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .set_current_upstream(params.remote_name, params.remote_branch)?;
    Ok(Value::Null)
}

fn clear_current_upstream(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.clear_current_upstream()?;
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TagInfoDto {
    name: String,
    target_id: String,
    annotated: bool,
    message: Option<String>,
    tagger_name: Option<String>,
    timestamp: Option<i64>,
}

impl From<TagInfo> for TagInfoDto {
    fn from(tag: TagInfo) -> Self {
        Self {
            name: tag.name,
            target_id: tag.target_id,
            annotated: tag.annotated,
            message: tag.message,
            tagger_name: tag.tagger_name,
            timestamp: tag.timestamp,
        }
    }
}

fn list_tags(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let tags: Vec<TagInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_tags()?
        .into_iter()
        .map(TagInfoDto::from)
        .collect();
    serde_json::to_value(tags).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTagParams {
    repo_path: String,
    name: String,
    message: Option<String>,
}

fn create_tag(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CreateTagParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.create_tag(params.name, params.message)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteTagParams {
    repo_path: String,
    name: String,
}

fn delete_tag(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: DeleteTagParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.delete_tag(params.name)?;
    Ok(Value::Null)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferProgressDto {
    operation_id: String,
    operation: String,
    phase: String,
    error_kind: Option<String>,
    current: usize,
    total: usize,
    received_bytes: usize,
    message: Option<String>,
}

fn transfer_progress_dto(event: TransferEvent) -> TransferProgressDto {
    match event {
        TransferEvent::Started {
            operation_id,
            operation,
        } => TransferProgressDto {
            operation_id,
            operation: format!("{operation:?}"),
            phase: "Starting".to_string(),
            error_kind: None,
            current: 0,
            total: 0,
            received_bytes: 0,
            message: None,
        },
        TransferEvent::Progress(progress) => TransferProgressDto {
            operation_id: progress.operation_id,
            operation: format!("{:?}", progress.operation),
            phase: format!("{:?}", progress.phase),
            error_kind: None,
            current: progress.current,
            total: progress.total,
            received_bytes: progress.received_bytes,
            // Sideband and reference-update text comes from the remote — never safe to forward
            // over IPC, even when it looks like ordinary progress output. Same redaction
            // `crates/tauri-app/src/commands/mod.rs`'s `TransferProgressDto::from` applies.
            message: None,
        },
        TransferEvent::Completed {
            operation_id,
            operation,
            error,
        } => {
            let failed = error.is_some();
            TransferProgressDto {
                operation_id,
                operation: format!("{operation:?}"),
                phase: if failed { "Failed" } else { "Completed" }.to_string(),
                error_kind: error.map(|kind| format!("{kind:?}")),
                current: 0,
                total: 0,
                received_bytes: 0,
                message: None,
            }
        }
    }
}

/// Drains `event_rx` on a dedicated thread, writing each event as a `transferProgress` JSON-RPC
/// notification (no `id`) to the shared `out`. Exits once every `Sender<TransferEvent>` clone
/// held inside `repo-service` for this operation is dropped — which happens when the worker
/// thread finishes the operation (or, if the request never reached a worker at all, when the
/// handler below returns and drops its own sender).
///
/// This per-operation thread is the one deliberate exception to the sidecar's otherwise
/// single-threaded design: it does no git work, it only forwards a channel to stdout. The lock is
/// taken for the whole write-plus-flush of one line, so notifications can never interleave with
/// the dispatch loop's responses mid-line.
fn spawn_progress_relay<W: std::io::Write + Send + 'static>(
    event_rx: mpsc::Receiver<TransferEvent>,
    out: Arc<Mutex<W>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for event in event_rx {
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "transferProgress",
                "params": transfer_progress_dto(event),
            });
            let Ok(line) = serde_json::to_string(&notification) else {
                continue;
            };
            let mut out = out.lock().unwrap_or_else(|error| error.into_inner());
            if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                // The reading end has gone away; stop relaying rather than spinning on a broken
                // pipe. The main loop notices the same failure and exits the process.
                return;
            }
        }
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchRemoteParams {
    repo_path: String,
    remote_name: String,
}

fn fetch_remote(
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: FetchRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id =
        worker_handle(repos, &params.repo_path)?.fetch_remote(params.remote_name, event_tx)?;
    Ok(Value::String(operation_id))
}

fn push_current_branch(
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: RemoteNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?
        .push_current_branch(params.remote_name, event_tx)?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushTagsParams {
    repo_path: String,
    remote_name: String,
    names: Vec<String>,
}

fn push_tags(
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: PushTagsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.push_tags(
        params.remote_name,
        params.names,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
enum PullOutcomeDto {
    UpToDate,
    FastForwarded { upstream_ref: String },
    Diverged { upstream_ref: String },
}

impl From<git_core::remote::PullOutcome> for PullOutcomeDto {
    fn from(outcome: git_core::remote::PullOutcome) -> Self {
        match outcome {
            git_core::remote::PullOutcome::UpToDate => PullOutcomeDto::UpToDate,
            git_core::remote::PullOutcome::FastForwarded { upstream_ref } => {
                PullOutcomeDto::FastForwarded { upstream_ref }
            }
            git_core::remote::PullOutcome::Diverged { upstream_ref } => {
                PullOutcomeDto::Diverged { upstream_ref }
            }
        }
    }
}

fn pull_current_upstream(
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.pull_current_upstream(event_tx)?;
    serde_json::to_value(PullOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StashEntryDto {
    index: usize,
    message: String,
    commit_id: String,
}

impl From<StashEntry> for StashEntryDto {
    fn from(entry: StashEntry) -> Self {
        Self {
            index: entry.index,
            message: entry.message,
            commit_id: entry.commit_id,
        }
    }
}

fn list_stashes(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let stashes: Vec<StashEntryDto> = worker_handle(repos, &params.repo_path)?
        .list_stashes()?
        .into_iter()
        .map(StashEntryDto::from)
        .collect();
    serde_json::to_value(stashes).map_err(|error| error.to_string())
}

fn save_stash(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.save_stash()?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StashIndexParams {
    repo_path: String,
    index: usize,
}

fn apply_stash(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: StashIndexParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.apply_stash(params.index)?;
    Ok(Value::Null)
}

fn drop_stash(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: StashIndexParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.drop_stash(params.index)?;
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlameLineDto {
    line_number: usize,
    content: String,
    commit_id: String,
    short_id: String,
    author_name: String,
    timestamp: i64,
}

impl From<BlameLine> for BlameLineDto {
    fn from(line: BlameLine) -> Self {
        Self {
            line_number: line.line_number,
            content: line.content,
            commit_id: line.commit_id,
            short_id: line.short_id,
            author_name: line.author_name,
            timestamp: line.timestamp,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetBlameParams {
    repo_path: String,
    commit_id: String,
    path: String,
}

fn get_blame(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetBlameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let lines: Vec<BlameLineDto> = worker_handle(repos, &params.repo_path)?
        .get_blame(params.commit_id, params.path)?
        .into_iter()
        .map(BlameLineDto::from)
        .collect();
    serde_json::to_value(lines).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum MergeOutcomeDto {
    UpToDate,
    FastForwarded,
    Merged,
    Conflicted { files: Vec<String> },
}

impl From<MergeOutcome> for MergeOutcomeDto {
    fn from(outcome: MergeOutcome) -> Self {
        match outcome {
            MergeOutcome::UpToDate => MergeOutcomeDto::UpToDate,
            MergeOutcome::FastForwarded => MergeOutcomeDto::FastForwarded,
            MergeOutcome::Merged => MergeOutcomeDto::Merged,
            MergeOutcome::Conflicted { files } => MergeOutcomeDto::Conflicted { files },
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartMergeParams {
    repo_path: String,
    branch_name: String,
}

fn start_merge(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: StartMergeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.start_merge(params.branch_name)?;
    serde_json::to_value(MergeOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum ConflictSegmentDto {
    Clean { content: String },
    Conflict { ours: String, theirs: String },
}

impl From<ConflictSegment> for ConflictSegmentDto {
    fn from(segment: ConflictSegment) -> Self {
        match segment {
            ConflictSegment::Clean { content } => ConflictSegmentDto::Clean { content },
            ConflictSegment::Conflict { ours, theirs } => {
                ConflictSegmentDto::Conflict { ours, theirs }
            }
        }
    }
}

fn get_conflict_hunks(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let segments: Vec<ConflictSegmentDto> = worker_handle(repos, &params.repo_path)?
        .get_conflict_hunks(params.path)?
        .into_iter()
        .map(ConflictSegmentDto::from)
        .collect();
    serde_json::to_value(segments).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveConflictParams {
    repo_path: String,
    path: String,
    resolved_content: String,
}

fn resolve_conflict(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: ResolveConflictParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .resolve_conflict(params.path, params.resolved_content)?;
    Ok(Value::Null)
}

fn abort_merge(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.abort_merge()?;
    Ok(Value::Null)
}

fn get_merge_message(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let message = worker_handle(repos, &params.repo_path)?.get_merge_message()?;
    serde_json::to_value(message).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
enum FileConflictChoiceDto {
    Ours,
    Theirs,
    Delete,
}

impl From<FileConflictChoiceDto> for FileConflictChoice {
    fn from(dto: FileConflictChoiceDto) -> Self {
        match dto {
            FileConflictChoiceDto::Ours => FileConflictChoice::Ours,
            FileConflictChoiceDto::Theirs => FileConflictChoice::Theirs,
            FileConflictChoiceDto::Delete => FileConflictChoice::Delete,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveAddDeleteConflictParams {
    repo_path: String,
    path: String,
    choice: FileConflictChoiceDto,
}

fn resolve_add_delete_conflict(
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    let params: ResolveAddDeleteConflictParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .resolve_add_delete_conflict(params.path, params.choice.into())?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};

    fn lines_of(buffer: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
        let written = buffer.lock().expect("buffer lock").clone();
        String::from_utf8(written)
            .expect("relay output is utf-8")
            .lines()
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|error| panic!("incomplete line {line:?}: {error}"))
            })
            .collect()
    }

    #[test]
    fn progress_relay_writes_one_notification_line_per_event_then_exits_when_the_channel_closes() {
        let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let (event_tx, event_rx) = mpsc::channel();
        let relay = spawn_progress_relay(event_rx, Arc::clone(&buffer));

        event_tx
            .send(TransferEvent::Started {
                operation_id: "fetch-1".to_string(),
                operation: TransferOperation::Fetch,
            })
            .unwrap();
        event_tx
            .send(TransferEvent::Progress(TransferProgress {
                operation_id: "fetch-1".to_string(),
                operation: TransferOperation::Fetch,
                phase: TransferPhase::Receiving,
                current: 3,
                total: 10,
                received_bytes: 128,
                message: Some("remote: sideband text from the server".to_string()),
            }))
            .unwrap();
        event_tx
            .send(TransferEvent::Completed {
                operation_id: "fetch-1".to_string(),
                operation: TransferOperation::Fetch,
                error: Some(TransferErrorKind::TransferFailed),
            })
            .unwrap();

        // Dropping the last sender is the relay's only shutdown signal — exactly what
        // `repo-service` does once the worker thread finishes the operation. Nothing joins this
        // thread in production, so if it did not exit on its own it would leak one thread per
        // transfer; this join is the test that it does.
        drop(event_tx);
        relay
            .join()
            .expect("relay thread exits once its channel's senders are dropped");

        let lines = lines_of(&buffer);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| line["jsonrpc"] == "2.0"
            && line["method"] == "transferProgress"
            && line.get("id").is_none()));
        assert_eq!(
            lines[0]["params"],
            serde_json::json!({
                "operationId": "fetch-1",
                "operation": "Fetch",
                "phase": "Starting",
                "errorKind": null,
                "current": 0,
                "total": 0,
                "receivedBytes": 0,
                "message": null,
            })
        );
        // The sideband text above is dropped, not forwarded — same redaction
        // `crates/tauri-app/src/commands/mod.rs` applies on the Tauri transport.
        assert_eq!(
            lines[1]["params"],
            serde_json::json!({
                "operationId": "fetch-1",
                "operation": "Fetch",
                "phase": "Receiving",
                "errorKind": null,
                "current": 3,
                "total": 10,
                "receivedBytes": 128,
                "message": null,
            })
        );
        assert_eq!(
            lines[2]["params"],
            serde_json::json!({
                "operationId": "fetch-1",
                "operation": "Fetch",
                "phase": "Failed",
                "errorKind": "TransferFailed",
                "current": 0,
                "total": 0,
                "receivedBytes": 0,
                "message": null,
            })
        );
    }

    #[test]
    fn concurrent_relays_sharing_one_writer_never_interleave_a_line() {
        // Two transfers can be in flight at once, each with its own relay thread, all writing to
        // the one shared stdout. The mutex is held across write-plus-flush, so every line must
        // still arrive whole and attributable to exactly one operation.
        let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let (first_tx, first_rx) = mpsc::channel();
        let (second_tx, second_rx) = mpsc::channel();
        let first = spawn_progress_relay(first_rx, Arc::clone(&buffer));
        let second = spawn_progress_relay(second_rx, Arc::clone(&buffer));

        let senders = [(first_tx, "fetch-1"), (second_tx, "push-2")];
        let feeders: Vec<_> = senders
            .into_iter()
            .map(|(tx, operation_id)| {
                std::thread::spawn(move || {
                    for index in 0..200 {
                        tx.send(TransferEvent::Progress(TransferProgress {
                            operation_id: operation_id.to_string(),
                            operation: TransferOperation::Fetch,
                            phase: TransferPhase::Receiving,
                            current: index,
                            total: 200,
                            received_bytes: index * 16,
                            message: None,
                        }))
                        .unwrap();
                    }
                })
            })
            .collect();
        for feeder in feeders {
            feeder.join().expect("feeder thread");
        }
        first.join().expect("first relay exits");
        second.join().expect("second relay exits");

        // `lines_of` parses every line, so a torn write fails here.
        let lines = lines_of(&buffer);
        assert_eq!(lines.len(), 400);
        assert_eq!(
            lines
                .iter()
                .filter(|line| line["params"]["operationId"] == "fetch-1")
                .count(),
            200
        );
        assert_eq!(
            lines
                .iter()
                .filter(|line| line["params"]["operationId"] == "push-2")
                .count(),
            200
        );
    }

    #[test]
    fn pull_outcome_serializes_with_a_kind_tag_and_camel_case_fields() {
        assert_eq!(
            serde_json::to_value(PullOutcomeDto::from(
                git_core::remote::PullOutcome::UpToDate
            ))
            .unwrap(),
            serde_json::json!({"kind": "UpToDate"})
        );
        assert_eq!(
            serde_json::to_value(PullOutcomeDto::from(
                git_core::remote::PullOutcome::FastForwarded {
                    upstream_ref: "refs/remotes/origin/main".to_string(),
                }
            ))
            .unwrap(),
            serde_json::json!({"kind": "FastForwarded", "upstreamRef": "refs/remotes/origin/main"})
        );
        assert_eq!(
            serde_json::to_value(PullOutcomeDto::from(
                git_core::remote::PullOutcome::Diverged {
                    upstream_ref: "refs/remotes/origin/main".to_string(),
                }
            ))
            .unwrap(),
            serde_json::json!({"kind": "Diverged", "upstreamRef": "refs/remotes/origin/main"})
        );
    }
}
