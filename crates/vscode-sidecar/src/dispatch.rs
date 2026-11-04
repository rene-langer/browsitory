use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use repo_service::wire::*;
use repo_service::worker::{HookEvent, TransferEvent, WorkerRegistry};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Shared, thread-safe handle to the sidecar's open repositories: one worker per repository,
/// whatever path spelling a request uses (#223, see `WorkerRegistry`).
///
/// `main.rs` spawns one thread per incoming request (see its module doc comment) so a slow
/// operation against one repo can never block another request — including one against a
/// *different* repo — from being read and dispatched. That only holds if the registry's lock is
/// never held across a blocking call: `open_repo`/`close_repo` only update it (the registry
/// spawns a worker unlocked), and every other handler calls `worker_handle` to clone out a
/// `WorkerHandle` (a cheap channel `Sender` clone) and sends its command and blocks on the reply
/// after the lock is released — the same "clone the handle, drop the guard, then block"
/// discipline `docs/ARCHITECTURE.md` describes for `tauri-app`'s `AppState`.
pub type Repos = Arc<WorkerRegistry>;

/// Test-only hook: sleeping here before dispatch runs, gated behind two env vars production
/// callers (the VSCode extension) never set, lets `tests/protocol_roundtrip.rs` deterministically
/// simulate a slow request (e.g. a large `get_blame`/`get_commit_graph`) without constructing a
/// synthetic multi-thousand-commit repository. Inert unless both env vars are set and the target
/// method matches, so it has no effect on production behavior.
fn test_delay_before_dispatch(method: &str) {
    let Ok(target_method) = std::env::var("BROWSITORY_SIDECAR_TEST_DELAY_METHOD") else {
        return;
    };
    if target_method != method {
        return;
    }
    let Ok(millis) = std::env::var("BROWSITORY_SIDECAR_TEST_DELAY_MS") else {
        return;
    };
    if let Ok(millis) = millis.parse::<u64>() {
        std::thread::sleep(std::time::Duration::from_millis(millis));
    }
}

/// Declares the sidecar's methods once: `dispatch` routes them, and `METHODS` lists the same
/// names for the transport parity check (#306), which compares it with
/// `frontend/src/ipc/generated/sidecar-methods.json`. The arms name the request's `params`,
/// `repos` and `stdout` through the idents given first (macro hygiene).
macro_rules! methods {
    (|$params:ident, $repos:ident, $stdout:ident| { $($name:literal => $call:expr),* $(,)? }) => {
        pub fn dispatch(
            method: &str,
            $params: Value,
            $repos: &Repos,
            $stdout: &Arc<Mutex<std::io::Stdout>>,
        ) -> Result<Value, String> {
            test_delay_before_dispatch(method);
            match method {
                $($name => $call,)*
                other => Err(format!("unknown method: {other}")),
            }
        }

        #[cfg(test)]
        const METHODS: &[&str] = &[$($name),*];
    };
}

methods!(|params, repos, stdout| {
    "open_repo" => open_repo(params, repos),
    "close_repo" => close_repo(params, repos),
    "get_status" => get_status(params, repos),
    "get_commit_graph" => get_commit_graph(params, repos),
    "get_working_diff" => get_working_diff(params, repos),
    "get_commit_diff" => get_commit_diff(params, repos),
    "get_file_history" => get_file_history(params, repos),
    "search_commits" => search_commits(params, repos),
    "cancel_search" => cancel_search(params, repos),
    "get_commit_rename_diff" => get_commit_rename_diff(params, repos),
    "get_range_files" => get_range_files(params, repos),
    "get_range_diff" => get_range_diff(params, repos),
    "get_working_tree_compare_files" => get_working_tree_compare_files(params, repos),
    "get_working_tree_compare_diff" => get_working_tree_compare_diff(params, repos),
    "get_branch_compare_range" => get_branch_compare_range(params, repos),
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
    "get_diff_settings" => get_diff_settings(),
    "set_diff_settings" => set_diff_settings(params),
    "get_external_tool_settings" => get_external_tool_settings(),
    "set_external_tool_settings" => set_external_tool_settings(params),
    "get_git_tools" => get_git_tools(params, repos),
    "get_file_sides" => get_file_sides(params, repos),
    "list_recent_authors" => list_recent_authors(params, repos),
    "get_commit_message_settings" => get_commit_message_settings(),
    "set_commit_message_settings" => set_commit_message_settings(params),
    "mark_resolved" => mark_resolved(params, repos),
    "resolve_whole_file" => resolve_whole_file(params, repos),
    "read_file_version" => read_file_version(params, repos),
    "repo_workdir" => repo_workdir(params, repos),
    "check_merge_target" => check_merge_target(params, repos),
    "get_graph_remote_branch_selection" => get_graph_remote_branch_selection(params),
    "set_graph_remote_branch_selection" => set_graph_remote_branch_selection(params),
    "get_commit_files" => get_commit_files(params, repos),
    "get_commit_message" => get_commit_message(params, repos),
    "get_commit_signature" => get_commit_signature(params, repos),
    "stage_file" => stage_file(params, repos),
    "unstage_file" => unstage_file(params, repos),
    "stage_hunk" => stage_hunk(params, repos),
    "unstage_hunk" => unstage_hunk(params, repos),
    "discard_hunk" => discard_hunk(params, repos),
    "stage_lines" => stage_lines(params, repos),
    "unstage_lines" => unstage_lines(params, repos),
    "discard_lines" => discard_lines(params, repos),
    "discard_file" => discard_file(params, repos),
    "discard_all_unstaged" => discard_all_unstaged(params, repos),
    "ignore_path" => ignore_path(params, repos),
    "commit" => commit(params, repos, stdout),
    "list_branches" => list_branches(params, repos),
    "create_branch" => create_branch(params, repos),
    "switch_branch" => switch_branch(params, repos, stdout),
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
    "cancel_transfer" => cancel_transfer(params, repos),
    "push_current_branch" => push_current_branch(params, repos, stdout),
    "force_push_current_branch" => force_push_current_branch(params, repos, stdout),
    "get_head_info" => get_head_info(params, repos),
    "amend_commit" => amend_commit(params, repos, stdout),
    "get_undo_state" => get_undo_state(params, repos),
    "undo_last" => undo_last(params, repos),
    "redo_last" => redo_last(params, repos),
    "cherry_pick" => cherry_pick(params, repos, stdout),
    "revert_commit" => revert_commit(params, repos, stdout),
    "cherry_pick_many" => cherry_pick_many(params, repos, stdout),
    "revert_many" => revert_many(params, repos, stdout),
    "continue_sequence" => continue_sequence(params, repos, stdout),
    "abort_sequence" => abort_sequence(params, repos),
    "reset_to_commit" => reset_to_commit(params, repos),
    "checkout_commit" => checkout_commit(params, repos, stdout),
    "push_tags" => push_tags(params, repos, stdout),
    "fetch_all_remotes" => fetch_all_remotes(params, repos, stdout),
    "push_branch" => push_branch(params, repos, stdout),
    "delete_remote_branch" => delete_remote_branch(params, repos, stdout),
    "pull_current_upstream" => pull_current_upstream(params, repos, stdout),
    "list_stashes" => list_stashes(params, repos),
    "save_stash" => save_stash(params, repos),
    "apply_stash" => apply_stash(params, repos),
    "pop_stash" => pop_stash(params, repos),
    "get_stash_files" => get_stash_files(params, repos),
    "drop_stash" => drop_stash(params, repos),
    "get_blame" => get_blame(params, repos),
    "start_merge" => start_merge(params, repos, stdout),
    "get_merge_options" => get_merge_options(params, repos),
    "set_merge_options" => set_merge_options(params),
    "get_conflict_hunks" => get_conflict_hunks(params, repos),
    "resolve_conflict" => resolve_conflict(params, repos),
    "abort_merge" => abort_merge(params, repos),
    "get_merge_message" => get_merge_message(params, repos),
    "resolve_add_delete_conflict" => resolve_add_delete_conflict(params, repos),
    "commits_since" => commits_since(params, repos),
    "start_rebase" => start_rebase(params, repos),
    "rebase_continue" => rebase_continue(params, repos),
    "abort_rebase" => abort_rebase(params, repos),
    "detect_forge_repository" => detect_forge_repository(params, repos),
    "save_forge_token" => save_forge_token(params, repos),
    "forget_forge_token" => forget_forge_token(params, repos),
    "list_pull_requests" => list_pull_requests(params, repos),
    "create_pull_request" => create_pull_request(params, repos),
});

/// Locks `repos` only long enough to clone out a `WorkerHandle` (a cheap channel `Sender`
/// clone), then drops the guard before returning. `repo_path` is a spelling `open_repo` was
/// called with; any open spelling of a repository reaches its one worker. Callers always send their command and block
/// on its reply *after* this function has returned, i.e. with the map lock already released —
/// that's what lets one repo's slow operation run without blocking access to any other repo's
/// entry in this map.
fn worker_handle(
    repos: &Repos,
    repo_path: &str,
) -> Result<repo_service::worker::WorkerHandle, String> {
    repos
        .get(repo_path)
        .map(|(_, handle)| handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))
}

#[derive(Deserialize)]
struct OpenRepoParams {
    path: String,
}

fn open_repo(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: OpenRepoParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    // `WorkerRegistry::open` doesn't hold its lock across `Worker::spawn`, and evicts a dead
    // worker (its thread having exited, e.g. from a panic inside a `Command` match arm) before
    // deciding whether to reuse or respawn — a plain `contains_key`/`Entry::Vacant` fast path
    // would keep handing back a dead entry forever. See AUD-2026-09-05-CONC-001. A second
    // spelling of an open repository shares its worker (#223).
    repos.open(&params.path)?;
    let _ = config::add_recent_repo(Path::new(&params.path));
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoPathParams {
    repo_path: String,
}

fn close_repo(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    // Drops the worker only when no other spelling of the repository is still open.
    repos.close(&params.repo_path);
    Ok(Value::Null)
}

fn get_status(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn get_commit_graph(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitGraphParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let commits: Vec<GraphCommitDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_graph(params.limit, params.selected_branches)?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetWorkingDiffParams {
    repo_path: String,
    path: String,
    staged: bool,
    opts: DiffOptionsDto,
    /// A staged rename's old path (optional, default `null`).
    #[serde(default)]
    old_path: Option<String>,
}

fn get_working_diff(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetWorkingDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
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
struct GetCommitDiffParams {
    repo_path: String,
    commit_id: String,
    path: String,
    opts: DiffOptionsDto,
}

fn get_commit_diff(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_diff(params.commit_id, params.path, params.opts.into())?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetFileHistoryParams {
    repo_path: String,
    path: String,
    limit: usize,
}

fn get_file_history(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetFileHistoryParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<FileHistoryEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_file_history(params.path, params.limit)?
        .into_iter()
        .map(FileHistoryEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchCommitsParams {
    repo_path: String,
    query: String,
    refs: Option<Vec<String>>,
    cursor: Option<String>,
}

fn search_commits(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: SearchCommitsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let page = worker_handle(repos, &params.repo_path)?.search_commits(
        params.query,
        params.refs,
        params.cursor,
    )?;
    serde_json::to_value(SearchPageDto::from(page)).map_err(|error| error.to_string())
}

/// Stops the repo's in-flight search, if any. Every request runs on its own thread (see
/// `main.rs`), and the cancel goes through the worker's side channel, so it never waits for the
/// search it stops.
fn cancel_search(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.cancel_search();
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitRenameDiffParams {
    repo_path: String,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: DiffOptionsDto,
}

fn get_commit_rename_diff(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitRenameDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetRangeFilesParams {
    repo_path: String,
    from_id: String,
    to_id: String,
}

fn get_range_files(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetRangeFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files: Vec<ChangedFileDto> = worker_handle(repos, &params.repo_path)?
        .get_range_files(params.from_id, params.to_id)?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect();
    serde_json::to_value(files).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetRangeDiffParams {
    repo_path: String,
    from_id: String,
    to_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
}

fn get_range_diff(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetRangeDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_range_diff(
            params.from_id,
            params.to_id,
            params.path,
            params.old_path,
            params.opts.into(),
        )?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

fn get_working_tree_compare_files(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files: Vec<ChangedFileDto> = worker_handle(repos, &params.repo_path)?
        .get_working_tree_compare_files(params.commit_id)?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect();
    serde_json::to_value(files).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetWorkingTreeCompareDiffParams {
    repo_path: String,
    commit_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
}

fn get_working_tree_compare_diff(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetWorkingTreeCompareDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_working_tree_compare_diff(
            params.commit_id,
            params.path,
            params.old_path,
            params.opts.into(),
        )?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetBranchCompareRangeParams {
    repo_path: String,
    ref_name: String,
}

fn get_branch_compare_range(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetBranchCompareRangeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let range =
        worker_handle(repos, &params.repo_path)?.get_branch_compare_range(params.ref_name)?;
    serde_json::to_value(CompareRangeDto::from(range)).map_err(|error| error.to_string())
}

fn list_recent_repos() -> Result<Value, String> {
    let paths = config::list_recent_repos().map_err(|error| error.to_string())?;
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    serde_json::to_value(paths).map_err(|error| error.to_string())
}

/// `list_open_repos`'s answer. (The desktop app's command returns an `(entries, activePath)`
/// tuple instead, which `tauriRepoClient.ts` turns into this shape.)
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "OpenRepos"))]
#[serde(rename_all = "camelCase")]
struct OpenReposDto {
    entries: Vec<OpenRepoEntryDto>,
    active_path: Option<String>,
}

/// `config::list_open_repos`'s `(entries, active path)`.
impl From<(Vec<config::OpenRepoEntry>, Option<PathBuf>)> for OpenReposDto {
    fn from((entries, active): (Vec<config::OpenRepoEntry>, Option<PathBuf>)) -> Self {
        Self {
            entries: entries.into_iter().map(OpenRepoEntryDto::from).collect(),
            active_path: active.map(|p| p.to_string_lossy().into_owned()),
        }
    }
}

fn list_open_repos_handler() -> Result<Value, String> {
    let open = config::list_open_repos().map_err(|error| error.to_string())?;
    serde_json::to_value(OpenReposDto::from(open)).map_err(|error| error.to_string())
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

fn get_graph_remote_branch_selection(params: Value) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let selection = config::get_graph_remote_branch_selection(Path::new(&params.repo_path))
        .map_err(|error| error.to_string())?;
    serde_json::to_value(selection).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetGraphRemoteBranchSelectionParams {
    repo_path: String,
    selected_branches: Vec<String>,
}

fn set_graph_remote_branch_selection(params: Value) -> Result<Value, String> {
    let params: SetGraphRemoteBranchSelectionParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::set_graph_remote_branch_selection(
        Path::new(&params.repo_path),
        &params.selected_branches,
    )
    .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitFilesParams {
    repo_path: String,
    commit_id: String,
}

fn get_commit_files(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files = worker_handle(repos, &params.repo_path)?.get_commit_files(params.commit_id)?;
    serde_json::to_value(files).map_err(|error| error.to_string())
}

fn get_commit_message(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let message = worker_handle(repos, &params.repo_path)?.get_commit_message(params.commit_id)?;
    serde_json::to_value(message).map_err(|error| error.to_string())
}

fn get_commit_signature(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let signature =
        worker_handle(repos, &params.repo_path)?.get_commit_signature(params.commit_id)?;
    serde_json::to_value(CommitSignatureDto::from(signature)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoFilePathParams {
    repo_path: String,
    path: String,
}

fn stage_file(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.stage_file(params.path)?;
    Ok(Value::Null)
}

fn unstage_file(params: Value, repos: &Repos) -> Result<Value, String> {
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
    context_lines: u32,
}

fn stage_hunk(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.stage_hunk(
        params.path,
        params.old_start,
        params.new_start,
        params.context_lines,
    )?;
    Ok(Value::Null)
}

fn unstage_hunk(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.unstage_hunk(
        params.path,
        params.old_start,
        params.new_start,
        params.context_lines,
    )?;
    Ok(Value::Null)
}

fn discard_hunk(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
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
struct LinesParams {
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    /// Every line's `content` of the hunk as the webview displayed it (stale-view guard).
    expected_lines: Vec<String>,
}

fn stage_lines(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: LinesParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
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

fn unstage_lines(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: LinesParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
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

fn discard_lines(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: LinesParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
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

fn discard_file(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.discard_file(params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscardAllParams {
    repo_path: String,
    /// Exactly the paths the confirm dialog listed.
    paths: Vec<String>,
}

fn discard_all_unstaged(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: DiscardAllParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.discard_all_unstaged(params.paths)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IgnorePathParams {
    repo_path: String,
    pattern: String,
}

fn ignore_path(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: IgnorePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.ignore_path(params.pattern)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct SetDiffSettingsParams {
    settings: DiffSettingsDto,
}

fn get_diff_settings() -> Result<Value, String> {
    let settings = config::get_diff_settings().map_err(|error| error.to_string())?;
    serde_json::to_value(DiffSettingsDto::from(settings)).map_err(|error| error.to_string())
}

fn set_diff_settings(params: Value) -> Result<Value, String> {
    let params: SetDiffSettingsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::set_diff_settings(&params.settings.into()).map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct SetExternalToolSettingsParams {
    settings: ExternalToolSettingsDto,
}

fn get_external_tool_settings() -> Result<Value, String> {
    let settings = config::get_external_tool_settings().map_err(|error| error.to_string())?;
    serde_json::to_value(ExternalToolSettingsDto::from(settings)).map_err(|error| error.to_string())
}

fn set_external_tool_settings(params: Value) -> Result<Value, String> {
    let params: SetExternalToolSettingsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    git_core::launch::check_tool_commands(
        params.settings.editor.as_deref(),
        params.settings.terminal.as_deref(),
    )?;
    config::set_external_tool_settings(&params.settings.into())
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

fn get_git_tools(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let tools = worker_handle(repos, &params.repo_path)?.get_git_tools()?;
    serde_json::to_value(GitToolsDto::from(tools)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkResolvedParams {
    repo_path: String,
    path: String,
    force: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveWholeFileParams {
    repo_path: String,
    path: String,
    side: ConflictSideDto,
}

fn resolve_whole_file(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ResolveWholeFileParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.resolve_whole_file(params.path, params.side.into())?;
    Ok(Value::Null)
}

fn mark_resolved(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: MarkResolvedParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome =
        worker_handle(repos, &params.repo_path)?.mark_resolved(params.path, params.force)?;
    serde_json::to_value(MarkResolvedOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "FileVersion"))]
#[serde(tag = "kind")]
enum FileVersionDto {
    Head,
    Index,
    Stage {
        stage: u8,
    },
    #[serde(rename_all = "camelCase")]
    Commit {
        commit_id: String,
    },
    #[serde(rename_all = "camelCase")]
    FirstParent {
        commit_id: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadFileVersionParams {
    repo_path: String,
    path: String,
    version: FileVersionDto,
}

/// Internal: not on `RepoClient`. Called by the extension host's `browsitory-file:` content
/// provider (diff and merge editor sides). Contents are lossy UTF-8, capped at
/// `MAX_FILE_VERSION_BYTES`; `text: null` when the path doesn't exist in that version.
fn read_file_version(params: Value, repos: &Repos) -> Result<Value, String> {
    use git_core::tools::FileVersion;
    let params: ReadFileVersionParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let version = match params.version {
        FileVersionDto::Head => FileVersion::Head,
        FileVersionDto::Index => FileVersion::Index,
        FileVersionDto::Stage { stage } if (1..=3).contains(&stage) => FileVersion::Stage(stage),
        FileVersionDto::Stage { .. } => return Err("stage must be 1, 2 or 3".into()),
        FileVersionDto::Commit { commit_id } => FileVersion::Commit(commit_id),
        FileVersionDto::FirstParent { commit_id } => FileVersion::FirstParent(commit_id),
    };
    let bytes = worker_handle(repos, &params.repo_path)?.read_file_version(params.path, version)?;
    Ok(serde_json::json!({ "text": bytes.map(|b| String::from_utf8_lossy(&b).into_owned()) }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoFileParams {
    repo_path: String,
    path: String,
}

/// Internal: not on `RepoClient`. Called by the extension host before it opens VSCode's merge
/// editor, with the same checks as the desktop merge tool: `null`, or an error saying why the
/// path can't be merged there.
fn check_merge_target(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoFileParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.check_merge_target(params.path)?;
    Ok(Value::Null)
}

/// Internal: not on `RepoClient`. The work tree root for the extension host's native launch
/// methods (the repo path may be a subdirectory of it).
fn repo_workdir(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let workdir = worker_handle(repos, &params.repo_path)?.workdir()?;
    Ok(Value::String(workdir.to_string_lossy().into_owned()))
}

/// `commit` / `amend_commit` params; `noVerify` (skip `pre-commit` and `commit-msg`) defaults
/// to `false`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitMessageParams {
    repo_path: String,
    message: String,
    #[serde(default)]
    no_verify: bool,
    /// Appended before the hooks run; absent means none.
    #[serde(default)]
    trailers: Vec<TrailerDto>,
    /// `amend_commit` only: replaces HEAD's author name and email.
    #[serde(default)]
    author: Option<SignatureInputDto>,
}

fn trailers(dtos: Vec<TrailerDto>) -> Vec<git_core::commit::Trailer> {
    dtos.into_iter().map(Into::into).collect()
}

fn list_recent_authors(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let authors: Vec<SignatureInputDto> = worker_handle(repos, &params.repo_path)?
        .list_recent_authors()?
        .into_iter()
        .map(SignatureInputDto::from)
        .collect();
    serde_json::to_value(authors).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileSidesParams {
    repo_path: String,
    path: String,
    source: DiffSourceDto,
}

fn get_file_sides(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: FileSidesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let sides = worker_handle(repos, &params.repo_path)?
        .get_file_sides(params.path, params.source.into())?;
    serde_json::to_value(FileSidesDto::from(sides)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
struct SetCommitMessageSettingsParams {
    settings: CommitMessageSettingsDto,
}

fn get_commit_message_settings() -> Result<Value, String> {
    let settings = config::get_commit_message_settings().map_err(|error| error.to_string())?;
    serde_json::to_value(CommitMessageSettingsDto::from(settings))
        .map_err(|error| error.to_string())
}

fn set_commit_message_settings(params: Value) -> Result<Value, String> {
    let params: SetCommitMessageSettingsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::set_commit_message_settings(&params.settings.into())
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

/// A hook event channel relayed as `hookProgress` notifications for `repo_path`.
fn hook_relay<W: std::io::Write + Send + 'static>(
    repo_path: &str,
    out: &Arc<Mutex<W>>,
) -> mpsc::Sender<HookEvent> {
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(repo_path.to_string(), event_rx, Arc::clone(out));
    event_tx
}

/// Writes each hook event as a `hookProgress` notification; same framing and shutdown as
/// `spawn_progress_relay`.
fn spawn_hook_relay<W: std::io::Write + Send + 'static>(
    repo_path: String,
    event_rx: mpsc::Receiver<HookEvent>,
    out: Arc<Mutex<W>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for event in event_rx {
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "hookProgress",
                "params": hook_progress_dto(&repo_path, event),
            });
            let Ok(line) = serde_json::to_string(&notification) else {
                continue;
            };
            let mut out = out.lock().unwrap_or_else(|error| error.into_inner());
            if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                return;
            }
        }
    })
}

fn commit(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: CommitMessageParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(params.repo_path.clone(), event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.commit(
        params.message,
        params.no_verify,
        trailers(params.trailers),
        event_tx,
    )?;
    serde_json::to_value(CommitOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn list_branches(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn create_branch(params: Value, repos: &Repos) -> Result<Value, String> {
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

/// `post-checkout` is relayed as `hookProgress` with `source: "Checkout"` (#185).
fn switch_branch<W: std::io::Write + Send + 'static>(
    params: Value,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, String> {
    let params: SwitchBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let events = hook_relay(&params.repo_path, out);
    worker_handle(repos, &params.repo_path)?.switch_branch(params.name, events)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteBranchParams {
    repo_path: String,
    name: String,
    force: bool,
}

fn delete_branch(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn rename_branch(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RenameBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.rename_branch(params.old_name, params.new_name)?;
    Ok(Value::Null)
}

fn list_worktrees(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn create_worktree(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn remove_worktree(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: WorktreeNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.remove_worktree(params.name)?;
    Ok(Value::Null)
}

fn prune_worktrees(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.prune_worktrees()?;
    Ok(Value::Null)
}

fn list_submodules(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let submodules: Vec<SubmoduleInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_submodules()?
        .into_iter()
        .map(SubmoduleInfoDto::from)
        .collect();
    serde_json::to_value(submodules).map_err(|error| error.to_string())
}

fn init_submodule(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn update_submodule(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: UpdateSubmoduleParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.update_submodule(params.path, params.recursive)?;
    Ok(Value::Null)
}

fn list_reflog_refs(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn get_reflog(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn restore_reflog_entry(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RestoreReflogEntryParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .restore_reflog_entry(params.reference, params.new_id)?;
    Ok(Value::Null)
}

fn list_remotes(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn list_remote_branches(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RemoteNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let branches =
        worker_handle(repos, &params.repo_path)?.list_remote_branches(params.remote_name)?;
    serde_json::to_value(branches).map_err(|error| error.to_string())
}

fn get_current_upstream(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn get_remote_upstreams(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn add_remote(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn rename_remote(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn update_remote_urls(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn remove_remote(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn save_https_credential(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: SaveHttpsCredentialParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.save_https_credential(
        params.remote_name,
        params.username,
        params.token,
    )?;
    Ok(Value::Null)
}

fn forget_https_credential(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn set_remote_auth_mode(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn set_current_upstream(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: SetCurrentUpstreamParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .set_current_upstream(params.remote_name, params.remote_branch)?;
    Ok(Value::Null)
}

fn clear_current_upstream(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.clear_current_upstream()?;
    Ok(Value::Null)
}

fn list_tags(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn create_tag(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn delete_tag(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: DeleteTagParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.delete_tag(params.name)?;
    Ok(Value::Null)
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
    /// `null`/absent = git config decides (`remote.<name>.prune`, then `fetch.prune`).
    #[serde(default)]
    prune: Option<bool>,
}

fn fetch_remote(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: FetchRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.fetch_remote(
        params.remote_name,
        params.prune,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchAllRemotesParams {
    repo_path: String,
    #[serde(default)]
    prune: Option<bool>,
}

fn fetch_all_remotes(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: FetchAllRemotesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id =
        worker_handle(repos, &params.repo_path)?.fetch_all_remotes(params.prune, event_tx)?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushBranchParams {
    repo_path: String,
    remote_name: String,
    local_branch: String,
    remote_branch: String,
    set_upstream: bool,
    #[serde(default)]
    no_verify: bool,
}

fn push_branch(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: PushBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.push_branch(
        params.remote_name,
        params.local_branch,
        params.remote_branch,
        params.set_upstream,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteRemoteBranchParams {
    repo_path: String,
    remote_name: String,
    branch_name: String,
    #[serde(default)]
    no_verify: bool,
}

fn delete_remote_branch(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: DeleteRemoteBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.delete_remote_branch(
        params.remote_name,
        params.branch_name,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelTransferParams {
    repo_path: String,
    operation_id: String,
}

/// Asks the worker to abort the in-flight transfer `operationId`.
///
/// Takes no `stdout`: it spawns no progress relay of its own, because the cancellation surfaces
/// on the relay the original fetch/push/pull already started — as that operation's terminal
/// `Completed` notification, carrying `errorKind: "Cancelled"`. Unknown or already-finished IDs
/// are a no-op, so the extension can fire this without racing the transfer it is cancelling.
fn cancel_transfer(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: CancelTransferParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.cancel_transfer(&params.operation_id);
    Ok(Value::Null)
}

/// Params of a current-branch push; `noVerify` (skip `pre-push`) defaults to `false`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushParams {
    repo_path: String,
    remote_name: String,
    #[serde(default)]
    no_verify: bool,
}

fn push_current_branch(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: PushParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.push_current_branch(
        params.remote_name,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

fn force_push_current_branch(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: PushParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.force_push_current_branch(
        params.remote_name,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AbortSequenceParams {
    repo_path: String,
    #[serde(default)]
    confirmed: Option<ConfirmedAbortDto>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitIdParams {
    repo_path: String,
    commit_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResetParams {
    repo_path: String,
    commit_id: String,
    mode: ResetModeDto,
}

fn get_head_info(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let info = worker_handle(repos, &params.repo_path)?.get_head_info()?;
    serde_json::to_value(HeadInfoDto::from(info)).map_err(|error| error.to_string())
}

fn get_undo_state(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let result = worker_handle(repos, &params.repo_path)?.get_undo_state()?;
    serde_json::to_value(UndoStateDto::from(result)).map_err(|error| error.to_string())
}

fn undo_last(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let result = worker_handle(repos, &params.repo_path)?.undo()?;
    serde_json::to_value(UndoOutcomeDto::from(result)).map_err(|error| error.to_string())
}

fn redo_last(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let result = worker_handle(repos, &params.repo_path)?.redo()?;
    serde_json::to_value(UndoOutcomeDto::from(result)).map_err(|error| error.to_string())
}

fn amend_commit(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: CommitMessageParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(params.repo_path.clone(), event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.amend_commit(
        params.message,
        params.no_verify,
        params.author.map(|a| git_core::commit::SignatureInput {
            name: a.name,
            email: a.email,
        }),
        trailers(params.trailers),
        event_tx,
    )?;
    serde_json::to_value(CommitOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickParams {
    repo_path: String,
    commit_id: String,
    /// 1-based parent for a merge commit; absent or `null` otherwise.
    #[serde(default)]
    mainline: Option<u32>,
}

/// A clean pick's `prepare-commit-msg`/`post-commit` are relayed as `hookProgress` with
/// `source: "Pick"` (#183).
fn cherry_pick<W: std::io::Write + Send + 'static>(
    params: Value,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, String> {
    let params: PickParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let events = hook_relay(&params.repo_path, out);
    let outcome = worker_handle(repos, &params.repo_path)?.cherry_pick(
        params.commit_id,
        params.mainline,
        events,
    )?;
    serde_json::to_value(PickOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn revert_commit<W: std::io::Write + Send + 'static>(
    params: Value,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, String> {
    let params: PickParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let events = hook_relay(&params.repo_path, out);
    let outcome = worker_handle(repos, &params.repo_path)?.revert_commit(
        params.commit_id,
        params.mainline,
        events,
    )?;
    serde_json::to_value(PickOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitIdsParams {
    repo_path: String,
    /// Oldest-first.
    commit_ids: Vec<String>,
}

fn cherry_pick_many<W: std::io::Write + Send + 'static>(
    params: Value,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, String> {
    let params: CommitIdsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let events = hook_relay(&params.repo_path, out);
    let outcome =
        worker_handle(repos, &params.repo_path)?.cherry_pick_many(params.commit_ids, events)?;
    serde_json::to_value(SequenceOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn revert_many<W: std::io::Write + Send + 'static>(
    params: Value,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, String> {
    let params: CommitIdsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let events = hook_relay(&params.repo_path, out);
    let outcome =
        worker_handle(repos, &params.repo_path)?.revert_many(params.commit_ids, events)?;
    serde_json::to_value(SequenceOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContinueSequenceParams {
    repo_path: String,
    /// "Continue without hooks" (#133); absent means run them.
    #[serde(default)]
    no_verify: bool,
}

/// The step Continue commits runs the commit hooks, relayed as `hookProgress` (#133).
fn continue_sequence<W: std::io::Write + Send + 'static>(
    params: Value,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, String> {
    let params: ContinueSequenceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(params.repo_path.clone(), event_rx, Arc::clone(out));
    let outcome =
        worker_handle(repos, &params.repo_path)?.continue_sequence(params.no_verify, event_tx)?;
    serde_json::to_value(SequenceOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn abort_sequence(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: AbortSequenceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let confirmed = params.confirmed.map(git_core::pick::ConfirmedAbort::from);
    let abort = worker_handle(repos, &params.repo_path)?.abort_sequence(confirmed)?;
    serde_json::to_value(SequenceAbortDto::from(abort)).map_err(|error| error.to_string())
}

fn reset_to_commit(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ResetParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .reset_to_commit(params.commit_id, params.mode.into())?;
    Ok(Value::Null)
}

/// `post-checkout` is relayed as `hookProgress` with `source: "Checkout"` (#185).
fn checkout_commit<W: std::io::Write + Send + 'static>(
    params: Value,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, String> {
    let params: CommitIdParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let events = hook_relay(&params.repo_path, out);
    worker_handle(repos, &params.repo_path)?.checkout_commit(params.commit_id, events)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushTagsParams {
    repo_path: String,
    remote_name: String,
    names: Vec<String>,
    #[serde(default)]
    no_verify: bool,
}

fn push_tags(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: PushTagsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?.push_tags(
        params.remote_name,
        params.names,
        params.no_verify,
        event_tx,
    )?;
    Ok(Value::String(operation_id))
}

fn pull_current_upstream(
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.pull_current_upstream(event_tx)?;
    serde_json::to_value(PullOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn list_stashes(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let stashes: Vec<StashEntryDto> = worker_handle(repos, &params.repo_path)?
        .list_stashes()?
        .into_iter()
        .map(StashEntryDto::from)
        .collect();
    serde_json::to_value(stashes).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveStashParams {
    repo_path: String,
    options: StashOptionsDto,
}

fn save_stash(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: SaveStashParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.save_stash(params.options.into())?;
    Ok(Value::Null)
}

/// A stash by list index plus the commit id the client listed there; the backend refuses with
/// "The stash list changed" when `stash@{index}` is now another stash.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StashIndexParams {
    repo_path: String,
    index: usize,
    commit_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StashApplyParams {
    repo_path: String,
    index: usize,
    commit_id: String,
    restore_index: bool,
}

fn apply_stash(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: StashApplyParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.apply_stash(
        params.index,
        params.commit_id,
        params.restore_index,
    )?;
    serde_json::to_value(StashApplyOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn pop_stash(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: StashApplyParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.pop_stash(
        params.index,
        params.commit_id,
        params.restore_index,
    )?;
    serde_json::to_value(StashApplyOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StashFilesParams {
    repo_path: String,
    commit_id: String,
}

fn get_stash_files(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: StashFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files = worker_handle(repos, &params.repo_path)?.get_stash_files(params.commit_id)?;
    serde_json::to_value(StashFilesDto::from(files)).map_err(|error| error.to_string())
}

fn drop_stash(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: StashIndexParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.drop_stash(params.index, params.commit_id)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetBlameParams {
    repo_path: String,
    commit_id: String,
    path: String,
}

fn get_blame(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetBlameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let lines: Vec<BlameLineDto> = worker_handle(repos, &params.repo_path)?
        .get_blame(params.commit_id, params.path)?
        .into_iter()
        .map(BlameLineDto::from)
        .collect();
    serde_json::to_value(lines).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartMergeParams {
    repo_path: String,
    branch_name: String,
    options: MergeOptionsDto,
}

/// A fast-forward's or squash's `post-merge` is relayed as `hookProgress` with
/// `source: "Merge"` (#185).
fn start_merge<W: std::io::Write + Send + 'static>(
    params: Value,
    repos: &Repos,
    out: &Arc<Mutex<W>>,
) -> Result<Value, String> {
    let params: StartMergeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let events = hook_relay(&params.repo_path, out);
    let outcome = worker_handle(repos, &params.repo_path)?.start_merge(
        params.branch_name,
        params.options.into(),
        events,
    )?;
    serde_json::to_value(MergeOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

/// The remembered merge options for the repo, else the defaults from its `merge.ff`.
fn get_merge_options(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let options = match config::get_merge_preference(Path::new(&params.repo_path))
        .map_err(|error| error.to_string())?
    {
        Some(preference) => MergeOptionsDto::from(preference),
        None => MergeOptionsDto::from(worker_handle(repos, &params.repo_path)?.merge_defaults()?),
    };
    serde_json::to_value(options).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetMergeOptionsParams {
    repo_path: String,
    options: MergeOptionsDto,
}

fn set_merge_options(params: Value) -> Result<Value, String> {
    let params: SetMergeOptionsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::set_merge_preference(
        Path::new(&params.repo_path),
        params.options.fast_forward.into(),
        params.options.squash,
    )
    .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

fn get_conflict_hunks(params: Value, repos: &Repos) -> Result<Value, String> {
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

fn resolve_conflict(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ResolveConflictParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .resolve_conflict(params.path, params.resolved_content)?;
    Ok(Value::Null)
}

fn abort_merge(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.abort_merge()?;
    Ok(Value::Null)
}

fn get_merge_message(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let message = worker_handle(repos, &params.repo_path)?.get_merge_message()?;
    serde_json::to_value(message).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveAddDeleteConflictParams {
    repo_path: String,
    path: String,
    choice: FileConflictChoiceDto,
}

fn resolve_add_delete_conflict(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ResolveAddDeleteConflictParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .resolve_add_delete_conflict(params.path, params.choice.into())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitsSinceParams {
    repo_path: String,
    onto: String,
}

fn commits_since(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: CommitsSinceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let commits: Vec<RebasePlanCommitDto> = worker_handle(repos, &params.repo_path)?
        .commits_since(params.onto)?
        .into_iter()
        .map(RebasePlanCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartRebaseParams {
    repo_path: String,
    onto: String,
    plan: Vec<RebasePlanEntryDto>,
}

fn start_rebase(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: StartRebaseParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let result = worker_handle(repos, &params.repo_path)?.start_rebase(
        params.onto,
        params.plan.into_iter().map(Into::into).collect(),
    )?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(|error| error.to_string())
}

fn rebase_continue(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let result = worker_handle(repos, &params.repo_path)?.rebase_continue()?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(|error| error.to_string())
}

fn abort_rebase(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let abort = worker_handle(repos, &params.repo_path)?.abort_rebase()?;
    serde_json::to_value(RebaseAbortDto::from(abort)).map_err(|error| error.to_string())
}

fn detect_forge_repository(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let repositories: Vec<ForgeRepositoryDto> = worker_handle(repos, &params.repo_path)?
        .detect_forge_repository()?
        .into_iter()
        .map(ForgeRepositoryDto::from)
        .collect();
    serde_json::to_value(repositories).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForgeTokenParams {
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
    token: String,
}

fn save_forge_token(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ForgeTokenParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.save_forge_token(
        params.provider.into(),
        params.account,
        params.token,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForgetForgeTokenParams {
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
}

fn forget_forge_token(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ForgetForgeTokenParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .forget_forge_token(params.provider.into(), params.account)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListPullRequestsParams {
    repo_path: String,
    remote_name: String,
    account: String,
}

fn list_pull_requests(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ListPullRequestsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let list = worker_handle(repos, &params.repo_path)?
        .list_pull_requests(params.remote_name, params.account)?;
    serde_json::to_value(PullRequestListDto::from(list)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePullRequestParams {
    repo_path: String,
    remote_name: String,
    account: String,
    pull_request: CreatePullRequestDto,
}

fn create_pull_request(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: CreatePullRequestParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let created = worker_handle(repos, &params.repo_path)?.create_pull_request(
        params.remote_name,
        params.account,
        params.pull_request.into(),
    )?;
    serde_json::to_value(PullRequestDto::from(created)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod wire_snapshots;

#[cfg(test)]
mod tests {
    use super::*;
    use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};
    use repo_service::worker::HookSource;

    /// #306: the TypeScript shape of the sidecar's own wire DTOs (the shared ones are in
    /// `repo_service::wire`), checked in as `frontend/src/ipc/generated/sidecar-dto.ts` and
    /// checked against `RepoClient.ts` by `frontend/src/ipc/dtoContract.ts`. Regenerate with
    /// `UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar dto_bindings_are_current`.
    #[test]
    fn dto_bindings_are_current() {
        let bindings = test_support::ts_declarations!(
            "// Generated from the VSCode sidecar's own wire DTOs (crates/vscode-sidecar/src/dispatch.rs) by ts-rs.\n\
             // The shared ones are in wire-dto.ts.\n\
             // Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar dto_bindings_are_current\n";
            import "./wire-dto" { OpenRepoEntryDto };
            FileVersionDto,
            OpenReposDto,
        );
        test_support::assert_generated_file(
            &test_support::repo_root().join("frontend/src/ipc/generated/sidecar-dto.ts"),
            &bindings,
        );
    }

    /// #306. Regenerate with
    /// `UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar method_list_is_current`.
    #[test]
    fn method_list_is_current() {
        let mut unique = METHODS.to_vec();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), METHODS.len(), "a method is listed twice");
        test_support::assert_generated_file(
            &test_support::repo_root().join("frontend/src/ipc/generated/sidecar-methods.json"),
            &test_support::json_name_list(METHODS),
        );
    }

    /// A repository with a subfolder `sub`, opened as `opened(root)`; returns the temp dir.
    fn repo_opened_as(repos: &Repos, opened: impl Fn(&Path) -> String) -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        open_repo(serde_json::json!({ "path": opened(dir.path()) }), repos).unwrap();
        dir
    }

    /// #223: a request through another spelling of an open repository reaches its worker.
    #[test]
    fn a_repository_opened_with_a_trailing_slash_answers_another_spelling() {
        let repos: Repos = Arc::default();
        let dir = repo_opened_as(&repos, |path| {
            format!("{}{}", path.display(), std::path::MAIN_SEPARATOR)
        });
        let plain = dir.path().to_str().unwrap().to_string();
        open_repo(serde_json::json!({ "path": plain }), &repos).unwrap();

        let status = get_status(serde_json::json!({ "repoPath": plain }), &repos).unwrap();

        assert_eq!(status, serde_json::json!([]));
        assert_eq!(repos.len(), 1);
    }

    /// #223: closing one spelling keeps the repository open for the other.
    #[test]
    fn closing_one_spelling_keeps_the_other_open() {
        let repos: Repos = Arc::default();
        let dir = repo_opened_as(&repos, |path| path.to_str().unwrap().to_string());
        let plain = dir.path().to_str().unwrap().to_string();
        let sub = dir.path().join("sub").to_str().unwrap().to_string();
        open_repo(serde_json::json!({ "path": sub }), &repos).unwrap();

        close_repo(serde_json::json!({ "repoPath": sub }), &repos).unwrap();

        assert!(get_status(serde_json::json!({ "repoPath": plain }), &repos).is_ok());
        assert_eq!(
            get_status(serde_json::json!({ "repoPath": sub }), &repos).unwrap_err(),
            format!("repo not open: {sub}")
        );
        close_repo(serde_json::json!({ "repoPath": plain }), &repos).unwrap();
        assert!(repos.is_empty());
    }

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
                hook_output: None,
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
                "hookOutput": null,
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
                "hookOutput": null,
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
                "hookOutput": null,
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
    fn remote_failed_event_carries_the_remote_name_and_other_events_omit_it() {
        let failed = serde_json::to_value(transfer_progress_dto(TransferEvent::RemoteFailed {
            operation_id: "fetch-all-7".into(),
            remote_name: "backup".into(),
            error: TransferErrorKind::TransferFailed,
        }))
        .unwrap();
        assert_eq!(failed["phase"], "RemoteFailed");
        assert_eq!(failed["operation"], "FetchAll");
        assert_eq!(failed["remoteName"], "backup");
        assert_eq!(failed["errorKind"], "TransferFailed");

        let completed = serde_json::to_value(transfer_progress_dto(TransferEvent::Completed {
            operation_id: "fetch-all-7".into(),
            operation: TransferOperation::FetchAll,
            error: None,
            hook_output: None,
        }))
        .unwrap();
        assert!(!completed.as_object().unwrap().contains_key("remoteName"));
        assert_eq!(completed["hookOutput"], serde_json::Value::Null);

        let hook_failed = serde_json::to_value(transfer_progress_dto(TransferEvent::Completed {
            operation_id: "push-8".into(),
            operation: TransferOperation::PushBranch,
            error: Some(TransferErrorKind::HookFailed),
            hook_output: Some("tests failed\n".into()),
        }))
        .unwrap();
        assert_eq!(hook_failed["phase"], "Failed");
        assert_eq!(hook_failed["errorKind"], "HookFailed");
        assert_eq!(hook_failed["hookOutput"], "tests failed\n");
        assert_eq!(hook_failed["message"], serde_json::Value::Null);
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

    /// base: a.txt="base\n"; `feature` adds b.txt, changes a.txt, adds c.txt; the checked-out
    /// branch changed a.txt too, so a three-commit cherry-pick pauses on step 2.
    #[cfg(unix)]
    fn paused_pick_repo() -> (tempfile::TempDir, Vec<String>) {
        use git2::{IndexAddOption, Repository};
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
            config.set_bool("commit.gpgSign", false).unwrap();
            let hooks = repo.path().join("hooks");
            config
                .set_str("core.hooksPath", hooks.to_str().unwrap())
                .unwrap();
        }
        let commit_all = |message: &str| {
            let mut index = repo.index().unwrap();
            index
                .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
                .unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = repo.signature().unwrap();
            let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
                .unwrap()
                .to_string()
        };
        std::fs::write(dir.path().join("a.txt"), "base\n").unwrap();
        commit_all("base");
        let base_branch = repo.head().unwrap().name().unwrap().to_string();
        {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("feature", &head, false).unwrap();
        }
        repo.set_head("refs/heads/feature").unwrap();
        let mut ids = Vec::new();
        for (path, contents, message) in [
            ("b.txt", "b\n", "f one"),
            ("a.txt", "feature\n", "f two"),
            ("c.txt", "c\n", "f three"),
        ] {
            std::fs::write(dir.path().join(path), contents).unwrap();
            ids.push(commit_all(message));
        }
        repo.set_head(&base_branch).unwrap();
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new()
                .force()
                .remove_untracked(true),
        ))
        .unwrap();
        std::fs::write(dir.path().join("a.txt"), "main\n").unwrap();
        commit_all("main change");
        (dir, ids)
    }

    #[test]
    fn hook_progress_dto_serializes_every_source() {
        for (source, name) in [
            (HookSource::Commit, "Commit"),
            (HookSource::Checkout, "Checkout"),
            (HookSource::Merge, "Merge"),
            (HookSource::Pick, "Pick"),
        ] {
            let dto = hook_progress_dto(
                "/repo",
                HookEvent::Finished {
                    operation_id: "op-1".into(),
                    hook: "post-checkout".into(),
                    source,
                },
            );
            assert_eq!(
                serde_json::to_value(dto).unwrap(),
                serde_json::json!({"repoPath": "/repo", "operationId": "op-1", "hook": "post-checkout", "phase": "Finished", "source": name})
            );
        }
    }

    #[test]
    fn pick_outcome_dto_serializes_a_hook_failure() {
        assert_eq!(
            serde_json::to_value(PickOutcomeDto::from(
                git_core::pick::PickOutcome::HookFailed {
                    hook: "prepare-commit-msg".into(),
                    output: "no ticket\n".into()
                }
            ))
            .unwrap(),
            serde_json::json!({"kind": "HookFailed", "hook": "prepare-commit-msg", "output": "no ticket\n"})
        );
    }

    #[test]
    fn sequence_outcome_dto_serializes_a_stop_after_cancel() {
        let value = serde_json::to_value(SequenceOutcomeDto::from(
            git_core::pick::SequenceOutcome::Stopped {
                kept: "f one".into(),
                progress: git_core::pick::SequenceProgress {
                    kind: git_core::pick::SequenceKind::Revert,
                    current_step: 2,
                    total_steps: 3,
                    current_commit: "c2".into(),
                },
            },
        ))
        .unwrap();
        assert_eq!(value["kind"], "Stopped");
        assert_eq!(value["kept"], "f one");
        assert_eq!(value["progress"]["currentStep"], 2);
    }

    /// Spawns a worker for `dir`, installs `hook` (exit 0) and returns the repos map and a
    /// relay buffer.
    #[cfg(unix)]
    fn relay_fixture(dir: &std::path::Path, hook: &str) -> (Repos, Arc<Mutex<Vec<u8>>>) {
        use std::os::unix::fs::PermissionsExt;
        let repos: Repos = Arc::default();
        repos.open(dir.to_str().unwrap()).unwrap();
        let hooks = dir.join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let path = hooks.join(hook);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (repos, Arc::new(Mutex::new(Vec::new())))
    }

    /// The two `hookProgress` lines (Started, Finished) the relay wrote, waiting for them.
    #[cfg(unix)]
    fn two_relayed_lines(buffer: &Arc<Mutex<Vec<u8>>>) -> Vec<Value> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while lines_of(buffer).len() < 2 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let lines = lines_of(buffer);
        assert_eq!(lines.len(), 2, "{lines:?}");
        lines
    }

    /// #185: `post-checkout` after a switch reaches the webview with `source: "Checkout"`.
    #[cfg(unix)]
    #[test]
    fn switch_branch_relays_post_checkout_with_source_checkout() {
        let (dir, _ids) = paused_pick_repo();
        let path = dir.path().to_str().unwrap().to_string();
        let (repos, buffer) = relay_fixture(dir.path(), "post-checkout");

        switch_branch(
            serde_json::json!({ "repoPath": path, "name": "feature" }),
            &repos,
            &buffer,
        )
        .unwrap();

        let lines = two_relayed_lines(&buffer);
        assert!(lines.iter().all(|line| line["method"] == "hookProgress"
            && line["params"]["hook"] == "post-checkout"
            && line["params"]["source"] == "Checkout"));
        assert!(lines[0]["params"]["operationId"]
            .as_str()
            .unwrap()
            .starts_with("switch-"));
    }

    /// #185: `post-merge` after a fast-forward reaches the webview with `source: "Merge"`.
    #[cfg(unix)]
    #[test]
    fn start_merge_relays_post_merge_with_source_merge() {
        let (dir, _ids) = paused_pick_repo();
        {
            let repo = git2::Repository::open(dir.path()).unwrap();
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            let sig = repo.signature().unwrap();
            repo.commit(
                Some("refs/heads/ahead"),
                &sig,
                &sig,
                "ahead",
                &head.tree().unwrap(),
                &[&head],
            )
            .unwrap();
        }
        let path = dir.path().to_str().unwrap().to_string();
        let (repos, buffer) = relay_fixture(dir.path(), "post-merge");

        let outcome = start_merge(
            serde_json::json!({
                "repoPath": path,
                "branchName": "ahead",
                "options": { "fastForward": "Allow", "squash": false }
            }),
            &repos,
            &buffer,
        )
        .unwrap();

        assert_eq!(outcome["kind"], "FastForwarded");
        let lines = two_relayed_lines(&buffer);
        assert!(lines.iter().all(|line| line["method"] == "hookProgress"
            && line["params"]["hook"] == "post-merge"
            && line["params"]["source"] == "Merge"));
        assert!(lines[0]["params"]["operationId"]
            .as_str()
            .unwrap()
            .starts_with("merge-"));
    }

    /// #183/#185: a clean pick's hooks reach the webview as `hookProgress` with
    /// `source: "Pick"`.
    #[cfg(unix)]
    #[test]
    fn cherry_pick_many_relays_its_hooks_with_source_pick() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, ids) = paused_pick_repo();
        let path = dir.path().to_str().unwrap().to_string();
        let repos: Repos = Arc::default();
        repos.open(&path).unwrap();
        let hooks = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("post-commit");
        std::fs::write(&hook, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        let outcome = cherry_pick_many(
            serde_json::json!({ "repoPath": path, "commitIds": [ids[0]] }),
            &repos,
            &buffer,
        )
        .unwrap();

        assert_eq!(outcome["kind"], "Completed");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while lines_of(&buffer).len() < 2 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let lines = lines_of(&buffer);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines.iter().all(|line| line["method"] == "hookProgress"
            && line["params"]["hook"] == "post-commit"
            && line["params"]["source"] == "Pick"));
        assert!(lines[0]["params"]["operationId"]
            .as_str()
            .unwrap()
            .starts_with("pick-"));
    }

    #[cfg(unix)]
    #[test]
    fn continue_sequence_accepts_no_verify_and_relays_hook_progress() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, ids) = paused_pick_repo();
        let path = dir.path().to_str().unwrap().to_string();
        let repos: Repos = Arc::default();
        repos.open(&path).unwrap();
        let handle = worker_handle(&repos, &path).unwrap();
        handle.cherry_pick_many(ids, mpsc::channel().0).unwrap();
        handle
            .resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        let hooks = dir.path().join(".git").join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("commit-msg");
        std::fs::write(&hook, "#!/bin/sh\necho no change id; exit 1\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));

        let failed =
            continue_sequence(serde_json::json!({ "repoPath": path }), &repos, &buffer).unwrap();

        assert_eq!(failed["kind"], "HookFailed");
        assert_eq!(failed["hook"], "commit-msg");
        assert_eq!(failed["output"], "no change id\n");
        assert_eq!(failed["progress"]["currentStep"], 2);
        // The relay thread may still be writing the last line.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while lines_of(&buffer).len() < 2 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let lines = lines_of(&buffer);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert!(lines.iter().all(|line| line["method"] == "hookProgress"));
        assert_eq!(lines[0]["params"]["hook"], "commit-msg");
        assert_eq!(lines[0]["params"]["source"], "Commit");
        assert!(lines[0]["params"]["operationId"]
            .as_str()
            .unwrap()
            .starts_with("continue-"));

        let completed = continue_sequence(
            serde_json::json!({ "repoPath": path, "noVerify": true }),
            &repos,
            &buffer,
        )
        .unwrap();
        assert_eq!(completed["kind"], "Completed");
        assert_eq!(completed["applied"], 3);
    }
}
