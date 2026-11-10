use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use repo_service::wire::{
    hook_progress_dto, transfer_progress_dto, AppearanceSettingsDto, CommitMessageSettingsDto,
    DiffSettingsDto, ErrorDto, FetchSettingsDto, OpenRepoEntryDto, OpenRepoEntryInput,
    TransferProgressDto, WorkspaceDto,
};
use repo_service::worker::{HookEvent, TransferEvent, WorkerHandle, WorkerRegistry};

mod branch;
mod clone;
mod compare;
mod forge;
mod history;
mod merge;
mod patch;
mod rebase;
mod reflog;
mod remote;
mod search;
mod stash;
mod status;
mod submodule;
mod tag;
mod tools;
mod tree;
mod undo;
#[cfg(test)]
mod wire_snapshots;
mod worktree;

pub use branch::{create_branch, delete_branch, list_branches, rename_branch, switch_branch};
pub use clone::{cancel_clone, clone_repo, init_repo, CloneState};
pub use compare::{
    get_branch_compare_range, get_range_diff, get_range_files, get_working_tree_compare_diff,
    get_working_tree_compare_files,
};
pub use forge::{
    create_pull_request, detect_forge_repository, forget_forge_token, list_pull_requests,
    save_forge_token,
};
pub use history::{
    abort_sequence, amend_commit, checkout_commit, cherry_pick, cherry_pick_many,
    continue_sequence, get_head_info, list_recent_authors, reset_to_commit, revert_commit,
    revert_many,
};
pub use merge::{
    abort_merge, get_conflict_hunks, get_merge_message, get_merge_options, mark_resolved,
    resolve_add_delete_conflict, resolve_conflict, resolve_whole_file, set_merge_options,
    start_merge,
};
pub use patch::{apply_commit_change, apply_patch, get_patch, open_patch_file, save_patch_file};
pub use rebase::{abort_rebase, commits_since, rebase_continue, start_rebase};
pub use reflog::{get_reflog, list_reflog_refs, restore_reflog_entry};
pub use remote::*;
pub use search::{cancel_search, search_commits};
pub use stash::{apply_stash, drop_stash, get_stash_files, list_stashes, pop_stash, save_stash};
pub use status::{
    clear_graph_branch_selection, clear_graph_remote_branch_selection, commit,
    discard_all_unstaged, discard_file, discard_hunk, discard_lines, get_blame, get_commit_diff,
    get_commit_files, get_commit_graph, get_commit_message, get_commit_rename_diff,
    get_commit_signature, get_file_history, get_graph_branch_selection,
    get_graph_remote_branch_selection, get_line_history, get_status, get_working_diff, ignore_path,
    locate_commit, set_graph_branch_selection, set_graph_remote_branch_selection, stage_file,
    stage_hunk, stage_lines, unstage_file, unstage_hunk, unstage_lines,
};
pub use submodule::{init_submodule, list_submodules, update_submodule};
pub use tag::{create_tag, delete_tag, list_tags};
pub use tools::{
    get_external_tool_settings, get_file_sides, get_git_tools, open_diff_tool, open_in_editor,
    open_merge_tool, open_terminal, reveal_in_file_manager, set_external_tool_settings,
};
pub use tree::{get_commit_tree, get_file_at_commit, restore_file_version, save_file_version};
pub use undo::{get_undo_state, redo_last, undo_last};
pub use worktree::{create_worktree, list_worktrees, prune_worktrees, remove_worktree};

/// The Tauri event a transfer event is emitted as, and its payload.
fn transfer_event_payload(event: TransferEvent) -> (&'static str, TransferProgressDto) {
    let name = match event {
        TransferEvent::Started { .. }
        | TransferEvent::Progress(_)
        | TransferEvent::RemoteFailed { .. } => "transfer-progress",
        TransferEvent::Completed { .. } => "transfer-complete",
    };
    (name, transfer_progress_dto(event))
}

fn emit_transfer_events(app: AppHandle, events: mpsc::Receiver<TransferEvent>) {
    thread::spawn(move || {
        for event in events {
            let (name, payload) = transfer_event_payload(event);
            let result = app.emit(name, payload);
            let _ = result;
        }
    });
}

/// A hook event channel whose events are emitted as `hook-progress` for `repo_path`.
fn hook_event_channel(app: AppHandle, repo_path: &str) -> mpsc::Sender<HookEvent> {
    let (event_tx, event_rx) = mpsc::channel();
    emit_hook_events(app, repo_path.to_string(), event_rx);
    event_tx
}

/// The Tauri event `hook_progress_dto` payloads are emitted as.
const HOOK_PROGRESS_EVENT: &str = "hook-progress";

/// Like `emit_transfer_events`: drains on its own thread until the worker drops its sender.
fn emit_hook_events(app: AppHandle, repo_path: String, events: mpsc::Receiver<HookEvent>) {
    thread::spawn(move || {
        for event in events {
            let _ = app.emit(HOOK_PROGRESS_EVENT, hook_progress_dto(&repo_path, event));
        }
    });
}

/// Desktop only: the VSCode extension hands conflicts to VSCode's merge editor instead.
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "MergeToolOutcome"))]
pub enum MergeToolOutcomeDto {
    Resolved,
    Unresolved,
}

impl From<git_core::tools::MergeToolOutcome> for MergeToolOutcomeDto {
    fn from(outcome: git_core::tools::MergeToolOutcome) -> Self {
        match outcome {
            git_core::tools::MergeToolOutcome::Resolved => Self::Resolved,
            git_core::tools::MergeToolOutcome::Unresolved => Self::Unresolved,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    /// One worker per repository, whatever path spelling a command uses (#223).
    pub registry: WorkerRegistry,
    /// One gate per open repository, keyed by the registry's repository key (not the command's
    /// spelling), so two spellings of one repository share it (see `RepoWorker::call`, #181).
    gates: Mutex<HashMap<String, Arc<RepoGate>>>,
}

/// Lets one call per repository at a time take a blocking-pool thread (see `RepoWorker::call`).
type RepoGate = tauri::async_runtime::Mutex<()>;

#[tauri::command]
pub async fn open_repo(path: String, state: State<'_, AppState>) -> Result<(), ErrorDto> {
    remember_repo(&state, &path)?;
    // Best-effort: a repo that opened successfully should count as "recent" even if we can't
    // persist that fact (e.g. an unwritable config dir) — don't fail the whole open_repo call
    // over it.
    let _ = config::add_recent_repo(Path::new(&path));
    Ok(())
}

#[tauri::command]
pub async fn close_repo(repo_path: String, state: State<'_, AppState>) -> Result<(), ErrorDto> {
    forget_repo(&state, &repo_path);
    Ok(())
}

/// Forgets the spelling `repo_path`. When no other spelling of the repository is open, drops its
/// worker and gate. Calls already holding a clone of either finish as usual.
fn forget_repo(state: &AppState, repo_path: &str) {
    // Same lock order as `repo_worker` (registry, then gates), so a lookup racing a close can't
    // leave a gate behind for a repository that is no longer open.
    state
        .registry
        .close_with(repo_path, |key| drop_gate(state, key));
}

/// Opens `path`'s worker. `open_with` evicts a dead worker (its thread having exited, e.g. from
/// a panic inside a `Command` match arm) before deciding whether to reuse or respawn — a plain
/// `contains_key` fast path would keep handing back a dead entry forever. See
/// AUD-2026-09-05-CONC-001. A second spelling of an open repository shares its worker. A gate
/// whose repository this leaves without a worker (`path` now names another repository, or a
/// respawn failed) is dropped, under the registry's lock like `forget_repo`'s.
fn remember_repo(state: &AppState, path: &str) -> Result<(), String> {
    state
        .registry
        .open_with(path, |key| drop_gate(state, key))
        .map(|_| ())
}

fn drop_gate(state: &AppState, key: &str) {
    state
        .gates
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(key);
}

#[tauri::command]
pub fn list_open_repos() -> Result<(Vec<OpenRepoEntryDto>, Option<String>), ErrorDto> {
    let (entries, active) = config::list_open_repos().map_err(ErrorDto::from_error)?;
    Ok((
        entries.into_iter().map(OpenRepoEntryDto::from).collect(),
        active.map(|p| p.to_string_lossy().into_owned()),
    ))
}

#[tauri::command]
pub fn persist_open_repos(
    entries: Vec<OpenRepoEntryInput>,
    active_path: Option<String>,
) -> Result<(), ErrorDto> {
    let entries = entries
        .into_iter()
        .map(|entry| config::OpenRepoEntry {
            path: PathBuf::from(entry.path),
            workspace_id: entry.workspace_id,
        })
        .collect::<Vec<_>>();
    config::set_open_repos(&entries, active_path.as_deref().map(Path::new))
        .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub async fn pick_repo_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|path| path.to_string())
}

// `async fn`, unlike this file's other `config`-crate commands (`list_workspaces`,
// `save_workspace`, etc.), which stay plain `fn` because they're a single small TOML read/write.
// A plain `fn` `#[tauri::command]` runs *inline*, on whatever thread hands it the IPC message —
// there's no threadpool dispatch for the non-async case (verified against the vendored
// `tauri-macros` 2.6.3 `body_blocking` codegen: it's a direct call, not a `spawn`/
// `spawn_blocking`), and that thread is shared with the webview's own event loop (reproduced
// locally: injecting an artificial delay into a plain `fn` command's body measurably froze the
// whole webview's WebDriver responsiveness for the delay's full duration). `async fn` alone fixes
// that — Tauri dispatches it onto the async runtime's worker pool instead of inline.
//
// Deliberately NOT wrapped in `tauri::async_runtime::spawn_blocking`, unlike a first attempt at
// this fix: `config::scan_repos_in_root`'s actual work (`fs::read_dir` over a handful of
// directories, no recursion) is microseconds, and routing microsecond work through
// `spawn_blocking` trades one latency source for another — Tokio's blocking thread pool creates
// OS threads lazily and reclaims them after a short idle timeout, so a `spawn_blocking` call
// separated from the *previous* one by tens of seconds of unrelated UI interaction (exactly
// `e2e/specs/workspaces.spec.ts`'s Edit-modal flow, which restarts the whole app process via
// `browser.reloadSession()` and then does ~40-70s of other work before this command's first call
// in that process's lifetime) can itself cold-start an OS thread under real-world CI scheduling
// pressure — observed on real CI as this exact command stalling long enough to fail a 45s wait,
// on both the original attempt and a retry within the same process. Plain async execution on
// Tokio's own worker threads (created once at runtime startup, never torn down) has no such
// cold-start variable, which is the more direct fix for microsecond-scale work: block a thread
// pool only when the work is actually slow enough to need one.
#[tauri::command]
pub async fn scan_repos_in_root(root: String) -> Result<Vec<String>, ErrorDto> {
    config::scan_repos_in_root(Path::new(&root))
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn list_workspaces() -> Result<Vec<WorkspaceDto>, ErrorDto> {
    config::list_workspaces()
        .map(|workspaces| workspaces.into_iter().map(WorkspaceDto::from).collect())
        .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn save_workspace(
    name: String,
    root: String,
    members: Vec<String>,
) -> Result<String, ErrorDto> {
    config::save_workspace(
        &name,
        Path::new(&root),
        &members.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
    )
    .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn update_workspace(id: String, name: String, members: Vec<String>) -> Result<(), ErrorDto> {
    config::update_workspace(
        &id,
        &name,
        &members.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
    )
    .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn delete_workspace(id: String) -> Result<(), ErrorDto> {
    config::delete_workspace(&id).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn list_recent_repos() -> Result<Vec<String>, ErrorDto> {
    config::list_recent_repos()
        .map(|paths| {
            paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn get_diff_settings() -> Result<DiffSettingsDto, ErrorDto> {
    config::get_diff_settings()
        .map(DiffSettingsDto::from)
        .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn set_diff_settings(settings: DiffSettingsDto) -> Result<(), ErrorDto> {
    config::set_diff_settings(&settings.into()).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn get_commit_message_settings() -> Result<CommitMessageSettingsDto, ErrorDto> {
    config::get_commit_message_settings()
        .map(CommitMessageSettingsDto::from)
        .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn set_commit_message_settings(settings: CommitMessageSettingsDto) -> Result<(), ErrorDto> {
    config::set_commit_message_settings(&settings.into()).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn get_fetch_settings() -> Result<FetchSettingsDto, ErrorDto> {
    config::get_fetch_settings()
        .map(FetchSettingsDto::from)
        .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn set_fetch_settings(settings: FetchSettingsDto) -> Result<(), ErrorDto> {
    config::set_fetch_settings(&settings.into()).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn get_appearance_settings() -> Result<AppearanceSettingsDto, ErrorDto> {
    config::get_appearance_settings()
        .map(AppearanceSettingsDto::from)
        .map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn set_appearance_settings(settings: AppearanceSettingsDto) -> Result<(), ErrorDto> {
    config::set_appearance_settings(&settings.into()).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn get_last_seen_version() -> Result<Option<String>, ErrorDto> {
    config::get_last_seen_version().map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn set_last_seen_version(version: String) -> Result<(), ErrorDto> {
    config::set_last_seen_version(&version).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub fn get_app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// A repository's worker as commands reach it (#125). Every round trip runs on Tokio's blocking
/// pool (`call` → `off_runtime`), so a busy worker (a hook, a signer, a transfer, or a queue of
/// commands behind one) never holds one of the async runtime's few threads. With those held,
/// every other command stalled, for every repository, including the `cancel_transfer` meant to
/// stop the hook. `repo_worker` is the only way commands get at a worker, so a new command
/// can't block the runtime by accident.
///
/// `scan_repos_in_root` stays plain async on purpose: it involves no worker and is microseconds
/// of work (see its comment).
pub(crate) struct RepoWorker {
    handle: WorkerHandle,
    gate: Arc<RepoGate>,
}

impl RepoWorker {
    /// Runs `f` (one or more worker round trips) on the blocking pool and awaits its result.
    ///
    /// Waits for the repository's gate first, so a repository holds at most one blocking-pool
    /// thread (#181). Each queued call used to hold its own thread until the serial worker got
    /// to it: a long hook plus repeated refreshes could fill Tokio's pool (512 threads), and every
    /// other repository's commands then queued behind them. The worker runs one command at a
    /// time, so waiting at the gate instead of in its queue loses little; waiting calls are
    /// parked tasks, and Tokio's mutex is FIFO, so calls keep the order in which they reach the
    /// gate. Each `call` closure only makes round trips on its own handle and never awaits
    /// another call, so the gate can't deadlock. Anything that must not wait behind the worker
    /// goes through `signal()` instead.
    ///
    /// The guard moves into the blocking closure: if the caller's future is dropped while the
    /// round trip runs, the gate stays held until the round trip ends.
    pub(crate) async fn call<T, F>(self, f: F) -> Result<T, ErrorDto>
    where
        F: FnOnce(&WorkerHandle) -> Result<T, ErrorDto> + Send + 'static,
        T: Send + 'static,
    {
        let turn = Arc::clone(&self.gate).lock_owned().await;
        let handle = self.handle;
        off_runtime(move || {
            let _turn = turn;
            f(&handle)
        })
        .await?
    }

    /// Ungated access for what never waits on the worker's queue: `cancel_transfer`,
    /// `cancel_search`, claiming a search generation (`search::queue_search`) and the stored
    /// work tree path (`tools::workdir_of`). These write into or read state shared with the
    /// worker thread and return at once, so they must not queue behind the call holding the
    /// gate (a cancel exists to stop that call).
    pub(crate) fn signal(&self) -> &WorkerHandle {
        &self.handle
    }

    /// A worker with a gate of its own, so tests can queue calls that each take a blocking
    /// thread (as before #181) on one worker.
    #[cfg(all(test, unix))]
    fn with_own_gate(handle: WorkerHandle) -> Self {
        Self {
            handle,
            gate: Arc::default(),
        }
    }
}

/// Looks up `repo_path`'s worker and its repository's gate (creating the gate on first use).
/// Clones the worker's channel handle and drops the guards before any round trip: holding the
/// mutex across one would serialize every command behind it and let a wedged worker hold the
/// lock forever.
pub(crate) fn repo_worker(state: &AppState, repo_path: &str) -> Result<RepoWorker, String> {
    state
        .registry
        .get_with(repo_path, |key, handle| {
            let gate = state
                .gates
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(key.to_string())
                .or_default()
                .clone();
            RepoWorker { handle, gate }
        })
        .ok_or_else(|| format!("repo not open: {repo_path}"))
}

/// Runs blocking work on Tokio's blocking pool, off the async runtime's threads (#125).
pub(crate) async fn off_runtime<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "worker task stopped".to_string())
}

/// Opens `url` in the user's default external browser/handler, never inside this app's own
/// webview. Used for a pull request's provider URL (github.com/bitbucket.org) — the only place
/// this crate ever needed to open an external link — so a click can't navigate the whole app
/// window away with no way back short of restarting it. Does not touch `AppState`/the worker:
/// this is a plain OS-level operation, not a git one.
#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), ErrorDto> {
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(ErrorDto::from_error)
}

#[cfg(test)]
mod tests;

/// #306: the TypeScript shape of the desktop app's own wire DTOs (the shared ones are in
/// `repo_service::wire`), checked in as `frontend/src/ipc/generated/tauri-dto.ts`.
/// `frontend/src/ipc/dtoContract.ts` checks it against the hand-written types in `RepoClient.ts`.
/// Regenerate with `UPDATE_TRANSPORT_METHODS=1 cargo test -p tauri-app dto_bindings_are_current`.
#[cfg(test)]
mod ts_bindings {
    use super::clone::{CloneErrorDto, CloneOutcomeDto, CloneRequestDto, HttpsInputDto};
    use super::*;

    #[test]
    fn dto_bindings_are_current() {
        let bindings = test_support::ts_declarations!(
            "// Generated from the desktop app's own wire DTOs (crates/tauri-app/src/commands) by ts-rs.\n\
             // The shared ones are in wire-dto.ts.\n\
             // Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p tauri-app dto_bindings_are_current\n";
            HttpsInputDto,
            CloneRequestDto,
            CloneOutcomeDto,
            CloneErrorDto,
            MergeToolOutcomeDto,
        );
        test_support::assert_generated_file(
            &test_support::repo_root().join("frontend/src/ipc/generated/tauri-dto.ts"),
            &bindings,
        );
    }
}
