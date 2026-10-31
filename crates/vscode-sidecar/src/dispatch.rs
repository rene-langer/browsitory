use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use git_core::blame::BlameLine;
use git_core::branch::BranchInfo;
use git_core::diff::DiffHunk;
use git_core::forge::{ForgeProvider, ForgeRepository};
use git_core::graph::GraphCommit;
use git_core::merge::{ConflictSegment, FileConflictChoice, MergeOutcome};
use git_core::rebase::{RebaseAction, RebasePlanCommit, RebasePlanEntry, RebaseStepResult};
use git_core::reflog::ReflogEntry;
use git_core::remote::TagInfo;
use git_core::stash::StashEntry;
use git_core::status::StatusEntry;
use git_core::submodule::SubmoduleInfo;
use git_core::worktree::WorktreeInfo;
use repo_service::worker::{HookEvent, TransferEvent, Worker};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Shared, thread-safe handle to the sidecar's open repositories, keyed by path.
///
/// `main.rs` spawns one thread per incoming request (see its module doc comment) so a slow
/// operation against one repo can never block another request — including one against a
/// *different* repo — from being read and dispatched. That only holds if access to this map
/// itself is never held across a blocking call: every handler below either mutates the map
/// directly (`open_repo`/`close_repo`, both just a lock-mutate-unlock) or, far more commonly,
/// calls `worker_handle` to lock the map just long enough to clone out a `WorkerHandle` (a cheap
/// channel `Sender` clone) and then drops the lock *before* sending a command and blocking on its
/// reply — the same "clone the handle, drop the guard, then block" discipline
/// `docs/ARCHITECTURE.md` describes for `tauri-app`'s `AppState`.
pub type Repos = Arc<Mutex<HashMap<String, Worker>>>;

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

pub fn dispatch(
    method: &str,
    params: Value,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    test_delay_before_dispatch(method);
    match method {
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
        "cancel_transfer" => cancel_transfer(params, repos),
        "push_current_branch" => push_current_branch(params, repos, stdout),
        "force_push_current_branch" => force_push_current_branch(params, repos, stdout),
        "get_head_info" => get_head_info(params, repos),
        "amend_commit" => amend_commit(params, repos, stdout),
        "get_undo_state" => get_undo_state(params, repos),
        "undo_last" => undo_last(params, repos),
        "redo_last" => redo_last(params, repos),
        "cherry_pick" => cherry_pick(params, repos),
        "revert_commit" => revert_commit(params, repos),
        "cherry_pick_many" => cherry_pick_many(params, repos),
        "revert_many" => revert_many(params, repos),
        "continue_sequence" => continue_sequence(params, repos, stdout),
        "abort_sequence" => abort_sequence(params, repos),
        "reset_to_commit" => reset_to_commit(params, repos),
        "checkout_commit" => checkout_commit(params, repos),
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
        "start_merge" => start_merge(params, repos),
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
        other => Err(format!("unknown method: {other}")),
    }
}

/// Locks `repos` only long enough to clone out a `WorkerHandle` (a cheap channel `Sender`
/// clone), then drops the guard before returning. Callers always send their command and block
/// on its reply *after* this function has returned, i.e. with the map lock already released —
/// that's what lets one repo's slow operation run without blocking access to any other repo's
/// entry in this map.
fn worker_handle(
    repos: &Repos,
    repo_path: &str,
) -> Result<repo_service::worker::WorkerHandle, String> {
    let repos = repos.lock().unwrap_or_else(|error| error.into_inner());
    repos
        .get(repo_path)
        .map(Worker::handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))
}

#[derive(Deserialize)]
struct OpenRepoParams {
    path: String,
}

fn open_repo(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: OpenRepoParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    // `ensure_worker` keeps the same "don't hold the map lock across `Worker::spawn`" discipline
    // this function used to inline here itself, and additionally evicts a dead worker (its
    // thread having exited, e.g. from a panic inside a `Command` match arm) before deciding
    // whether to reuse or respawn — a plain `contains_key`/`Entry::Vacant` fast path would keep
    // handing back a dead entry forever. See AUD-2026-09-05-CONC-001.
    repo_service::worker::ensure_worker(repos, &params.path)?;
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
    repos
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&params.repo_path);
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusEntryDto {
    path: String,
    /// A staged rename's old path; always present (`null` otherwise).
    old_path: Option<String>,
    staged: bool,
    kind: String,
}

impl From<StatusEntry> for StatusEntryDto {
    fn from(entry: StatusEntry) -> Self {
        Self {
            path: entry.path,
            old_path: entry.old_path,
            staged: entry.staged,
            kind: format!("{:?}", entry.kind),
        }
    }
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
    remote_branch_refs: Vec<String>,
    tags: Vec<String>,
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
            remote_branch_refs: c.remote_branch_refs,
            tags: c.tags,
        }
    }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileHistoryEntryDto {
    commit: GraphCommitDto,
    path: String,
    // Serialized as `null` (not omitted) — the TS type is `string | null`.
    old_path: Option<String>,
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
        .map(|e| FileHistoryEntryDto {
            commit: GraphCommitDto::from(e.commit),
            path: e.path,
            old_path: e.old_path,
        })
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchPageDto {
    commits: Vec<GraphCommitDto>,
    // Serialized as `null` (not omitted) — the TS type is `string | null`.
    next_cursor: Option<String>,
    scanned: usize,
}

fn search_commits(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: SearchCommitsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let page = worker_handle(repos, &params.repo_path)?.search_commits(
        params.query,
        params.refs,
        params.cursor,
    )?;
    serde_json::to_value(SearchPageDto {
        commits: page.commits.into_iter().map(GraphCommitDto::from).collect(),
        next_cursor: page.next_cursor,
        scanned: page.scanned,
    })
    .map_err(|error| error.to_string())
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangedFileDto {
    path: String,
    old_path: Option<String>,
}

impl From<git_core::diff::ChangedFile> for ChangedFileDto {
    fn from(f: git_core::diff::ChangedFile) -> Self {
        ChangedFileDto {
            path: f.path,
            old_path: f.old_path,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CompareRangeDto {
    from_id: String,
    to_id: String,
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
    serde_json::to_value(CompareRangeDto {
        from_id: range.from_id,
        to_id: range.to_id,
    })
    .map_err(|error| error.to_string())
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

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct DiffOptionsDto {
    ignore_whitespace: bool,
    context_lines: u32,
}

impl From<DiffOptionsDto> for git_core::diff::DiffOptions {
    fn from(dto: DiffOptionsDto) -> Self {
        git_core::diff::DiffOptions {
            ignore_whitespace: dto.ignore_whitespace,
            context_lines: dto.context_lines,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiffSettingsDto {
    ignore_whitespace: bool,
    context_lines: u32,
    syntax_highlight: bool,
}

impl From<config::DiffSettings> for DiffSettingsDto {
    fn from(s: config::DiffSettings) -> Self {
        DiffSettingsDto {
            ignore_whitespace: s.ignore_whitespace,
            context_lines: s.context_lines,
            syntax_highlight: s.syntax_highlight,
        }
    }
}

impl From<DiffSettingsDto> for config::DiffSettings {
    fn from(d: DiffSettingsDto) -> Self {
        config::DiffSettings {
            ignore_whitespace: d.ignore_whitespace,
            context_lines: d.context_lines,
            syntax_highlight: d.syntax_highlight,
        }
    }
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

#[derive(Serialize, Deserialize)]
struct ExternalToolSettingsDto {
    editor: Option<String>,
    terminal: Option<String>,
}

impl From<config::ExternalToolSettings> for ExternalToolSettingsDto {
    fn from(settings: config::ExternalToolSettings) -> Self {
        Self {
            editor: settings.editor,
            terminal: settings.terminal,
        }
    }
}

impl From<ExternalToolSettingsDto> for config::ExternalToolSettings {
    fn from(dto: ExternalToolSettingsDto) -> Self {
        Self {
            editor: dto.editor,
            terminal: dto.terminal,
        }
    }
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

#[derive(Serialize)]
enum GitConfigSourceDto {
    System,
    Xdg,
    Global,
    Local,
    Worktree,
    Other,
}

impl From<git_core::tools::ConfigSource> for GitConfigSourceDto {
    fn from(source: git_core::tools::ConfigSource) -> Self {
        use git_core::tools::ConfigSource;
        match source {
            ConfigSource::System => Self::System,
            ConfigSource::Xdg => Self::Xdg,
            ConfigSource::Global => Self::Global,
            ConfigSource::Local => Self::Local,
            ConfigSource::Worktree => Self::Worktree,
            ConfigSource::Other => Self::Other,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitToolDto {
    name: String,
    command: Option<String>,
    source: GitConfigSourceDto,
    via_merge_tool: bool,
    problem: Option<String>,
}

impl From<git_core::tools::GitToolInfo> for GitToolDto {
    fn from(info: git_core::tools::GitToolInfo) -> Self {
        Self {
            name: info.name,
            command: info.command,
            source: info.source.into(),
            via_merge_tool: info.via_merge_tool,
            problem: info.problem,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitToolsDto {
    diff_tool: Option<GitToolDto>,
    merge_tool: Option<GitToolDto>,
}

fn get_git_tools(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let tools = worker_handle(repos, &params.repo_path)?.get_git_tools()?;
    serde_json::to_value(GitToolsDto {
        diff_tool: tools.diff_tool.map(Into::into),
        merge_tool: tools.merge_tool.map(Into::into),
    })
    .map_err(|error| error.to_string())
}

#[derive(Serialize)]
enum MarkResolvedOutcomeDto {
    Resolved,
    MarkersRemain,
}

impl From<git_core::merge::MarkResolvedOutcome> for MarkResolvedOutcomeDto {
    fn from(outcome: git_core::merge::MarkResolvedOutcome) -> Self {
        match outcome {
            git_core::merge::MarkResolvedOutcome::Resolved => Self::Resolved,
            git_core::merge::MarkResolvedOutcome::MarkersRemain => Self::MarkersRemain,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkResolvedParams {
    repo_path: String,
    path: String,
    force: bool,
}

#[derive(Deserialize)]
enum ConflictSideDto {
    Ours,
    Theirs,
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
    let side = match params.side {
        ConflictSideDto::Ours => git_core::merge::ConflictSide::Ours,
        ConflictSideDto::Theirs => git_core::merge::ConflictSide::Theirs,
    };
    worker_handle(repos, &params.repo_path)?.resolve_whole_file(params.path, side)?;
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

#[derive(Deserialize)]
struct TrailerDto {
    key: String,
    value: String,
}

fn trailers(dtos: Vec<TrailerDto>) -> Vec<git_core::commit::Trailer> {
    dtos.into_iter()
        .map(|dto| git_core::commit::Trailer {
            key: dto.key,
            value: dto.value,
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
struct SignatureInputDto {
    name: String,
    email: String,
}

fn list_recent_authors(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let authors: Vec<SignatureInputDto> = worker_handle(repos, &params.repo_path)?
        .list_recent_authors()?
        .into_iter()
        .map(|a| SignatureInputDto {
            name: a.name,
            email: a.email,
        })
        .collect();
    serde_json::to_value(authors).map_err(|error| error.to_string())
}

/// Which two sides of a diff (`RepoClient`'s `DiffSource`).
#[derive(Deserialize)]
#[serde(tag = "kind")]
enum DiffSourceDto {
    Unstaged,
    /// `oldPath` (optional, default `null`): a staged rename's HEAD-side path.
    #[serde(rename_all = "camelCase")]
    Staged {
        #[serde(default)]
        old_path: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Commit {
        commit_id: String,
        old_path: Option<String>,
    },
    /// `toId: null` is the working tree.
    #[serde(rename_all = "camelCase")]
    Range {
        from_id: String,
        to_id: Option<String>,
        old_path: Option<String>,
    },
}

impl From<DiffSourceDto> for git_core::tools::DiffSource {
    fn from(dto: DiffSourceDto) -> Self {
        match dto {
            DiffSourceDto::Unstaged => Self::Unstaged,
            DiffSourceDto::Staged { old_path } => Self::Staged { old_path },
            DiffSourceDto::Commit {
                commit_id,
                old_path,
            } => Self::Commit {
                commit_id,
                old_path,
            },
            DiffSourceDto::Range {
                from_id,
                to_id,
                old_path,
            } => Self::Range {
                from_id,
                to_id,
                old_path,
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileSidesParams {
    repo_path: String,
    path: String,
    source: DiffSourceDto,
}

/// One side for the image preview; `mime` is sniffed from the bytes (`null`: not an image).
#[derive(Serialize)]
#[serde(tag = "kind")]
enum FileSideDto {
    Missing,
    TooLarge {
        size: u64,
    },
    Loaded {
        size: u64,
        base64: String,
        mime: Option<&'static str>,
    },
}

impl From<git_core::diff::FileSide> for FileSideDto {
    fn from(side: git_core::diff::FileSide) -> Self {
        match side {
            git_core::diff::FileSide::Missing => Self::Missing,
            git_core::diff::FileSide::TooLarge { size } => Self::TooLarge { size },
            git_core::diff::FileSide::Loaded { bytes } => Self::Loaded {
                size: bytes.len() as u64,
                mime: git_core::diff::sniff_image_mime(&bytes),
                base64: repo_service::base64::encode(&bytes),
            },
        }
    }
}

fn get_file_sides(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: FileSidesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let sides = worker_handle(repos, &params.repo_path)?
        .get_file_sides(params.path, params.source.into())?;
    Ok(serde_json::json!({
        "old": FileSideDto::from(sides.old),
        "new": FileSideDto::from(sides.new),
    }))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitMessageSettingsDto {
    summary_limit: u32,
    body_ruler: u32,
    show_ruler: bool,
}

#[derive(Deserialize)]
struct SetCommitMessageSettingsParams {
    settings: CommitMessageSettingsDto,
}

fn get_commit_message_settings() -> Result<Value, String> {
    let s = config::get_commit_message_settings().map_err(|error| error.to_string())?;
    serde_json::to_value(CommitMessageSettingsDto {
        summary_limit: s.summary_limit,
        body_ruler: s.body_ruler,
        show_ruler: s.show_ruler,
    })
    .map_err(|error| error.to_string())
}

fn set_commit_message_settings(params: Value) -> Result<Value, String> {
    let params: SetCommitMessageSettingsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::set_commit_message_settings(&config::CommitMessageSettings {
        summary_limit: params.settings.summary_limit,
        body_ruler: params.settings.body_ruler,
        show_ruler: params.settings.show_ruler,
    })
    .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

/// A commit/amend result: a failing gate hook is data the UI renders, not an error.
#[derive(Serialize)]
#[serde(tag = "kind")]
enum CommitOutcomeDto {
    Committed {
        #[serde(rename = "commitId")]
        commit_id: String,
    },
    HookFailed {
        hook: String,
        output: String,
    },
}

impl From<git_core::commit::CommitOutcome> for CommitOutcomeDto {
    fn from(outcome: git_core::commit::CommitOutcome) -> Self {
        match outcome {
            git_core::commit::CommitOutcome::Committed { commit_id } => {
                Self::Committed { commit_id }
            }
            git_core::commit::CommitOutcome::HookFailed { hook, output } => {
                Self::HookFailed { hook, output }
            }
        }
    }
}

/// Payload of the `hookProgress` notification (commit/amend/Continue hooks).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HookProgressDto {
    repo_path: String,
    operation_id: String,
    hook: String,
    phase: &'static str,
}

fn hook_progress_dto(repo_path: &str, event: HookEvent) -> HookProgressDto {
    let (operation_id, hook, phase) = match event {
        HookEvent::Started { operation_id, hook } => (operation_id, hook, "Started"),
        HookEvent::Finished { operation_id, hook } => (operation_id, hook, "Finished"),
    };
    HookProgressDto {
        repo_path: repo_path.to_string(),
        operation_id,
        hook,
        phase,
    }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchInfoDto {
    name: String,
    is_current: bool,
    // Nulls are part of the wire contract: no `skip_serializing_if`.
    upstream: Option<String>,
    ahead: Option<usize>,
    behind: Option<usize>,
}

impl From<BranchInfo> for BranchInfoDto {
    fn from(branch: BranchInfo) -> Self {
        Self {
            name: branch.name,
            is_current: branch.is_current,
            upstream: branch.upstream,
            ahead: branch.ahead,
            behind: branch.behind,
        }
    }
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

fn switch_branch(params: Value, repos: &Repos) -> Result<Value, String> {
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
    /// Effective git-config prune default for this remote (the UI's Prune toggle fallback).
    prune: bool,
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
            prune: remote.prune,
        }
    }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamInfoDto {
    local_branch: String,
    remote_name: String,
    remote_branch: String,
    ahead: Option<usize>,
    behind: Option<usize>,
    /// Where Push lands under `push.default`; null = ask with the Push to… dialog.
    push_branch: Option<String>,
}

impl From<git_core::remote::UpstreamInfo> for UpstreamInfoDto {
    fn from(upstream: git_core::remote::UpstreamInfo) -> Self {
        Self {
            local_branch: upstream.local_branch,
            remote_name: upstream.remote_name,
            remote_branch: upstream.remote_branch,
            ahead: upstream.ahead,
            behind: upstream.behind,
            push_branch: upstream.push_branch,
        }
    }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TagInfoDto {
    name: String,
    target_id: String,
    commit_id: Option<String>,
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
            commit_id: tag.commit_id,
            annotated: tag.annotated,
            message: tag.message,
            tagger_name: tag.tagger_name,
            timestamp: tag.timestamp,
        }
    }
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
    /// Only on `RemoteFailed`: the remote that failed during a Fetch all.
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_name: Option<String>,
    /// Local `pre-push` hook output on a `HookFailed` completion, `null` otherwise. Unlike
    /// `message` this comes from the user's own script, not the remote.
    hook_output: Option<String>,
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
            remote_name: None,
            hook_output: None,
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
            remote_name: None,
            hook_output: None,
        },
        TransferEvent::RemoteFailed {
            operation_id,
            remote_name,
            error,
        } => TransferProgressDto {
            operation_id,
            // Only Fetch all reports per-remote failures.
            operation: format!("{:?}", git_core::remote::TransferOperation::FetchAll),
            phase: "RemoteFailed".to_string(),
            error_kind: Some(format!("{error:?}")),
            current: 0,
            total: 0,
            received_bytes: 0,
            message: None,
            remote_name: Some(remote_name),
            hook_output: None,
        },
        TransferEvent::Completed {
            operation_id,
            operation,
            error,
            hook_output,
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
                remote_name: None,
                hook_output,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HeadInfoDto {
    head_id: Option<String>,
    detached_at: Option<String>,
    head_pushed: bool,
    operation: Option<&'static str>,
    squash_message: Option<String>,
    commit_hooks: Vec<String>,
    sequence: Option<SequenceProgressDto>,
    rebase: Option<RebaseInfoDto>,
}

impl From<git_core::repo::HeadInfo> for HeadInfoDto {
    fn from(info: git_core::repo::HeadInfo) -> Self {
        use git_core::repo::OperationKind;
        Self {
            head_id: info.head_id,
            detached_at: info.detached_at,
            head_pushed: info.head_pushed,
            operation: info.operation.map(|kind| match kind {
                OperationKind::Merge => "Merge",
                OperationKind::CherryPick => "CherryPick",
                OperationKind::Revert => "Revert",
            }),
            squash_message: info.squash_message,
            commit_hooks: info.commit_hooks,
            sequence: info.sequence.map(SequenceProgressDto::from),
            rebase: info.rebase.map(RebaseInfoDto::from),
        }
    }
}

/// Same wire shape as the Tauri app's `RebaseInfoDto`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RebaseInfoDto {
    branch: String,
    onto: String,
    current_step: usize,
    total_steps: usize,
    health: RebaseHealthDto,
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "reason")]
enum RebaseHealthDto {
    Paused,
    Diverged(&'static str),
}

impl From<git_core::repo::RebaseInfo> for RebaseInfoDto {
    fn from(info: git_core::repo::RebaseInfo) -> Self {
        use git_core::rebase::{RebaseDivergence, RebaseHealth};
        Self {
            branch: info.branch,
            onto: info.onto,
            current_step: info.current_step,
            total_steps: info.total_steps,
            health: match info.health {
                RebaseHealth::Paused => RebaseHealthDto::Paused,
                RebaseHealth::Diverged(reason) => RebaseHealthDto::Diverged(match reason {
                    RebaseDivergence::HeadMoved => "HeadMoved",
                    RebaseDivergence::BranchMoved => "BranchMoved",
                    RebaseDivergence::BranchMissing => "BranchMissing",
                    RebaseDivergence::OtherOperation => "OtherOperation",
                }),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SequenceProgressDto {
    kind: &'static str,
    current_step: usize,
    total_steps: usize,
    current_commit: String,
}

impl From<git_core::pick::SequenceProgress> for SequenceProgressDto {
    fn from(progress: git_core::pick::SequenceProgress) -> Self {
        Self {
            kind: match progress.kind {
                git_core::pick::SequenceKind::CherryPick => "CherryPick",
                git_core::pick::SequenceKind::Revert => "Revert",
            },
            current_step: progress.current_step,
            total_steps: progress.total_steps,
            current_commit: progress.current_commit,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum SequenceOutcomeDto {
    Completed {
        applied: usize,
        skipped: Vec<String>,
    },
    Conflicted {
        files: Vec<String>,
        progress: SequenceProgressDto,
    },
    /// Continue's commit hook failed; the step is still paused (#133).
    HookFailed {
        hook: String,
        output: String,
        progress: SequenceProgressDto,
    },
}

impl From<git_core::pick::SequenceOutcome> for SequenceOutcomeDto {
    fn from(outcome: git_core::pick::SequenceOutcome) -> Self {
        match outcome {
            git_core::pick::SequenceOutcome::Completed { applied, skipped } => {
                Self::Completed { applied, skipped }
            }
            git_core::pick::SequenceOutcome::Conflicted { files, progress } => Self::Conflicted {
                files,
                progress: progress.into(),
            },
            git_core::pick::SequenceOutcome::HookFailed {
                hook,
                output,
                progress,
            } => Self::HookFailed {
                hook,
                output,
                progress: progress.into(),
            },
        }
    }
}

#[derive(Serialize)]
struct SequenceAbortDto {
    rewound: bool,
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum PickOutcomeDto {
    Committed {
        #[serde(rename = "commitId")]
        commit_id: String,
    },
    Conflicted {
        files: Vec<String>,
    },
}

impl From<git_core::pick::PickOutcome> for PickOutcomeDto {
    fn from(outcome: git_core::pick::PickOutcome) -> Self {
        match outcome {
            git_core::pick::PickOutcome::Committed { commit_id } => Self::Committed { commit_id },
            git_core::pick::PickOutcome::Conflicted { files } => Self::Conflicted { files },
        }
    }
}

#[derive(Serialize)]
struct UndoStateDto {
    undo: Option<String>,
    redo: Option<String>,
}

impl From<repo_service::worker::UndoState> for UndoStateDto {
    fn from(state: repo_service::worker::UndoState) -> Self {
        Self {
            undo: state.undo,
            redo: state.redo,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum SignatureStatusDto {
    Unsigned,
    Good,
    GoodUntrusted,
    Bad,
    UnknownKey,
    Expired,
    ExpiredKey,
    RevokedKey,
    Error,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum SignatureFormatDto {
    Openpgp,
    Ssh,
}

#[derive(Serialize)]
struct CommitSignatureDto {
    status: SignatureStatusDto,
    format: Option<SignatureFormatDto>,
    signer: Option<String>,
    key: Option<String>,
    detail: Option<String>,
}

impl From<git_core::signing::CommitSignature> for CommitSignatureDto {
    fn from(signature: git_core::signing::CommitSignature) -> Self {
        use git_core::signing::{SignatureFormat, SignatureStatus};
        Self {
            status: match signature.status {
                SignatureStatus::Unsigned => SignatureStatusDto::Unsigned,
                SignatureStatus::Good => SignatureStatusDto::Good,
                SignatureStatus::GoodUntrusted => SignatureStatusDto::GoodUntrusted,
                SignatureStatus::Bad => SignatureStatusDto::Bad,
                SignatureStatus::UnknownKey => SignatureStatusDto::UnknownKey,
                SignatureStatus::Expired => SignatureStatusDto::Expired,
                SignatureStatus::ExpiredKey => SignatureStatusDto::ExpiredKey,
                SignatureStatus::RevokedKey => SignatureStatusDto::RevokedKey,
                SignatureStatus::Error => SignatureStatusDto::Error,
            },
            format: signature.format.map(|format| match format {
                SignatureFormat::OpenPgp => SignatureFormatDto::Openpgp,
                SignatureFormat::Ssh => SignatureFormatDto::Ssh,
            }),
            signer: signature.signer,
            key: signature.key,
            detail: signature.detail,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum UndoOutcomeDto {
    Done { label: String },
    Nothing,
    RepositoryChanged { label: String },
    OperationInProgress,
}

impl From<repo_service::worker::UndoOutcome> for UndoOutcomeDto {
    fn from(outcome: repo_service::worker::UndoOutcome) -> Self {
        use repo_service::worker::UndoOutcome;
        match outcome {
            UndoOutcome::Done { label } => Self::Done { label },
            UndoOutcome::Nothing => Self::Nothing,
            UndoOutcome::RepositoryChanged { label } => Self::RepositoryChanged { label },
            UndoOutcome::OperationInProgress => Self::OperationInProgress,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum ResetModeDto {
    Soft,
    Mixed,
    Hard,
}

impl From<ResetModeDto> for git_core::reset::ResetMode {
    fn from(dto: ResetModeDto) -> Self {
        match dto {
            ResetModeDto::Soft => Self::Soft,
            ResetModeDto::Mixed => Self::Mixed,
            ResetModeDto::Hard => Self::Hard,
        }
    }
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

fn cherry_pick(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: PickParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome =
        worker_handle(repos, &params.repo_path)?.cherry_pick(params.commit_id, params.mainline)?;
    serde_json::to_value(PickOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn revert_commit(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: PickParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?
        .revert_commit(params.commit_id, params.mainline)?;
    serde_json::to_value(PickOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitIdsParams {
    repo_path: String,
    /// Oldest-first.
    commit_ids: Vec<String>,
}

fn cherry_pick_many(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: CommitIdsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.cherry_pick_many(params.commit_ids)?;
    serde_json::to_value(SequenceOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn revert_many(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: CommitIdsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.revert_many(params.commit_ids)?;
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
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let abort = worker_handle(repos, &params.repo_path)?.abort_sequence()?;
    serde_json::to_value(SequenceAbortDto {
        rewound: abort.rewound,
    })
    .map_err(|error| error.to_string())
}

fn reset_to_commit(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ResetParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .reset_to_commit(params.commit_id, params.mode.into())?;
    Ok(Value::Null)
}

fn checkout_commit(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: CommitIdParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.checkout_commit(params.commit_id)?;
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
struct StashOptionsDto {
    message: Option<String>,
    keep_index: bool,
    include_untracked: bool,
    paths: Vec<String>,
}

impl From<StashOptionsDto> for git_core::stash::StashOptions {
    fn from(dto: StashOptionsDto) -> Self {
        Self {
            message: dto.message,
            keep_index: dto.keep_index,
            include_untracked: dto.include_untracked,
            paths: dto.paths,
        }
    }
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

#[derive(Serialize)]
#[serde(tag = "kind")]
enum StashApplyOutcomeDto {
    Applied,
    Conflicted { files: Vec<String> },
}

impl From<git_core::stash::StashApplyOutcome> for StashApplyOutcomeDto {
    fn from(outcome: git_core::stash::StashApplyOutcome) -> Self {
        match outcome {
            git_core::stash::StashApplyOutcome::Applied => Self::Applied,
            git_core::stash::StashApplyOutcome::Conflicted { files } => Self::Conflicted { files },
        }
    }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StashFilesDto {
    tracked: Vec<String>,
    untracked: Vec<String>,
    untracked_commit_id: Option<String>,
}

fn get_stash_files(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: StashFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files = worker_handle(repos, &params.repo_path)?.get_stash_files(params.commit_id)?;
    serde_json::to_value(StashFilesDto {
        tracked: files.tracked,
        untracked: files.untracked,
        untracked_commit_id: files.untracked_commit_id,
    })
    .map_err(|error| error.to_string())
}

fn drop_stash(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: StashIndexParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.drop_stash(params.index, params.commit_id)?;
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

#[derive(Serialize)]
#[serde(tag = "kind")]
enum MergeOutcomeDto {
    UpToDate,
    FastForwarded,
    Merged,
    Squashed,
    Conflicted { files: Vec<String> },
}

impl From<MergeOutcome> for MergeOutcomeDto {
    fn from(outcome: MergeOutcome) -> Self {
        match outcome {
            MergeOutcome::UpToDate => MergeOutcomeDto::UpToDate,
            MergeOutcome::FastForwarded => MergeOutcomeDto::FastForwarded,
            MergeOutcome::Merged => MergeOutcomeDto::Merged,
            MergeOutcome::Squashed => MergeOutcomeDto::Squashed,
            MergeOutcome::Conflicted { files } => MergeOutcomeDto::Conflicted { files },
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum FastForwardDto {
    Allow,
    Never,
    Only,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MergeOptionsDto {
    fast_forward: FastForwardDto,
    squash: bool,
}

impl From<MergeOptionsDto> for git_core::merge::MergeOptions {
    fn from(dto: MergeOptionsDto) -> Self {
        use git_core::merge::FastForward;
        Self {
            fast_forward: match dto.fast_forward {
                FastForwardDto::Allow => FastForward::Allow,
                FastForwardDto::Never => FastForward::Never,
                FastForwardDto::Only => FastForward::Only,
            },
            squash: dto.squash,
        }
    }
}

impl From<git_core::merge::MergeOptions> for MergeOptionsDto {
    fn from(options: git_core::merge::MergeOptions) -> Self {
        use git_core::merge::FastForward;
        Self {
            fast_forward: match options.fast_forward {
                FastForward::Allow => FastForwardDto::Allow,
                FastForward::Never => FastForwardDto::Never,
                FastForward::Only => FastForwardDto::Only,
            },
            squash: options.squash,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartMergeParams {
    repo_path: String,
    branch_name: String,
    options: MergeOptionsDto,
}

fn start_merge(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: StartMergeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?
        .start_merge(params.branch_name, params.options.into())?;
    serde_json::to_value(MergeOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

/// The remembered merge options for the repo, else the defaults from its `merge.ff`.
fn get_merge_options(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let options = match config::get_merge_preference(Path::new(&params.repo_path))
        .map_err(|error| error.to_string())?
    {
        Some(preference) => MergeOptionsDto {
            fast_forward: match preference.fast_forward {
                config::MergeFastForward::Allow => FastForwardDto::Allow,
                config::MergeFastForward::Never => FastForwardDto::Never,
                config::MergeFastForward::Only => FastForwardDto::Only,
            },
            squash: preference.squash,
        },
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
    let fast_forward = match params.options.fast_forward {
        FastForwardDto::Allow => config::MergeFastForward::Allow,
        FastForwardDto::Never => config::MergeFastForward::Never,
        FastForwardDto::Only => config::MergeFastForward::Only,
    };
    config::set_merge_preference(
        Path::new(&params.repo_path),
        fast_forward,
        params.options.squash,
    )
    .map_err(|error| error.to_string())?;
    Ok(Value::Null)
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

fn resolve_add_delete_conflict(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: ResolveAddDeleteConflictParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .resolve_add_delete_conflict(params.path, params.choice.into())?;
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RebasePlanCommitDto {
    id: String,
    short_id: String,
    summary: String,
    author_name: String,
    timestamp: i64,
}

impl From<RebasePlanCommit> for RebasePlanCommitDto {
    fn from(c: RebasePlanCommit) -> Self {
        Self {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            timestamp: c.timestamp,
        }
    }
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
#[serde(tag = "kind")]
enum RebaseActionDto {
    Pick,
    Reword { message: String },
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl From<RebaseActionDto> for RebaseAction {
    fn from(dto: RebaseActionDto) -> Self {
        match dto {
            RebaseActionDto::Pick => RebaseAction::Pick,
            RebaseActionDto::Reword { message } => RebaseAction::Reword { message },
            RebaseActionDto::Edit => RebaseAction::Edit,
            RebaseActionDto::Squash => RebaseAction::Squash,
            RebaseActionDto::Fixup => RebaseAction::Fixup,
            RebaseActionDto::Drop => RebaseAction::Drop,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RebasePlanEntryDto {
    commit_id: String,
    action: RebaseActionDto,
    combined_message: Option<String>,
}

impl From<RebasePlanEntryDto> for RebasePlanEntry {
    fn from(dto: RebasePlanEntryDto) -> Self {
        RebasePlanEntry {
            commit_id: dto.commit_id,
            action: dto.action.into(),
            combined_message: dto.combined_message,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum RebaseStepResultDto {
    Conflicted { files: Vec<String> },
    PausedForEdit,
    Advanced,
    Done,
}

impl From<RebaseStepResult> for RebaseStepResultDto {
    fn from(result: RebaseStepResult) -> Self {
        match result {
            RebaseStepResult::Conflicted { files } => RebaseStepResultDto::Conflicted { files },
            RebaseStepResult::PausedForEdit => RebaseStepResultDto::PausedForEdit,
            RebaseStepResult::Advanced => RebaseStepResultDto::Advanced,
            RebaseStepResult::Done => RebaseStepResultDto::Done,
        }
    }
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
    Ok(serde_json::json!({ "restored": abort.restored }))
}

#[derive(Clone, Copy, Serialize, Deserialize)]
enum ForgeProviderDto {
    GitHub,
    Bitbucket,
}

impl From<ForgeProvider> for ForgeProviderDto {
    fn from(provider: ForgeProvider) -> Self {
        match provider {
            ForgeProvider::GitHub => ForgeProviderDto::GitHub,
            ForgeProvider::Bitbucket => ForgeProviderDto::Bitbucket,
        }
    }
}

impl From<ForgeProviderDto> for ForgeProvider {
    fn from(dto: ForgeProviderDto) -> Self {
        match dto {
            ForgeProviderDto::GitHub => ForgeProvider::GitHub,
            ForgeProviderDto::Bitbucket => ForgeProvider::Bitbucket,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgeRepositoryDto {
    provider: ForgeProviderDto,
    host: String,
    owner: String,
    name: String,
    remote_name: String,
}

impl From<ForgeRepository> for ForgeRepositoryDto {
    fn from(repository: ForgeRepository) -> Self {
        Self {
            provider: repository.provider.into(),
            host: repository.host,
            owner: repository.owner,
            name: repository.name,
            remote_name: repository.remote_name,
        }
    }
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestDto {
    id: String,
    number: u64,
    title: String,
    url: String,
    author: String,
    source_branch: String,
    target_branch: String,
    state: String,
}

impl From<repo_service::pull_requests::PullRequest> for PullRequestDto {
    fn from(pull_request: repo_service::pull_requests::PullRequest) -> Self {
        Self {
            id: pull_request.id,
            number: pull_request.number,
            title: pull_request.title,
            url: pull_request.url,
            author: pull_request.author,
            source_branch: pull_request.source_branch,
            target_branch: pull_request.target_branch,
            state: pull_request.state,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestListDto {
    pull_requests: Vec<PullRequestDto>,
    truncated: bool,
}

impl From<repo_service::pull_requests::PullRequestList> for PullRequestListDto {
    fn from(list: repo_service::pull_requests::PullRequestList) -> Self {
        Self {
            pull_requests: list
                .pull_requests
                .into_iter()
                .map(PullRequestDto::from)
                .collect(),
            truncated: list.truncated,
        }
    }
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
struct CreatePullRequestInputDto {
    title: String,
    description: Option<String>,
    source_branch: String,
    target_branch: String,
}

impl From<CreatePullRequestInputDto> for repo_service::pull_requests::CreatePullRequest {
    fn from(dto: CreatePullRequestInputDto) -> Self {
        Self {
            title: dto.title,
            description: dto.description,
            source_branch: dto.source_branch,
            target_branch: dto.target_branch,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePullRequestParams {
    repo_path: String,
    remote_name: String,
    account: String,
    pull_request: CreatePullRequestInputDto,
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

    #[cfg(unix)]
    #[test]
    fn continue_sequence_accepts_no_verify_and_relays_hook_progress() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, ids) = paused_pick_repo();
        let path = dir.path().to_str().unwrap().to_string();
        let repos: Repos = Arc::new(Mutex::new(HashMap::new()));
        repos.lock().unwrap().insert(
            path.clone(),
            Worker::spawn(dir.path().to_path_buf()).unwrap(),
        );
        let handle = worker_handle(&repos, &path).unwrap();
        handle.cherry_pick_many(ids).unwrap();
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
