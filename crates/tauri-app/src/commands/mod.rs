use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use repo_service::wire::*;
use repo_service::worker::{HookEvent, TransferEvent, WorkerHandle, WorkerRegistry};

mod branch;
mod clone;
mod compare;
mod forge;
mod history;
mod merge;
mod rebase;
mod reflog;
mod remote;
mod search;
mod stash;
mod status;
mod submodule;
mod tag;
mod tools;
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
pub use rebase::{abort_rebase, commits_since, rebase_continue, start_rebase};
pub use reflog::{get_reflog, list_reflog_refs, restore_reflog_entry};
pub use remote::*;
pub use search::{cancel_search, search_commits};
pub use stash::{apply_stash, drop_stash, get_stash_files, list_stashes, pop_stash, save_stash};
pub use status::{
    commit, discard_all_unstaged, discard_file, discard_hunk, discard_lines, get_blame,
    get_commit_diff, get_commit_files, get_commit_graph, get_commit_message,
    get_commit_rename_diff, get_commit_signature, get_file_history, get_graph_branch_selection,
    get_graph_remote_branch_selection, get_status, get_working_diff, ignore_path,
    set_graph_branch_selection, set_graph_remote_branch_selection, stage_file, stage_hunk,
    stage_lines, unstage_file, unstage_hunk, unstage_lines,
};
pub use submodule::{init_submodule, list_submodules, update_submodule};
pub use tag::{create_tag, delete_tag, list_tags};
pub use tools::{
    get_external_tool_settings, get_file_sides, get_git_tools, open_diff_tool, open_in_editor,
    open_merge_tool, open_terminal, reveal_in_file_manager, set_external_tool_settings,
};
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

pub(crate) fn trailers_from(dtos: Option<Vec<TrailerDto>>) -> Vec<git_core::commit::Trailer> {
    dtos.unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect()
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
pub async fn open_repo(path: String, state: State<'_, AppState>) -> Result<(), String> {
    remember_repo(&state, &path)?;
    // Best-effort: a repo that opened successfully should count as "recent" even if we can't
    // persist that fact (e.g. an unwritable config dir) — don't fail the whole open_repo call
    // over it.
    let _ = config::add_recent_repo(Path::new(&path));
    Ok(())
}

#[tauri::command]
pub async fn close_repo(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
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
pub fn list_open_repos() -> Result<(Vec<OpenRepoEntryDto>, Option<String>), String> {
    let (entries, active) = config::list_open_repos().map_err(|e| e.to_string())?;
    Ok((
        entries.into_iter().map(OpenRepoEntryDto::from).collect(),
        active.map(|p| p.to_string_lossy().into_owned()),
    ))
}

#[tauri::command]
pub fn persist_open_repos(
    entries: Vec<OpenRepoEntryInput>,
    active_path: Option<String>,
) -> Result<(), String> {
    let entries = entries
        .into_iter()
        .map(|entry| config::OpenRepoEntry {
            path: PathBuf::from(entry.path),
            workspace_id: entry.workspace_id,
        })
        .collect::<Vec<_>>();
    config::set_open_repos(&entries, active_path.as_deref().map(Path::new))
        .map_err(|e| e.to_string())
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
pub async fn scan_repos_in_root(root: String) -> Result<Vec<String>, String> {
    config::scan_repos_in_root(Path::new(&root))
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_workspaces() -> Result<Vec<WorkspaceDto>, String> {
    config::list_workspaces()
        .map(|workspaces| workspaces.into_iter().map(WorkspaceDto::from).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_workspace(name: String, root: String, members: Vec<String>) -> Result<String, String> {
    config::save_workspace(
        &name,
        Path::new(&root),
        &members.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_workspace(id: String, name: String, members: Vec<String>) -> Result<(), String> {
    config::update_workspace(
        &id,
        &name,
        &members.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_workspace(id: String) -> Result<(), String> {
    config::delete_workspace(&id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_recent_repos() -> Result<Vec<String>, String> {
    config::list_recent_repos()
        .map(|paths| {
            paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_diff_settings() -> Result<DiffSettingsDto, String> {
    config::get_diff_settings()
        .map(DiffSettingsDto::from)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_diff_settings(settings: DiffSettingsDto) -> Result<(), String> {
    config::set_diff_settings(&settings.into()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_commit_message_settings() -> Result<CommitMessageSettingsDto, String> {
    config::get_commit_message_settings()
        .map(CommitMessageSettingsDto::from)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_commit_message_settings(settings: CommitMessageSettingsDto) -> Result<(), String> {
    config::set_commit_message_settings(&settings.into()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_last_seen_version() -> Result<Option<String>, String> {
    config::get_last_seen_version().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_last_seen_version(version: String) -> Result<(), String> {
    config::set_last_seen_version(&version).map_err(|e| e.to_string())
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
    pub(crate) async fn call<T, F>(self, f: F) -> Result<T, String>
    where
        F: FnOnce(&WorkerHandle) -> Result<T, String> + Send + 'static,
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
pub async fn open_external_url(url: String) -> Result<(), String> {
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::path::Path;
    use std::sync::mpsc;

    /// More runtime threads than the async runtime has.
    fn saturating_count() -> usize {
        std::thread::available_parallelism().map_or(4, |n| n.get()) + 2
    }

    /// Spawns a trivial task and waits (from a plain thread) up to 5 s for it to run.
    fn runtime_responds() -> bool {
        let probe = tauri::async_runtime::spawn(async { 7 });
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = done_tx.send(tauri::async_runtime::block_on(probe).ok());
        });
        done_rx.recv_timeout(std::time::Duration::from_secs(5)) == Ok(Some(7))
    }

    #[test]
    fn off_runtime_keeps_async_runtime_threads_free() {
        let mut releases = Vec::new();
        let mut blockers = Vec::new();
        for _ in 0..saturating_count() {
            let (release_tx, release_rx) = mpsc::channel::<()>();
            releases.push(release_tx);
            blockers.push(tauri::async_runtime::spawn(super::off_runtime(move || {
                release_rx.recv().is_ok()
            })));
        }

        let responded = runtime_responds();

        for release in releases {
            let _ = release.send(());
        }
        for blocker in blockers {
            assert_eq!(tauri::async_runtime::block_on(blocker).unwrap(), Ok(true));
        }
        assert!(
            responded,
            "a blocked off_runtime job held an async runtime thread"
        );
    }

    /// A repository with one staged file and a `pre-commit` hook that waits until the returned
    /// release file exists, so a commit keeps its worker busy for as long as the test wants.
    #[cfg(unix)]
    fn repo_with_waiting_pre_commit() -> (tempfile::TempDir, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
            config.set_bool("commit.gpgSign", false).unwrap();
            config
                .set_str(
                    "core.hooksPath",
                    repo.path().join("hooks").to_str().unwrap(),
                )
                .unwrap();
        }
        // A first commit ("base"), made by git2 so no hook runs, gives a search something to find.
        std::fs::write(dir.path().join("base.txt"), "base").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("base.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "base", &tree, &[])
            .unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let release = repo.path().join("release");
        let hooks = repo.path().join("hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join("pre-commit");
        std::fs::write(
            &hook,
            format!(
                "#!/bin/sh\nwhile [ ! -e '{}' ]; do sleep 0.05; done\n",
                release.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, release)
    }

    /// An `AppState` with a worker open for each of `dirs`, as `open_repo` opens them. Returns
    /// the paths they were opened with.
    fn state_with_repos(dirs: &[&std::path::Path]) -> (super::AppState, Vec<String>) {
        let state = super::AppState::default();
        let keys = dirs
            .iter()
            .map(|dir| {
                let key = dir.to_string_lossy().into_owned();
                super::remember_repo(&state, &key).unwrap();
                key
            })
            .collect();
        (state, keys)
    }

    /// The gate of the repository opened as `path`.
    #[cfg(unix)]
    fn gate_of(state: &super::AppState, path: &str) -> std::sync::Arc<super::RepoGate> {
        let (key, _) = state.registry.get(path).unwrap();
        state.gates.lock().unwrap()[&key].clone()
    }

    #[cfg(unix)]
    #[test]
    fn queued_calls_for_a_busy_repo_do_not_delay_another_repo() {
        let (dir_a, release) = repo_with_waiting_pre_commit();
        let dir_b = tempfile::TempDir::new().unwrap();
        git2::Repository::init(dir_b.path()).unwrap();
        let (state, keys) = state_with_repos(&[dir_a.path(), dir_b.path()]);
        let (started_tx, started_rx) = mpsc::channel();
        let commit = tauri::async_runtime::spawn(
            super::repo_worker(&state, &keys[0])
                .unwrap()
                .call(move |w| w.commit("msg".into(), false, Vec::new(), started_tx)),
        );
        assert!(matches!(
            started_rx.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(HookEvent::Started { .. })
        ));
        // More calls than Tokio's blocking pool has threads (512), as repeated refreshes queue
        // up behind a long hook.
        let reads: Vec<_> = (0..600)
            .map(|_| {
                tauri::async_runtime::spawn(
                    super::repo_worker(&state, &keys[0])
                        .unwrap()
                        .call(|w| w.get_status()),
                )
            })
            .collect();
        // Let the runtime start them before repository B asks.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let other = super::repo_worker(&state, &keys[1]).unwrap();
        let (answered_tx, answered_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = tauri::async_runtime::block_on(other.call(|w| w.get_status()));
            let _ = answered_tx.send(result.is_ok());
        });

        let answered = answered_rx.recv_timeout(std::time::Duration::from_secs(5));

        std::fs::write(&release, "").unwrap();
        assert!(matches!(
            tauri::async_runtime::block_on(commit).unwrap(),
            Ok(git_core::commit::CommitOutcome::Committed { .. })
        ));
        for read in reads {
            assert!(tauri::async_runtime::block_on(read).unwrap().is_ok());
        }
        assert_eq!(
            answered,
            Ok(true),
            "calls queued for a busy repository delayed another repository"
        );
    }

    #[cfg(unix)]
    #[test]
    fn repo_worker_call_does_not_hold_a_runtime_thread_while_a_hook_runs() {
        let (dir, release) = repo_with_waiting_pre_commit();
        let (state, keys) = state_with_repos(&[dir.path()]);
        let (started_tx, started_rx) = mpsc::channel();
        let commit = tauri::async_runtime::spawn(
            super::repo_worker(&state, &keys[0])
                .unwrap()
                .call(move |w| w.commit("msg".into(), false, Vec::new(), started_tx)),
        );
        // The hook is running: the worker is busy until the release file appears.
        assert!(matches!(
            started_rx.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(HookEvent::Started { .. })
        ));
        // Each read gets a gate of its own, so every one of them reaches `off_runtime` and waits
        // on the worker there: this checks the #125 property, not the #181 gate.
        let (_, handle) = state.registry.get(&keys[0]).unwrap();
        let reads: Vec<_> = (0..saturating_count())
            .map(|_| {
                tauri::async_runtime::spawn(
                    super::RepoWorker::with_own_gate(handle.clone()).call(|w| w.get_status()),
                )
            })
            .collect();

        let responded = runtime_responds();

        std::fs::write(&release, "").unwrap();
        assert!(matches!(
            tauri::async_runtime::block_on(commit).unwrap(),
            Ok(git_core::commit::CommitOutcome::Committed { .. })
        ));
        for read in reads {
            assert!(tauri::async_runtime::block_on(read).unwrap().is_ok());
        }
        assert!(
            responded,
            "a queued worker call held an async runtime thread"
        );
    }

    /// Starts a commit whose `pre-commit` hook waits for `release`, and returns once the hook
    /// runs, i.e. once the commit's call holds the repository's gate.
    #[cfg(unix)]
    fn hold_gate_with_hook(
        state: &super::AppState,
        key: &str,
    ) -> tauri::async_runtime::JoinHandle<Result<git_core::commit::CommitOutcome, String>> {
        let (started_tx, started_rx) = mpsc::channel();
        let commit = tauri::async_runtime::spawn(
            super::repo_worker(state, key)
                .unwrap()
                .call(move |w| w.commit("msg".into(), false, Vec::new(), started_tx)),
        );
        assert!(matches!(
            started_rx.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(HookEvent::Started { .. })
        ));
        assert!(gate_of(state, key).try_lock().is_err());
        commit
    }

    /// #223: a second spelling of an open repository waits at the same gate.
    #[cfg(unix)]
    #[test]
    fn two_spellings_share_one_gate() {
        let (dir, release) = repo_with_waiting_pre_commit();
        let (state, keys) = state_with_repos(&[dir.path()]);
        let slashed = format!("{}/", keys[0]);
        state.registry.open(&slashed).unwrap();
        let commit = hold_gate_with_hook(&state, &keys[0]);

        let via_other = super::repo_worker(&state, &slashed).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &via_other.gate,
            &gate_of(&state, &keys[0])
        ));
        let (answered_tx, answered_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = tauri::async_runtime::block_on(via_other.call(|w| w.get_status()));
            let _ = answered_tx.send(result.is_ok());
        });
        let while_held = answered_rx.recv_timeout(std::time::Duration::from_millis(500));

        std::fs::write(&release, "").unwrap();
        assert!(tauri::async_runtime::block_on(commit).unwrap().is_ok());
        assert!(
            while_held.is_err(),
            "a call through another spelling ran while the gate was held"
        );
        assert_eq!(
            answered_rx.recv_timeout(std::time::Duration::from_secs(10)),
            Ok(true)
        );
    }

    #[cfg(unix)]
    #[test]
    fn workdir_answers_while_a_hook_holds_the_gate() {
        let (dir, release) = repo_with_waiting_pre_commit();
        let (state, keys) = state_with_repos(&[dir.path()]);
        let commit = hold_gate_with_hook(&state, &keys[0]);

        let (tx, rx) = mpsc::channel();
        let workdir = std::thread::scope(|scope| {
            scope.spawn(|| {
                let _ = tx.send(super::tools::workdir_of(&state, &keys[0]));
            });
            rx.recv_timeout(std::time::Duration::from_secs(2))
        });

        std::fs::write(&release, "").unwrap();
        assert!(tauri::async_runtime::block_on(commit).unwrap().is_ok());
        let workdir = workdir.expect("workdir waited behind the gate").unwrap();
        assert_eq!(
            std::fs::canonicalize(workdir).unwrap(),
            std::fs::canonicalize(dir.path()).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_search_waiting_at_the_gate_is_cancelled_before_it_runs() {
        let (dir, release) = repo_with_waiting_pre_commit();
        let (state, keys) = state_with_repos(&[dir.path()]);
        let commit = hold_gate_with_hook(&state, &keys[0]);
        let search = tauri::async_runtime::spawn(
            super::search::queue_search(&state, &keys[0], "base".into(), None, None).unwrap(),
        );

        super::repo_worker(&state, &keys[0])
            .unwrap()
            .signal()
            .cancel_search();
        std::fs::write(&release, "").unwrap();

        assert!(tauri::async_runtime::block_on(commit).unwrap().is_ok());
        assert_eq!(
            tauri::async_runtime::block_on(search).unwrap().unwrap_err(),
            "Search cancelled."
        );
        // A search started after the cancel runs normally.
        let page = tauri::async_runtime::block_on(
            super::search::queue_search(&state, &keys[0], "base".into(), None, None).unwrap(),
        )
        .unwrap();
        assert_eq!(page.commits.len(), 1);
    }

    #[test]
    fn calls_for_one_repo_keep_their_order() {
        use std::future::Future;
        use std::pin::Pin;
        use std::task::Poll;
        let dir = tempfile::TempDir::new().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        let (state, keys) = state_with_repos(&[dir.path()]);
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        type Call = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
        let mut calls: Vec<Option<Call>> = (0..10u64)
            .map(|i| {
                let seen = seen.clone();
                let call: Call = Box::pin(super::repo_worker(&state, &keys[0]).unwrap().call(
                    move |w| {
                        // Earlier calls take longer: without the gate, later ones would overtake.
                        std::thread::sleep(std::time::Duration::from_millis((10 - i) * 10));
                        w.get_status()?;
                        seen.lock().unwrap().push(i);
                        Ok(())
                    },
                ));
                Some(call)
            })
            .collect();

        // Poll the calls in the order they were made, as the commands they stand for arrive.
        tauri::async_runtime::block_on(std::future::poll_fn(|cx| {
            for slot in calls.iter_mut() {
                if let Some(call) = slot {
                    if let Poll::Ready(result) = call.as_mut().poll(cx) {
                        assert_eq!(result, Ok(()));
                        *slot = None;
                    }
                }
            }
            if calls.iter().all(Option::is_none) {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        }));

        assert_eq!(*seen.lock().unwrap(), (0..10).collect::<Vec<_>>());
    }

    #[test]
    fn closing_a_repo_drops_its_gate() {
        let dir = tempfile::TempDir::new().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        let (state, keys) = state_with_repos(&[dir.path()]);
        let first = super::repo_worker(&state, &keys[0]).unwrap();
        let again = super::repo_worker(&state, &keys[0]).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first.gate, &again.gate));

        super::forget_repo(&state, &keys[0]);

        assert!(state.gates.lock().unwrap().is_empty());
        assert!(state.registry.is_empty());
        assert!(super::repo_worker(&state, &keys[0]).is_err());
    }

    /// #223: reopening a spelling that now names another repository drops the old one's gate.
    #[cfg(unix)]
    #[test]
    fn reopening_a_spelling_that_names_another_repository_drops_the_old_gate() {
        let first = tempfile::TempDir::new().unwrap();
        git2::Repository::init(first.path()).unwrap();
        let second = tempfile::TempDir::new().unwrap();
        git2::Repository::init(second.path()).unwrap();
        let links = tempfile::TempDir::new().unwrap();
        let link = links.path().join("link");
        std::os::unix::fs::symlink(first.path(), &link).unwrap();
        let (state, keys) = state_with_repos(&[&link]);
        super::repo_worker(&state, &keys[0]).unwrap();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(second.path(), &link).unwrap();

        super::remember_repo(&state, &keys[0]).unwrap();

        assert!(state.gates.lock().unwrap().is_empty());
        assert_eq!(state.registry.len(), 1);
        super::repo_worker(&state, &keys[0]).unwrap();
        let (key, _) = state.registry.get(&keys[0]).unwrap();
        assert_eq!(
            state.gates.lock().unwrap().keys().collect::<Vec<_>>(),
            [&key]
        );
    }

    /// #223: closing one spelling's tab keeps the repository's worker and gate for the other.
    #[test]
    fn closing_one_spelling_keeps_the_gate() {
        let dir = tempfile::TempDir::new().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let (state, keys) = state_with_repos(&[dir.path()]);
        let sub = dir.path().join("sub").to_string_lossy().into_owned();
        state.registry.open(&sub).unwrap();
        let first = super::repo_worker(&state, &keys[0]).unwrap();

        super::forget_repo(&state, &sub);

        let again = super::repo_worker(&state, &keys[0]).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first.gate, &again.gate));
        assert_eq!(state.gates.lock().unwrap().len(), 1);
        assert!(super::repo_worker(&state, &sub).is_err());
        assert!(tauri::async_runtime::block_on(again.call(|w| w.get_status())).is_ok());
    }
    use std::path::PathBuf;

    use config::{OpenRepoEntry, Workspace};
    use git_core::graph::GraphCommit;
    use git_core::reflog::ReflogEntry;
    use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};
    use git_core::submodule::SubmoduleInfo;
    use git_core::worktree::WorktreeInfo;

    use repo_service::worker::{HookEvent, HookSource, TransferEvent};

    use super::{hook_progress_dto, ChangedFileDto, CommitOutcomeDto, CompareRangeDto};
    use super::{
        transfer_event_payload, BranchInfoDto, CommitSignatureDto, DiffOptionsDto, DiffSettingsDto,
        FileHistoryEntryDto, ForgeProviderDto, GraphCommitDto, HeadInfoDto, OpenRepoEntryDto,
        OpenRepoEntryInput, PickOutcomeDto, PullOutcomeDto, ReflogEntryDto, RemoteAuthModeDto,
        RemoteInfoDto, ResetModeDto, SearchPageDto, SubmoduleInfoDto, UndoOutcomeDto, UndoStateDto,
        UpstreamInfoDto, WorkspaceDto, WorktreeInfoDto,
    };
    use super::{CommitMessageSettingsDto, FileSidesDto, SignatureInputDto, TrailerDto};
    use super::{
        MergeOptionsDto, MergeOutcomeDto, StashApplyOutcomeDto, StashFilesDto, StashOptionsDto,
    };

    #[test]
    fn tag_info_dto_serializes_the_peeled_commit_id_in_camel_case() {
        let tag = |commit_id: Option<&str>| git_core::remote::TagInfo {
            name: "v1".into(),
            target_id: "tagobj".into(),
            commit_id: commit_id.map(str::to_string),
            annotated: true,
            message: Some("m".into()),
            tagger_name: None,
            timestamp: None,
        };
        let json = serde_json::to_value(super::TagInfoDto::from(tag(Some("abc")))).unwrap();
        assert_eq!(json["targetId"], "tagobj");
        assert_eq!(json["commitId"], "abc");
        let json = serde_json::to_value(super::TagInfoDto::from(tag(None))).unwrap();
        assert_eq!(json["commitId"], serde_json::Value::Null);
    }

    #[test]
    fn file_sides_dto_encodes_base64_sniffs_the_type_and_tags_each_side() {
        use git_core::diff::{FileSide, FileSides};
        let png = b"\x89PNG\r\n\x1a\n".to_vec();
        let dto = serde_json::to_value(FileSidesDto::from(FileSides {
            old: FileSide::Loaded {
                bytes: b"foo".to_vec(),
            },
            new: FileSide::Loaded { bytes: png },
        }))
        .unwrap();
        assert_eq!(
            dto,
            serde_json::json!({
                "old": { "kind": "Loaded", "size": 3, "base64": "Zm9v", "mime": null },
                "new": { "kind": "Loaded", "size": 8, "base64": "iVBORw0KGgo=", "mime": "image/png" },
            })
        );
        let big = serde_json::to_value(FileSidesDto::from(FileSides {
            old: FileSide::TooLarge { size: 25_000_000 },
            new: FileSide::Missing,
        }))
        .unwrap();
        assert_eq!(
            big,
            serde_json::json!({
                "old": { "kind": "TooLarge", "size": 25_000_000 },
                "new": { "kind": "Missing" },
            })
        );
    }

    #[test]
    fn commit_inputs_and_settings_dtos_are_camel_case() {
        let trailer: TrailerDto = serde_json::from_value(
            serde_json::json!({ "key": "Co-authored-by", "value": "A <a@x>" }),
        )
        .unwrap();
        assert_eq!(
            git_core::commit::Trailer::from(trailer),
            git_core::commit::Trailer {
                key: "Co-authored-by".into(),
                value: "A <a@x>".into()
            }
        );
        assert!(super::trailers_from(None).is_empty());
        let author: SignatureInputDto =
            serde_json::from_value(serde_json::json!({ "name": "Ada", "email": "a@x" })).unwrap();
        assert_eq!(git_core::commit::SignatureInput::from(author).email, "a@x");
        assert_eq!(
            serde_json::to_value(SignatureInputDto::from(git_core::commit::SignatureInput {
                name: "Bob".into(),
                email: "b@x".into()
            }))
            .unwrap(),
            serde_json::json!({ "name": "Bob", "email": "b@x" })
        );
        let settings = serde_json::to_value(CommitMessageSettingsDto::from(
            config::CommitMessageSettings::default(),
        ))
        .unwrap();
        assert_eq!(
            settings,
            serde_json::json!({ "summaryLimit": 50, "bodyRuler": 72, "showRuler": true })
        );
        let back: CommitMessageSettingsDto = serde_json::from_value(
            serde_json::json!({ "summaryLimit": 60, "bodyRuler": 80, "showRuler": false }),
        )
        .unwrap();
        assert_eq!(
            config::CommitMessageSettings::from(back),
            config::CommitMessageSettings {
                summary_limit: 60,
                body_ruler: 80,
                show_ruler: false
            }
        );
    }

    #[test]
    fn diff_options_dto_parses_camel_case() {
        let dto: DiffOptionsDto = serde_json::from_value(
            serde_json::json!({"ignoreWhitespace": true, "contextLines": 5}),
        )
        .unwrap();
        assert_eq!(
            git_core::diff::DiffOptions::from(dto),
            git_core::diff::DiffOptions {
                ignore_whitespace: true,
                context_lines: 5
            }
        );
    }

    #[test]
    fn status_entry_dto_always_serializes_old_path() {
        use git_core::status::{StatusEntry, StatusKind};
        let plain = super::StatusEntryDto::from(StatusEntry {
            path: "a.txt".into(),
            old_path: None,
            staged: false,
            kind: StatusKind::Modified,
        });
        assert_eq!(
            serde_json::to_value(plain).unwrap(),
            serde_json::json!({"path": "a.txt", "oldPath": null, "staged": false, "kind": "Modified"})
        );
        let renamed = super::StatusEntryDto::from(StatusEntry {
            path: "new.txt".into(),
            old_path: Some("old.txt".into()),
            staged: true,
            kind: StatusKind::Renamed,
        });
        assert_eq!(
            serde_json::to_value(renamed).unwrap()["oldPath"],
            serde_json::json!("old.txt")
        );
    }

    #[test]
    fn diff_source_dto_parses_every_variant() {
        use super::DiffSourceDto;
        use git_core::tools::DiffSource;
        let parse = |v: serde_json::Value| {
            DiffSource::from(serde_json::from_value::<DiffSourceDto>(v).unwrap())
        };
        assert_eq!(
            parse(serde_json::json!({"kind": "Unstaged"})),
            DiffSource::Unstaged
        );
        assert_eq!(
            parse(serde_json::json!({"kind": "Staged"})),
            DiffSource::Staged { old_path: None }
        );
        assert_eq!(
            parse(serde_json::json!({"kind": "Staged", "oldPath": "a.png"})),
            DiffSource::Staged {
                old_path: Some("a.png".into())
            }
        );
        assert_eq!(
            parse(serde_json::json!({"kind": "Commit", "commitId": "abc", "oldPath": null})),
            DiffSource::Commit {
                commit_id: "abc".into(),
                old_path: None
            }
        );
        assert_eq!(
            parse(
                serde_json::json!({"kind": "Range", "fromId": "a", "toId": "b", "oldPath": "old.rs"})
            ),
            DiffSource::Range {
                from_id: "a".into(),
                to_id: Some("b".into()),
                old_path: Some("old.rs".into())
            }
        );
        assert_eq!(
            parse(
                serde_json::json!({"kind": "Range", "fromId": "a", "toId": null, "oldPath": null})
            ),
            DiffSource::Range {
                from_id: "a".into(),
                to_id: None,
                old_path: None
            }
        );
        assert!(
            serde_json::from_value::<DiffSourceDto>(serde_json::json!({"kind": "Nope"})).is_err()
        );
    }

    #[test]
    fn tool_outcome_and_settings_dtos_serialize_for_the_frontend() {
        use super::{
            ExternalToolSettingsDto, GitToolsDto, MarkResolvedOutcomeDto, MergeToolOutcomeDto,
        };
        assert_eq!(
            serde_json::to_value(MergeToolOutcomeDto::from(
                git_core::tools::MergeToolOutcome::Resolved
            ))
            .unwrap(),
            "Resolved"
        );
        assert_eq!(
            serde_json::to_value(MergeToolOutcomeDto::from(
                git_core::tools::MergeToolOutcome::Unresolved
            ))
            .unwrap(),
            "Unresolved"
        );
        assert_eq!(
            serde_json::to_value(MarkResolvedOutcomeDto::from(
                git_core::merge::MarkResolvedOutcome::MarkersRemain
            ))
            .unwrap(),
            "MarkersRemain"
        );
        let settings: ExternalToolSettingsDto =
            serde_json::from_value(serde_json::json!({"editor": "code", "terminal": null}))
                .unwrap();
        assert_eq!(
            config::ExternalToolSettings::from(settings),
            config::ExternalToolSettings {
                editor: Some("code".into()),
                terminal: None
            }
        );
        assert_eq!(
            serde_json::to_value(ExternalToolSettingsDto::from(
                config::ExternalToolSettings {
                    editor: None,
                    terminal: Some("kitty".into())
                }
            ))
            .unwrap(),
            serde_json::json!({"editor": null, "terminal": "kitty"})
        );
        let tools = GitToolsDto::from(git_core::tools::GitTools {
            diff_tool: Some(git_core::tools::GitToolInfo {
                name: "meld".into(),
                command: Some("meld $LOCAL $REMOTE".into()),
                source: git_core::tools::ConfigSource::Global,
                via_merge_tool: true,
                problem: None,
            }),
            merge_tool: None,
        });
        assert_eq!(
            serde_json::to_value(tools).unwrap(),
            serde_json::json!({
                "diffTool": {"name": "meld", "command": "meld $LOCAL $REMOTE", "source": "Global", "viaMergeTool": true, "problem": null},
                "mergeTool": null
            })
        );
    }

    #[test]
    fn diff_settings_dto_round_trips_camel_case() {
        let dto = DiffSettingsDto::from(config::DiffSettings {
            ignore_whitespace: false,
            context_lines: 3,
            syntax_highlight: true,
        });
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"ignoreWhitespace": false, "contextLines": 3, "syntaxHighlight": true})
        );
        let back: DiffSettingsDto = serde_json::from_value(json).unwrap();
        assert_eq!(config::DiffSettings::from(back).context_lines, 3);
    }

    #[test]
    fn workspace_dto_serializes_camel_case_paths() {
        let dto = WorkspaceDto::from(Workspace {
            id: "workspace-1".into(),
            name: "Project Suite".into(),
            root_path: PathBuf::from("/repos/suite"),
            member_paths: vec![
                PathBuf::from("/repos/suite/api"),
                PathBuf::from("/repos/suite/web"),
            ],
        });

        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "id": "workspace-1",
                "name": "Project Suite",
                "rootPath": "/repos/suite",
                "memberPaths": ["/repos/suite/api", "/repos/suite/web"],
            })
        );
    }

    #[test]
    fn open_repo_entry_dto_serializes_camel_case_workspace_id() {
        let dto = OpenRepoEntryDto::from(OpenRepoEntry {
            path: PathBuf::from("/repos/suite/api"),
            workspace_id: Some("workspace-1".into()),
        });

        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "path": "/repos/suite/api",
                "workspaceId": "workspace-1",
            })
        );
    }

    #[test]
    fn open_repo_entry_input_deserializes_camel_case_workspace_id() {
        let input: OpenRepoEntryInput = serde_json::from_value(serde_json::json!({
            "path": "/repos/suite/api",
            "workspaceId": "workspace-1",
        }))
        .unwrap();

        assert_eq!(input.path, "/repos/suite/api");
        assert_eq!(input.workspace_id, Some("workspace-1".into()));
    }

    #[test]
    fn remote_failed_event_carries_the_remote_name_and_other_events_omit_it() {
        let (event, failed) = transfer_event_payload(TransferEvent::RemoteFailed {
            operation_id: "fetch-all-7".into(),
            remote_name: "backup".into(),
            error: TransferErrorKind::MissingCredential,
        });
        assert_eq!(event, "transfer-progress");
        let json = serde_json::to_value(failed).unwrap();
        assert_eq!(json["phase"], "RemoteFailed");
        assert_eq!(json["operation"], "FetchAll");
        assert_eq!(json["operationId"], "fetch-all-7");
        assert_eq!(json["remoteName"], "backup");
        assert_eq!(json["errorKind"], "MissingCredential");

        let (_, completed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-all-7".into(),
            operation: TransferOperation::FetchAll,
            error: None,
            hook_output: None,
        });
        let json = serde_json::to_value(completed).unwrap();
        assert!(!json.as_object().unwrap().contains_key("remoteName"));
        assert_eq!(json["operation"], "FetchAll");
    }

    #[test]
    fn upstream_info_dto_serializes_the_push_branch_including_null() {
        let upstream = |push_branch: Option<&str>| {
            serde_json::to_value(UpstreamInfoDto::from(git_core::remote::UpstreamInfo {
                local_branch: "fx".into(),
                remote_name: "origin".into(),
                remote_branch: "feature-x".into(),
                ahead: None,
                behind: None,
                push_branch: push_branch.map(str::to_string),
            }))
            .unwrap()
        };
        assert_eq!(upstream(Some("feature-x"))["pushBranch"], "feature-x");
        let json = upstream(None);
        assert!(json.as_object().unwrap().contains_key("pushBranch"));
        assert_eq!(json["pushBranch"], serde_json::Value::Null);
    }

    #[test]
    fn remote_info_dto_serializes_the_prune_default() {
        let json = serde_json::to_value(RemoteInfoDto::from((
            git_core::remote::RemoteInfo {
                name: "origin".into(),
                fetch_url: "https://example.com/repo.git".into(),
                push_url: None,
                prune: true,
            },
            None,
        )))
        .unwrap();
        assert_eq!(json["prune"], true);
        assert_eq!(json["fetchUrl"], "https://example.com/repo.git");
    }

    #[test]
    fn branch_info_dto_serializes_upstream_and_counts_including_nulls() {
        let tracked = serde_json::to_value(BranchInfoDto::from(git_core::branch::BranchInfo {
            name: "main".into(),
            is_current: true,
            upstream: Some("origin/main".into()),
            ahead: Some(2),
            behind: Some(0),
        }))
        .unwrap();
        assert_eq!(tracked["isCurrent"], true);
        assert_eq!(tracked["upstream"], "origin/main");
        assert_eq!(tracked["ahead"], 2);
        assert_eq!(tracked["behind"], 0);

        let untracked = serde_json::to_value(BranchInfoDto::from(git_core::branch::BranchInfo {
            name: "topic".into(),
            is_current: false,
            upstream: None,
            ahead: None,
            behind: None,
        }))
        .unwrap();
        let object = untracked.as_object().unwrap();
        assert_eq!(object["upstream"], serde_json::Value::Null);
        assert!(object.contains_key("ahead") && object.contains_key("behind"));
    }

    #[test]
    fn graph_commit_dto_serializes_camel_case_fields() {
        let dto = GraphCommitDto::from(GraphCommit {
            id: "abc123".into(),
            short_id: "abc123".into(),
            summary: "Add remote branch badges".into(),
            author_name: "Rene".into(),
            author_email: "rene@example.com".into(),
            timestamp: 1_700_000_000,
            parent_ids: vec!["def456".into()],
            branch_refs: vec!["main".into()],
            remote_branch_refs: vec!["origin/main".into()],
            tags: vec!["v1.0".into()],
        });

        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "id": "abc123",
                "shortId": "abc123",
                "summary": "Add remote branch badges",
                "authorName": "Rene",
                "authorEmail": "rene@example.com",
                "timestamp": 1_700_000_000i64,
                "parentIds": ["def456"],
                "branchRefs": ["main"],
                "remoteBranchRefs": ["origin/main"],
                "tags": ["v1.0"],
            })
        );
    }

    #[test]
    fn file_history_entry_dto_serializes_camel_case_and_null_old_path() {
        let commit = GraphCommit {
            id: "abc123".into(),
            short_id: "abc123".into(),
            summary: "Edit".into(),
            author_name: "Rene".into(),
            author_email: "rene@example.com".into(),
            timestamp: 1_700_000_000,
            parent_ids: vec![],
            branch_refs: vec![],
            remote_branch_refs: vec![],
            tags: vec![],
        };
        let plain = FileHistoryEntryDto::from(git_core::history::FileHistoryEntry {
            commit: commit.clone(),
            path: "new.txt".into(),
            old_path: None,
        });
        let renamed = FileHistoryEntryDto::from(git_core::history::FileHistoryEntry {
            commit,
            path: "new.txt".into(),
            old_path: Some("old.txt".into()),
        });

        let plain = serde_json::to_value(plain).unwrap();
        assert_eq!(plain["path"], "new.txt");
        assert_eq!(plain["oldPath"], serde_json::Value::Null);
        assert!(plain.as_object().unwrap().contains_key("oldPath"));
        assert_eq!(plain["commit"]["shortId"], "abc123");
        assert_eq!(serde_json::to_value(renamed).unwrap()["oldPath"], "old.txt");
    }

    #[test]
    fn search_page_dto_serializes_camel_case_and_null_cursor() {
        let commit = GraphCommit {
            id: "abc123".into(),
            short_id: "abc123".into(),
            summary: "Hit".into(),
            author_name: "Rene".into(),
            author_email: "rene@example.com".into(),
            timestamp: 1_700_000_000,
            parent_ids: vec![],
            branch_refs: vec![],
            remote_branch_refs: vec![],
            tags: vec![],
        };
        let done = serde_json::to_value(SearchPageDto::from(git_core::search::SearchPage {
            commits: vec![commit.clone()],
            next_cursor: None,
            scanned: 42,
        }))
        .unwrap();
        assert_eq!(done["nextCursor"], serde_json::Value::Null);
        assert!(done.as_object().unwrap().contains_key("nextCursor"));
        assert_eq!(done["scanned"], 42);
        assert_eq!(done["commits"][0]["shortId"], "abc123");

        let more = serde_json::to_value(SearchPageDto::from(git_core::search::SearchPage {
            commits: vec![],
            next_cursor: Some("20000:0123456789abcdef".into()),
            scanned: 20_000,
        }))
        .unwrap();
        assert_eq!(more["nextCursor"], "20000:0123456789abcdef");
    }

    #[test]
    fn compare_dtos_serialize_camel_case_with_a_null_old_path() {
        let plain = serde_json::to_value(ChangedFileDto::from(git_core::diff::ChangedFile {
            path: "a.txt".into(),
            old_path: None,
        }))
        .unwrap();
        assert_eq!(
            plain,
            serde_json::json!({ "path": "a.txt", "oldPath": null })
        );

        let renamed = serde_json::to_value(ChangedFileDto::from(git_core::diff::ChangedFile {
            path: "new.txt".into(),
            old_path: Some("old.txt".into()),
        }))
        .unwrap();
        assert_eq!(renamed["oldPath"], "old.txt");

        let range = serde_json::to_value(CompareRangeDto::from(git_core::diff::CompareRange {
            from_id: "base".into(),
            to_id: "tip".into(),
        }))
        .unwrap();
        assert_eq!(
            range,
            serde_json::json!({ "fromId": "base", "toId": "tip" })
        );
    }

    #[test]
    fn worktree_info_dto_serializes_camel_case_fields() {
        let dto = WorktreeInfoDto::from(WorktreeInfo {
            name: "feature-tree".into(),
            path: PathBuf::from("/repos/project-feature"),
            head: "refs/heads/feature".into(),
            is_main: false,
            is_locked: true,
            is_prunable: false,
        });

        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "name": "feature-tree",
                "path": "/repos/project-feature",
                "head": "refs/heads/feature",
                "isMain": false,
                "isLocked": true,
                "isPrunable": false,
            })
        );
    }

    #[test]
    fn worktree_info_dto_serializes_an_unknown_head_as_null() {
        let dto = WorktreeInfoDto::from(WorktreeInfo {
            name: "stale-tree".into(),
            path: PathBuf::from("/repos/stale-tree"),
            head: String::new(),
            is_main: false,
            is_locked: false,
            is_prunable: true,
        });

        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "name": "stale-tree",
                "path": "/repos/stale-tree",
                "head": null,
                "isMain": false,
                "isLocked": false,
                "isPrunable": true,
            })
        );
    }

    #[test]
    fn submodule_info_dto_serializes_camel_case_fields() {
        let dto = SubmoduleInfoDto::from(SubmoduleInfo {
            path: "deps/child".into(),
            url: Some("https://example.com/child.git".into()),
            gitlink_id: Some("0123456789abcdef".into()),
            initialized: true,
            head_id: Some("fedcba9876543210".into()),
        });

        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "path": "deps/child",
                "url": "https://example.com/child.git",
                "gitlinkId": "0123456789abcdef",
                "initialized": true,
                "headId": "fedcba9876543210",
            })
        );
    }

    #[test]
    fn reflog_entry_dto_serializes_camel_case_fields_and_optional_summary() {
        let with_summary = ReflogEntryDto::from(ReflogEntry {
            reference: "HEAD".into(),
            old_id: "1111111".into(),
            new_id: "2222222".into(),
            committer_name: "Test User".into(),
            committer_email: "test@example.com".into(),
            timestamp: 1_725_000_000,
            message: "commit: second commit".into(),
            summary: Some("second commit".into()),
        });
        let without_summary = ReflogEntryDto::from(ReflogEntry {
            reference: "refs/heads/main".into(),
            old_id: "2222222".into(),
            new_id: "3333333".into(),
            committer_name: "Test User".into(),
            committer_email: "test@example.com".into(),
            timestamp: 1_725_000_001,
            message: "branch: reset".into(),
            summary: None,
        });

        assert_eq!(
            serde_json::to_value(with_summary).unwrap(),
            serde_json::json!({
                "reference": "HEAD",
                "oldId": "1111111",
                "newId": "2222222",
                "committerName": "Test User",
                "committerEmail": "test@example.com",
                "timestamp": 1_725_000_000,
                "message": "commit: second commit",
                "summary": "second commit",
            })
        );
        assert_eq!(
            serde_json::to_value(without_summary).unwrap()["summary"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn remote_auth_mode_wire_values_match_the_typescript_union() {
        assert_eq!(
            serde_json::to_value(RemoteAuthModeDto::HttpsToken).unwrap(),
            serde_json::json!("HttpsToken")
        );
        assert_eq!(
            serde_json::to_value(RemoteAuthModeDto::SshAgent).unwrap(),
            serde_json::json!("SshAgent")
        );
    }

    #[test]
    fn forge_provider_wire_values_match_the_typescript_union() {
        assert_eq!(
            serde_json::to_value(ForgeProviderDto::GitHub).unwrap(),
            serde_json::json!("GitHub")
        );
        assert_eq!(
            serde_json::to_value(ForgeProviderDto::Bitbucket).unwrap(),
            serde_json::json!("Bitbucket")
        );
    }

    #[test]
    fn pull_outcome_wire_values_match_the_typescript_union() {
        let fast_forwarded = serde_json::to_value(PullOutcomeDto::FastForwarded {
            upstream_ref: "refs/remotes/origin/main".into(),
        })
        .expect("serialize pull outcome");
        let diverged = serde_json::to_value(PullOutcomeDto::Diverged {
            upstream_ref: "refs/remotes/origin/main".into(),
        })
        .expect("serialize pull outcome");

        assert_eq!(
            serde_json::json!({ "kind": "UpToDate" }),
            serde_json::to_value(PullOutcomeDto::UpToDate).unwrap()
        );
        assert_eq!(
            serde_json::json!({ "kind": "FastForwarded", "upstreamRef": "refs/remotes/origin/main" }),
            fast_forwarded
        );
        assert_eq!(
            serde_json::json!({ "kind": "Diverged", "upstreamRef": "refs/remotes/origin/main" }),
            diverged
        );
    }

    #[test]
    fn transfer_event_bridge_redacts_sideband_and_failure_messages() {
        let (_, progress) = transfer_event_payload(TransferEvent::Progress(TransferProgress {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            phase: TransferPhase::Receiving,
            current: 2,
            total: 4,
            received_bytes: 1024,
            message: Some("Authorization: Bearer secret-token".into()),
        }));
        let (_, completed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::TransferFailed),
            hook_output: None,
        });

        assert_eq!(progress.message, None);
        assert_eq!(completed.message, None);
        assert_eq!(completed.error_kind.as_deref(), Some("TransferFailed"));
    }

    #[test]
    fn transfer_failure_is_emitted_as_a_sanitized_failed_terminal_event() {
        let (event_name, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::TransferFailed),
            hook_output: None,
        });

        assert_eq!(event_name, "transfer-complete");
        assert_eq!(failed.phase, "Failed");
        assert_eq!(failed.operation, "Fetch");
        assert_eq!(failed.error_kind.as_deref(), Some("TransferFailed"));
        assert_eq!(failed.message, None);
    }

    #[test]
    fn a_cancelled_transfer_is_emitted_with_its_own_terminal_error_kind() {
        // The frontend distinguishes "you cancelled this" from a real transport failure purely
        // by this wire value, so it must not collapse into `TransferFailed`.
        let (event_name, cancelled) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::Cancelled),
            hook_output: None,
        });

        assert_eq!(event_name, "transfer-complete");
        assert_eq!(cancelled.error_kind.as_deref(), Some("Cancelled"));
        assert_eq!(cancelled.message, None);
    }

    #[test]
    fn missing_credential_failure_is_emitted_as_a_safe_terminal_kind() {
        let (_, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::MissingCredential),
            hook_output: None,
        });

        assert_eq!(failed.error_kind.as_deref(), Some("MissingCredential"));
        assert_eq!(failed.message, None);
    }

    #[test]
    fn push_failure_payload_preserves_only_safe_operation_and_error_kinds() {
        let (_, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "push-42".into(),
            operation: TransferOperation::PushBranch,
            error: Some(TransferErrorKind::NonFastForward),
            hook_output: None,
        });

        assert_eq!(failed.operation, "PushBranch");
        assert_eq!(failed.error_kind.as_deref(), Some("NonFastForward"));
        assert_eq!(failed.message, None);
    }

    #[test]
    fn commit_outcome_dto_is_kind_tagged() {
        assert_eq!(
            serde_json::to_value(CommitOutcomeDto::from(
                git_core::commit::CommitOutcome::Committed {
                    commit_id: "c1".into()
                }
            ))
            .unwrap(),
            serde_json::json!({"kind": "Committed", "commitId": "c1"})
        );
        assert_eq!(
            serde_json::to_value(CommitOutcomeDto::from(
                git_core::commit::CommitOutcome::HookFailed {
                    hook: "pre-commit".into(),
                    output: "no\n".into()
                }
            ))
            .unwrap(),
            serde_json::json!({"kind": "HookFailed", "hook": "pre-commit", "output": "no\n"})
        );
    }

    #[test]
    fn hook_progress_dto_carries_the_repo_and_phase() {
        let dto = hook_progress_dto(
            "/repo",
            HookEvent::Started {
                operation_id: "commit-1".into(),
                hook: "pre-commit".into(),
                source: HookSource::Commit,
            },
        );
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({"repoPath": "/repo", "operationId": "commit-1", "hook": "pre-commit", "phase": "Started", "source": "Commit"})
        );
        let dto = hook_progress_dto(
            "/repo",
            HookEvent::Finished {
                operation_id: "commit-1".into(),
                hook: "pre-commit".into(),
                source: HookSource::Commit,
            },
        );
        assert_eq!(serde_json::to_value(dto).unwrap()["phase"], "Finished");
    }

    #[test]
    fn hook_progress_dto_serializes_its_source() {
        for (source, name) in [
            (HookSource::Commit, "Commit"),
            (HookSource::Checkout, "Checkout"),
            (HookSource::Merge, "Merge"),
            (HookSource::Pick, "Pick"),
        ] {
            let dto = hook_progress_dto(
                "/repo",
                HookEvent::Started {
                    operation_id: "switch-1".into(),
                    hook: "post-checkout".into(),
                    source,
                },
            );
            assert_eq!(serde_json::to_value(dto).unwrap()["source"], name);
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
    fn a_hook_failed_transfer_carries_its_output_and_others_do_not() {
        let (_, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "push-1".into(),
            operation: TransferOperation::PushBranch,
            error: Some(TransferErrorKind::HookFailed),
            hook_output: Some("tests failed\n".into()),
        });
        let value = serde_json::to_value(failed).unwrap();
        assert_eq!(value["errorKind"], "HookFailed");
        assert_eq!(value["hookOutput"], "tests failed\n");
        assert_eq!(value["message"], serde_json::Value::Null);
        let (_, started) = transfer_event_payload(TransferEvent::Started {
            operation_id: "push-1".into(),
            operation: TransferOperation::PushBranch,
        });
        assert_eq!(
            serde_json::to_value(started).unwrap()["hookOutput"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn head_info_dto_serializes_camel_case_with_operation_name() {
        let dto = HeadInfoDto::from(git_core::repo::HeadInfo {
            head_id: Some("abc".into()),
            detached_at: None,
            head_pushed: true,
            operation: Some(git_core::repo::OperationKind::CherryPick),
            squash_message: None,
            commit_hooks: vec!["pre-commit".into()],
            sequence: None,
            rebase: None,
            bisect: None,
            terminal_sequence: None,
            terminal_rebase: None,
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({"headId": "abc", "detachedAt": null, "headPushed": true, "operation": "CherryPick", "squashMessage": null, "commitHooks": ["pre-commit"], "sequence": null, "rebase": null, "bisect": null, "terminalSequence": null, "terminalRebase": null})
        );
        let dto = HeadInfoDto::from(git_core::repo::HeadInfo {
            head_id: Some("abc".into()),
            detached_at: None,
            head_pushed: false,
            operation: None,
            squash_message: Some("Squashed commit of the following:\n".into()),
            commit_hooks: vec![],
            sequence: None,
            rebase: None,
            bisect: None,
            terminal_sequence: None,
            terminal_rebase: None,
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap()["squashMessage"],
            serde_json::json!("Squashed commit of the following:\n")
        );
    }

    #[test]
    fn sequence_outcome_stopped_serializes_the_kept_commit_and_progress() {
        use git_core::pick::{SequenceKind, SequenceOutcome, SequenceProgress};
        let stopped =
            serde_json::to_value(super::SequenceOutcomeDto::from(SequenceOutcome::Stopped {
                kept: "f one".into(),
                progress: SequenceProgress {
                    kind: SequenceKind::CherryPick,
                    current_step: 2,
                    total_steps: 3,
                    current_commit: "c2".into(),
                },
            }))
            .unwrap();
        assert_eq!(stopped["kind"], "Stopped");
        assert_eq!(stopped["kept"], "f one");
        assert_eq!(stopped["progress"]["currentStep"], 2);
        assert_eq!(stopped["progress"]["currentCommit"], "c2");
    }

    #[test]
    fn sequence_outcome_hook_failed_serializes_with_hook_output_and_progress() {
        use git_core::pick::{SequenceKind, SequenceOutcome, SequenceProgress};
        let failed = serde_json::to_value(super::SequenceOutcomeDto::from(
            SequenceOutcome::HookFailed {
                hook: "commit-msg".into(),
                output: "x".into(),
                progress: SequenceProgress {
                    kind: SequenceKind::CherryPick,
                    current_step: 2,
                    total_steps: 3,
                    current_commit: "abc".into(),
                },
            },
        ))
        .unwrap();
        assert_eq!(
            failed,
            serde_json::json!({
                "kind": "HookFailed",
                "hook": "commit-msg",
                "output": "x",
                "progress": { "kind": "CherryPick", "currentStep": 2, "totalSteps": 3, "currentCommit": "abc" }
            })
        );
    }

    #[test]
    fn sequence_outcome_and_progress_dtos_serialize_tagged_camel_case() {
        use git_core::pick::{SequenceKind, SequenceOutcome, SequenceProgress};
        let progress = SequenceProgress {
            kind: SequenceKind::CherryPick,
            current_step: 2,
            total_steps: 5,
            current_commit: "abc".into(),
        };
        let conflicted = serde_json::to_value(super::SequenceOutcomeDto::from(
            SequenceOutcome::Conflicted {
                files: vec!["a.txt".into()],
                progress: progress.clone(),
            },
        ))
        .unwrap();
        assert_eq!(
            conflicted,
            serde_json::json!({
                "kind": "Conflicted",
                "files": ["a.txt"],
                "progress": { "kind": "CherryPick", "currentStep": 2, "totalSteps": 5, "currentCommit": "abc" }
            })
        );
        let completed = serde_json::to_value(super::SequenceOutcomeDto::from(
            SequenceOutcome::Completed {
                applied: 2,
                skipped: vec!["s".into()],
            },
        ))
        .unwrap();
        assert_eq!(
            completed,
            serde_json::json!({ "kind": "Completed", "applied": 2, "skipped": ["s"] })
        );

        let head = |sequence| {
            serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
                head_id: None,
                detached_at: None,
                head_pushed: false,
                operation: None,
                squash_message: None,
                commit_hooks: vec![],
                sequence,
                rebase: None,
                bisect: None,
                terminal_sequence: None,
                terminal_rebase: None,
            }))
            .unwrap()
        };
        let reverting = head(Some(SequenceProgress {
            kind: SequenceKind::Revert,
            ..progress
        }));
        assert_eq!(reverting["sequence"]["kind"], "Revert");
        assert_eq!(reverting["sequence"]["currentStep"], 2);
        let none = head(None);
        assert!(none.as_object().unwrap().contains_key("sequence"));
        assert_eq!(none["sequence"], serde_json::Value::Null);
    }

    #[test]
    fn sequence_abort_dto_and_conflict_side_input() {
        assert_eq!(
            serde_json::to_value(super::SequenceAbortDto::from(
                git_core::pick::SequenceAbort { rewound: false }
            ))
            .unwrap(),
            serde_json::json!({ "rewound": false })
        );
        let side: super::ConflictSideDto =
            serde_json::from_value(serde_json::json!("Theirs")).unwrap();
        assert_eq!(
            git_core::merge::ConflictSide::from(side),
            git_core::merge::ConflictSide::Theirs
        );
        assert!(
            serde_json::from_value::<super::ConflictSideDto>(serde_json::json!("Base")).is_err()
        );
    }

    #[test]
    fn merge_options_dto_round_trips_camel_case() {
        let dto: MergeOptionsDto =
            serde_json::from_value(serde_json::json!({"fastForward": "Only", "squash": true}))
                .unwrap();
        assert_eq!(
            git_core::merge::MergeOptions::from(dto),
            git_core::merge::MergeOptions {
                fast_forward: git_core::merge::FastForward::Only,
                squash: true
            }
        );
        assert_eq!(
            serde_json::to_value(MergeOptionsDto::from(
                git_core::merge::MergeOptions::default()
            ))
            .unwrap(),
            serde_json::json!({"fastForward": "Allow", "squash": false})
        );
        let never: MergeOptionsDto =
            serde_json::from_value(serde_json::json!({"fastForward": "Never", "squash": false}))
                .unwrap();
        let preference = config::MergePreference {
            repo_path: "/r".into(),
            fast_forward: config::MergeFastForward::Never,
            squash: false,
        };
        assert_eq!(
            serde_json::to_value(MergeOptionsDto::from(preference)).unwrap(),
            serde_json::to_value(never).unwrap()
        );
    }

    #[test]
    fn merge_outcome_dto_has_squashed() {
        assert_eq!(
            serde_json::to_value(MergeOutcomeDto::from(
                git_core::merge::MergeOutcome::Squashed
            ))
            .unwrap(),
            serde_json::json!({"kind": "Squashed"})
        );
    }

    #[test]
    fn stash_dtos_serialize_camel_case() {
        let opts: StashOptionsDto = serde_json::from_value(serde_json::json!(
            {"message": null, "keepIndex": true, "includeUntracked": false, "paths": ["a.txt"]}
        ))
        .unwrap();
        assert_eq!(
            git_core::stash::StashOptions::from(opts),
            git_core::stash::StashOptions {
                message: None,
                keep_index: true,
                include_untracked: false,
                paths: vec!["a.txt".into()]
            }
        );
        assert_eq!(
            serde_json::to_value(StashApplyOutcomeDto::from(
                git_core::stash::StashApplyOutcome::Conflicted {
                    files: vec!["f".into()]
                }
            ))
            .unwrap(),
            serde_json::json!({"kind": "Conflicted", "files": ["f"]})
        );
        assert_eq!(
            serde_json::to_value(StashApplyOutcomeDto::from(
                git_core::stash::StashApplyOutcome::Applied
            ))
            .unwrap(),
            serde_json::json!({"kind": "Applied"})
        );
        assert_eq!(
            serde_json::to_value(StashFilesDto::from(git_core::stash::StashFiles {
                tracked: vec!["t".into()],
                untracked: vec![],
                untracked_commit_id: None
            }))
            .unwrap(),
            serde_json::json!({"tracked": ["t"], "untracked": [], "untrackedCommitId": null})
        );
    }

    #[test]
    fn pick_outcome_dto_is_kind_tagged() {
        assert_eq!(
            serde_json::to_value(PickOutcomeDto::from(
                git_core::pick::PickOutcome::Committed {
                    commit_id: "c1".into()
                }
            ))
            .unwrap(),
            serde_json::json!({"kind": "Committed", "commitId": "c1"})
        );
        assert_eq!(
            serde_json::to_value(PickOutcomeDto::from(
                git_core::pick::PickOutcome::Conflicted {
                    files: vec!["a".into()]
                }
            ))
            .unwrap(),
            serde_json::json!({"kind": "Conflicted", "files": ["a"]})
        );
    }

    #[test]
    fn commit_signature_dto_serializes_for_the_frontend() {
        use git_core::signing::{CommitSignature, SignatureFormat, SignatureStatus};
        let dto = CommitSignatureDto::from(CommitSignature {
            status: SignatureStatus::GoodUntrusted,
            format: Some(SignatureFormat::OpenPgp),
            signer: Some("Jane <j@e>".into()),
            key: Some("ABCD".into()),
            detail: None,
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "status": "goodUntrusted", "format": "openpgp",
                "signer": "Jane <j@e>", "key": "ABCD", "detail": null
            })
        );
        let unsigned = CommitSignatureDto::from(CommitSignature {
            status: SignatureStatus::Unsigned,
            format: None,
            signer: None,
            key: None,
            detail: None,
        });
        assert_eq!(
            serde_json::to_value(unsigned).unwrap()["status"],
            "unsigned"
        );
        for (status, wire) in [
            (SignatureStatus::Good, "good"),
            (SignatureStatus::Bad, "bad"),
            (SignatureStatus::UnknownKey, "unknownKey"),
            (SignatureStatus::Expired, "expired"),
            (SignatureStatus::ExpiredKey, "expiredKey"),
            (SignatureStatus::RevokedKey, "revokedKey"),
            (SignatureStatus::Error, "error"),
        ] {
            let dto = CommitSignatureDto::from(CommitSignature {
                status,
                format: Some(SignatureFormat::Ssh),
                signer: None,
                key: None,
                detail: None,
            });
            let value = serde_json::to_value(dto).unwrap();
            assert_eq!(value["status"], wire);
            assert_eq!(value["format"], "ssh");
        }
    }

    #[test]
    fn undo_dtos_serialize_for_the_frontend() {
        use repo_service::worker::{UndoOutcome, UndoState};
        let state = UndoStateDto::from(UndoState {
            undo: Some("Stage a.txt".into()),
            redo: None,
        });
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::json!({ "undo": "Stage a.txt", "redo": null })
        );
        let cases = [
            (
                UndoOutcome::Done {
                    label: "Stage a.txt".into(),
                },
                serde_json::json!({ "kind": "Done", "label": "Stage a.txt" }),
            ),
            (
                UndoOutcome::Nothing,
                serde_json::json!({ "kind": "Nothing" }),
            ),
            (
                UndoOutcome::RepositoryChanged { label: "x".into() },
                serde_json::json!({ "kind": "RepositoryChanged", "label": "x" }),
            ),
            (
                UndoOutcome::BranchHeld {
                    label: "x".into(),
                    message: "branch 'b' is checked out in another worktree at '/w'".into(),
                },
                serde_json::json!({
                    "kind": "BranchHeld",
                    "label": "x",
                    "message": "branch 'b' is checked out in another worktree at '/w'",
                }),
            ),
            (
                UndoOutcome::OperationInProgress,
                serde_json::json!({ "kind": "OperationInProgress" }),
            ),
        ];
        for (outcome, expected) in cases {
            assert_eq!(
                serde_json::to_value(UndoOutcomeDto::from(outcome)).unwrap(),
                expected
            );
        }
    }

    #[test]
    fn reset_mode_dto_parses_lowercase() {
        let mode: ResetModeDto = serde_json::from_value(serde_json::json!("hard")).unwrap();
        assert_eq!(
            git_core::reset::ResetMode::from(mode),
            git_core::reset::ResetMode::Hard
        );
    }

    #[test]
    fn force_push_and_stale_lease_names_reach_the_wire() {
        let (_, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "force-push-1".into(),
            operation: TransferOperation::ForcePushBranch,
            error: Some(TransferErrorKind::StaleLease),
            hook_output: None,
        });

        assert_eq!(failed.operation, "ForcePushBranch");
        assert_eq!(failed.error_kind.as_deref(), Some("StaleLease"));
    }

    fn expected_transfer_phase_wire_value(phase: TransferPhase) -> &'static str {
        match phase {
            TransferPhase::Receiving => "Receiving",
            TransferPhase::Updating => "Updating",
            TransferPhase::RunningHook => "RunningHook",
        }
    }

    #[test]
    fn transfer_phase_wire_values_match_the_typescript_union() {
        for phase in [
            TransferPhase::Receiving,
            TransferPhase::Updating,
            TransferPhase::RunningHook,
        ] {
            let (_, progress) = transfer_event_payload(TransferEvent::Progress(TransferProgress {
                operation_id: "fetch-42".into(),
                operation: TransferOperation::Fetch,
                phase,
                current: 0,
                total: 0,
                received_bytes: 0,
                message: None,
            }));

            assert_eq!(progress.phase, expected_transfer_phase_wire_value(phase));
        }

        let (_, started) = transfer_event_payload(TransferEvent::Started {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
        });
        let (_, completed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            error: None,
            hook_output: None,
        });
        let (_, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::TransferFailed),
            hook_output: None,
        });
        assert_eq!(
            [started.phase, completed.phase, failed.phase],
            ["Starting", "Completed", "Failed"]
        );
    }

    #[test]
    fn two_open_repos_have_independent_worker_state() {
        use repo_service::worker::Worker;
        use std::collections::HashMap;

        let dir_a = tempfile::TempDir::new().unwrap();
        let repo_a = git2::Repository::init(dir_a.path()).unwrap();
        {
            let mut config = repo_a.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        std::fs::write(dir_a.path().join("a.txt"), "a").unwrap();

        let dir_b = tempfile::TempDir::new().unwrap();
        let repo_b = git2::Repository::init(dir_b.path()).unwrap();
        {
            let mut config = repo_b.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }

        let mut workers: HashMap<String, Worker> = HashMap::new();
        workers.insert(
            dir_a.path().to_string_lossy().into_owned(),
            Worker::spawn(dir_a.path().to_path_buf()).unwrap(),
        );
        workers.insert(
            dir_b.path().to_string_lossy().into_owned(),
            Worker::spawn(dir_b.path().to_path_buf()).unwrap(),
        );

        let handle_a = workers[&dir_a.path().to_string_lossy().into_owned()].handle();
        let handle_b = workers[&dir_b.path().to_string_lossy().into_owned()].handle();

        handle_a.stage_file("a.txt".to_string()).unwrap();

        assert_eq!(handle_a.get_status().unwrap().len(), 1);
        assert!(handle_a.get_status().unwrap()[0].staged);
        assert!(handle_b.get_status().unwrap().is_empty());
    }

    fn head_info_with_rebase(health: git_core::rebase::RebaseHealth) -> serde_json::Value {
        serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
            head_id: None,
            detached_at: None,
            head_pushed: false,
            operation: Some(git_core::repo::OperationKind::CherryPick),
            squash_message: None,
            commit_hooks: vec![],
            sequence: None,
            rebase: Some(git_core::repo::RebaseInfo {
                branch: "feature/x".into(),
                onto: "c0ffee".into(),
                current_step: 1,
                total_steps: 2,
                health,
            }),
            bisect: None,
            terminal_sequence: None,
            terminal_rebase: None,
        }))
        .unwrap()
    }

    #[test]
    fn confirmed_abort_dto_deserializes_camel_case() {
        let dto: super::ConfirmedAbortDto = serde_json::from_value(serde_json::json!({
            "headId": "h",
            "startedAt": "s",
            "rewinds": true,
            "commitsToRemove": 2,
            "pausedOnConflict": true
        }))
        .unwrap();
        assert_eq!(
            git_core::pick::ConfirmedAbort::from(dto),
            git_core::pick::ConfirmedAbort {
                head_id: "h".into(),
                started_at: "s".into(),
                rewinds: true,
                commits_to_remove: 2,
                paused_on_conflict: true,
            }
        );
    }

    #[test]
    fn head_info_dto_serializes_a_terminal_sequence() {
        let head = |terminal_sequence| {
            serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
                head_id: None,
                detached_at: None,
                head_pushed: false,
                operation: None,
                squash_message: None,
                commit_hooks: vec![],
                sequence: None,
                rebase: None,
                bisect: None,
                terminal_sequence,
                terminal_rebase: None,
            }))
            .unwrap()
        };
        let paused = head(Some(git_core::pick::TerminalSequence {
            kind: git_core::pick::SequenceKind::CherryPick,
            paused_on_conflict: false,
            started_at: "abc".into(),
            started_at_summary: "X".into(),
            rewinds: true,
            commits_to_remove: 1,
            damage: None,
        }));
        assert_eq!(
            paused["terminalSequence"],
            serde_json::json!({
                "kind": "CherryPick",
                "pausedOnConflict": false,
                "startedAt": "abc",
                "startedAtSummary": "X",
                "rewinds": true,
                "commitsToRemove": 1
            })
        );
        let none = head(None);
        assert!(none.as_object().unwrap().contains_key("terminalSequence"));
        assert_eq!(none["terminalSequence"], serde_json::Value::Null);
    }

    #[test]
    fn head_info_dto_serializes_a_terminal_rebase() {
        let head = |terminal_rebase| {
            serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
                head_id: None,
                detached_at: None,
                head_pushed: false,
                operation: None,
                squash_message: None,
                commit_hooks: vec![],
                sequence: None,
                rebase: None,
                bisect: None,
                terminal_sequence: None,
                terminal_rebase,
            }))
            .unwrap()
        };
        let rebasing = head(Some(git_core::repo::TerminalRebase {
            kind: git_core::repo::TerminalRebaseKind::Rebase,
            branch: "topic".into(),
            onto: "abc".into(),
            current_step: 2,
            total_steps: 3,
        }));
        assert_eq!(
            rebasing["terminalRebase"],
            serde_json::json!({
                "kind": "Rebase",
                "branch": "topic",
                "onto": "abc",
                "currentStep": 2,
                "totalSteps": 3
            })
        );
        let am = head(Some(git_core::repo::TerminalRebase {
            kind: git_core::repo::TerminalRebaseKind::Am,
            branch: String::new(),
            onto: String::new(),
            current_step: 1,
            total_steps: 2,
        }));
        assert_eq!(am["terminalRebase"]["kind"], "Am");
        let none = head(None);
        assert!(none.as_object().unwrap().contains_key("terminalRebase"));
        assert_eq!(none["terminalRebase"], serde_json::Value::Null);
    }

    #[test]
    fn head_info_dto_serializes_a_bisect() {
        let head = |bisect| {
            serde_json::to_value(HeadInfoDto::from(git_core::repo::HeadInfo {
                head_id: None,
                detached_at: None,
                head_pushed: false,
                operation: None,
                squash_message: None,
                commit_hooks: vec![],
                sequence: None,
                rebase: None,
                bisect,
                terminal_sequence: None,
                terminal_rebase: None,
            }))
            .unwrap()
        };
        let bisecting = head(Some(git_core::repo::BisectInfo {
            started_from: "main".into(),
        }));
        assert_eq!(
            bisecting["bisect"],
            serde_json::json!({ "startedFrom": "main" })
        );
        let none = head(None);
        assert!(none.as_object().unwrap().contains_key("bisect"));
        assert_eq!(none["bisect"], serde_json::Value::Null);
    }

    #[test]
    fn head_info_dto_serializes_a_paused_rebase() {
        let json = head_info_with_rebase(git_core::rebase::RebaseHealth::Paused);
        assert_eq!(
            json["rebase"],
            serde_json::json!({
                "branch": "feature/x",
                "onto": "c0ffee",
                "currentStep": 1,
                "totalSteps": 2,
                "health": { "kind": "Paused" },
            })
        );
    }

    #[test]
    fn head_info_dto_serializes_a_diverged_rebase_reason() {
        use git_core::rebase::{RebaseDivergence, RebaseHealth};
        for (reason, name) in [
            (RebaseDivergence::HeadMoved, "HeadMoved"),
            (RebaseDivergence::BranchMoved, "BranchMoved"),
            (RebaseDivergence::BranchMissing, "BranchMissing"),
            (RebaseDivergence::OtherOperation, "OtherOperation"),
        ] {
            let json = head_info_with_rebase(RebaseHealth::Diverged(reason));
            assert_eq!(
                json["rebase"]["health"],
                serde_json::json!({ "kind": "Diverged", "reason": name })
            );
        }
    }

    #[test]
    fn rebase_abort_dto_serializes_restored() {
        assert_eq!(
            serde_json::to_value(super::RebaseAbortDto::from(git_core::rebase::RebaseAbort {
                restored: false
            }))
            .unwrap(),
            serde_json::json!({ "restored": false })
        );
    }
}

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
