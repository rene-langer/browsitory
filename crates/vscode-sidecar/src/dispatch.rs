//! Routes the sidecar's JSON-RPC methods to their handlers (`methods!` below). The handlers
//! live in one file per feature under `dispatch/`, mirroring `crates/tauri-app/src/commands/`;
//! params types shared by several features and the hook relay stay here.

use repo_service::wire::ErrorDto;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use repo_service::wire::{hook_progress_dto, SignatureInputDto, TrailerDto};
use repo_service::worker::{HookEvent, WorkerRegistry};
use serde::Deserialize;
use serde_json::Value;

mod branch;
mod compare;
mod forge;
mod graph;
mod history;
mod merge;
mod patch;
mod rebase;
mod reflog;
mod remote;
mod repos;
mod search;
mod settings;
mod stash;
mod status;
mod submodule;
mod tag;
mod tools;
mod tree;
mod undo;
mod watch;
mod worktree;

use branch::*;
use compare::*;
use forge::*;
use graph::*;
use history::*;
use merge::*;
use patch::*;
use rebase::*;
use reflog::*;
use remote::*;
use repos::*;
use search::*;
use settings::*;
use stash::*;
use status::*;
use submodule::*;
use tag::*;
use tools::*;
use tree::*;
use undo::*;
use watch::*;
use worktree::*;

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
///
/// An arm names its handler's params type first, as in `get_status(RepoPathParams, repos)`:
/// `call_handler!` reads the request's `params` into that type before calling the handler. A
/// handler written with no arguments, as in `list_recent_repos()`, ignores the request's
/// `params`, whatever they are.
macro_rules! methods {
    (|$params:ident, $repos:ident, $stdout:ident| {
        $($name:literal => $handler:ident($($args:tt)*)),* $(,)?
    }) => {
        pub fn dispatch(
            method: &str,
            $params: Value,
            $repos: &Repos,
            $stdout: &Arc<Mutex<std::io::Stdout>>,
        ) -> Result<Value, ErrorDto> {
            test_delay_before_dispatch(method);
            match method {
                $($name => call_handler!($params, $handler($($args)*)),)*
                other => Err(format!("unknown method: {other}").into()),
            }
        }

        #[cfg(test)]
        const METHODS: &[&str] = &[$($name),*];
    };
}

/// One `methods!` arm. The params are read with `serde_json::from_value`, and a failure answers
/// with its `to_string()`. That text is the wire error (pinned by
/// `tests/wire/dispatch-responses.txt` and `tests/params_errors.rs`). It names the params struct
/// (`invalid type: null, expected struct RepoPathParams`), so renaming one changes the wire.
macro_rules! call_handler {
    ($params:ident, $handler:ident()) => {
        $handler()
    };
    ($params:ident, $handler:ident($params_type:ty $(, $arg:ident)*)) => {
        $handler(
            serde_json::from_value::<$params_type>($params).map_err(ErrorDto::from_error)?
            $(, $arg)*
        )
    };
}

methods!(|params, repos, stdout| {
    "open_repo" => open_repo(OpenRepoParams, repos),
    "close_repo" => close_repo(RepoPathParams, repos),
    "watch_repo" => watch_repo(RepoPathParams, stdout),
    "unwatch_repo" => unwatch_repo(RepoPathParams),
    "get_status" => get_status(RepoPathParams, repos),
    "get_change_stamp" => get_change_stamp(RepoPathParams, repos),
    "get_status_with_stamp" => get_status_with_stamp(RepoPathParams, repos),
    "list_untracked_folder" => list_untracked_folder(RepoFilePathParams, repos),
    "get_commit_graph" => get_commit_graph(GetCommitGraphParams, repos),
    "locate_commit" => locate_commit(LocateCommitParams, repos),
    "get_merged_commits" => get_merged_commits(GetMergedCommitsParams, repos),
    "get_graph_collapse_merges" => get_graph_collapse_merges(RepoPathParams),
    "set_graph_collapse_merges" => set_graph_collapse_merges(SetGraphCollapseMergesParams),
    "get_working_diff" => get_working_diff(GetWorkingDiffParams, repos),
    "get_commit_diff" => get_commit_diff(GetCommitDiffParams, repos),
    "get_file_history" => get_file_history(GetFileHistoryParams, repos),
    "get_line_history" => get_line_history(GetLineHistoryParams, repos),
    "search_commits" => search_commits(SearchCommitsParams, repos),
    "cancel_search" => cancel_search(RepoPathParams, repos),
    "get_commit_rename_diff" => get_commit_rename_diff(GetCommitRenameDiffParams, repos),
    "get_range_files" => get_range_files(GetRangeFilesParams, repos),
    "get_range_diff" => get_range_diff(GetRangeDiffParams, repos),
    "get_working_tree_compare_files" => get_working_tree_compare_files(GetCommitFilesParams, repos),
    "get_working_tree_compare_diff" => get_working_tree_compare_diff(
        GetWorkingTreeCompareDiffParams, repos
    ),
    "get_branch_compare_range" => get_branch_compare_range(GetBranchCompareRangeParams, repos),
    "list_recent_repos" => list_recent_repos(),
    "remove_recent_repo" => remove_recent_repo(RemoveRecentRepoParams),
    "list_missing_recent_repos" => list_missing_recent_repos(),
    "list_pinned_repos" => list_pinned_repos(),
    "pin_recent_repo" => pin_recent_repo(PinRecentRepoParams),
    "unpin_recent_repo" => unpin_recent_repo(PinRecentRepoParams),
    "list_open_repos" => list_open_repos_handler(),
    "persist_open_repos" => persist_open_repos(PersistOpenReposParams),
    "list_repo_aliases" => list_repo_aliases(),
    "set_repo_alias" => set_repo_alias(SetRepoAliasParams),
    "scan_repos_in_root" => scan_repos_in_root(ScanReposInRootParams),
    "list_workspaces" => list_workspaces_handler(),
    "save_workspace" => save_workspace(SaveWorkspaceParams),
    "update_workspace" => update_workspace(UpdateWorkspaceParams),
    "delete_workspace" => delete_workspace(DeleteWorkspaceParams),
    "get_graph_branch_selection" => get_graph_branch_selection(RepoPathParams),
    "set_graph_branch_selection" => set_graph_branch_selection(SetGraphBranchSelectionParams),
    "clear_graph_branch_selection" => clear_graph_branch_selection(RepoPathParams),
    "get_diff_settings" => get_diff_settings(),
    "set_diff_settings" => set_diff_settings(SetDiffSettingsParams),
    "get_external_tool_settings" => get_external_tool_settings(),
    "set_external_tool_settings" => set_external_tool_settings(SetExternalToolSettingsParams),
    "get_git_tools" => get_git_tools(RepoPathParams, repos),
    "get_file_sides" => get_file_sides(FileSidesParams, repos),
    "get_commit_tree" => get_commit_tree(GetCommitTreeParams, repos),
    "get_file_at_commit" => get_file_at_commit(CommitFileParams, repos),
    "restore_file_version" => restore_file_version(CommitFileParams, repos),
    "write_file_version" => write_file_version(WriteFileVersionParams, repos),
    "list_recent_authors" => list_recent_authors(RepoPathParams, repos),
    "get_commit_message_settings" => get_commit_message_settings(),
    "set_commit_message_settings" => set_commit_message_settings(SetCommitMessageSettingsParams),
    "get_fetch_settings" => get_fetch_settings(),
    "set_fetch_settings" => set_fetch_settings(SetFetchSettingsParams),
    "get_appearance_settings" => get_appearance_settings(),
    "set_appearance_settings" => set_appearance_settings(SetAppearanceSettingsParams),
    "list_font_families" => list_font_families(),
    "get_update_settings" => get_update_settings(),
    "set_update_settings" => set_update_settings(SetUpdateSettingsParams),
    "mark_resolved" => mark_resolved(MarkResolvedParams, repos),
    "resolve_whole_file" => resolve_whole_file(ResolveWholeFileParams, repos),
    "read_file_version" => read_file_version(ReadFileVersionParams, repos),
    "repo_workdir" => repo_workdir(RepoPathParams, repos),
    "check_merge_target" => check_merge_target(RepoFileParams, repos),
    "get_root_files" => get_root_files(RepoPathParams, repos),
    "create_root_file" => create_root_file(CreateRootFileParams, repos),
    "get_graph_remote_branch_selection" => get_graph_remote_branch_selection(RepoPathParams),
    "set_graph_remote_branch_selection" => set_graph_remote_branch_selection(
        SetGraphRemoteBranchSelectionParams
    ),
    "clear_graph_remote_branch_selection" => clear_graph_remote_branch_selection(RepoPathParams),
    "get_commit_files" => get_commit_files(GetCommitFilesParams, repos),
    "get_commit_stats" => get_commit_stats(GetCommitStatsParams, repos),
    "get_commit_message" => get_commit_message(GetCommitFilesParams, repos),
    "get_commit_signature" => get_commit_signature(GetCommitFilesParams, repos),
    "get_commit_details" => get_commit_details(GetCommitFilesParams, repos),
    "stage_file" => stage_file(RepoFilePathParams, repos),
    "unstage_file" => unstage_file(RepoFilePathParams, repos),
    "stage_paths" => stage_paths(RepoPathsParams, repos),
    "unstage_paths" => unstage_paths(RepoPathsParams, repos),
    "stage_hunk" => stage_hunk(HunkParams, repos),
    "unstage_hunk" => unstage_hunk(HunkParams, repos),
    "discard_hunk" => discard_hunk(HunkParams, repos),
    "stage_lines" => stage_lines(LinesParams, repos),
    "unstage_lines" => unstage_lines(LinesParams, repos),
    "discard_lines" => discard_lines(LinesParams, repos),
    "discard_file" => discard_file(RepoFilePathParams, repos),
    "discard_all_unstaged" => discard_all_unstaged(DiscardAllParams, repos),
    "apply_commit_change" => apply_commit_change(ApplyCommitChangeParams, repos),
    "get_patch" => get_patch(GetPatchParams, repos),
    "apply_patch" => apply_patch(ApplyPatchParams, repos, stdout),
    "preview_patch" => preview_patch(PreviewPatchParams, repos),
    "ignore_path" => ignore_path(IgnorePathParams, repos),
    "ignore_paths" => ignore_paths(IgnorePathsParams, repos),
    "commit" => commit(CommitMessageParams, repos, stdout),
    "set_identity" => set_identity(SetIdentityParams, repos),
    "list_branches" => list_branches(RepoPathParams, repos),
    "create_branch" => create_branch(CreateBranchParams, repos),
    "switch_branch" => switch_branch(SwitchBranchParams, repos, stdout),
    "get_switch_conflicts" => get_switch_conflicts(SwitchBranchParams, repos),
    "stash_and_switch" => stash_and_switch(StashAndSwitchParams, repos, stdout),
    "delete_branch" => delete_branch(DeleteBranchParams, repos),
    "rename_branch" => rename_branch(RenameBranchParams, repos),
    "list_worktrees" => list_worktrees(RepoPathParams, repos),
    "create_worktree" => create_worktree(CreateWorktreeParams, repos),
    "remove_worktree" => remove_worktree(WorktreeNameParams, repos),
    "prune_worktrees" => prune_worktrees(RepoPathParams, repos),
    "list_submodules" => list_submodules(RepoPathParams, repos),
    "init_submodule" => init_submodule(RepoFilePathParams, repos),
    "update_submodule" => update_submodule(UpdateSubmoduleParams, repos),
    "list_reflog_refs" => list_reflog_refs(RepoPathParams, repos),
    "get_reflog" => get_reflog(GetReflogParams, repos),
    "restore_reflog_entry" => restore_reflog_entry(RestoreReflogEntryParams, repos),
    "list_remotes" => list_remotes(RepoPathParams, repos),
    "list_remote_branches" => list_remote_branches(RemoteNameParams, repos),
    "get_current_upstream" => get_current_upstream(RepoPathParams, repos),
    "get_remote_upstreams" => get_remote_upstreams(GetRemoteUpstreamsParams, repos),
    "add_remote" => add_remote(AddRemoteParams, repos),
    "rename_remote" => rename_remote(RenameRemoteParams, repos),
    "update_remote_urls" => update_remote_urls(UpdateRemoteUrlsParams, repos),
    "remove_remote" => remove_remote(RemoveRemoteParams, repos),
    "save_https_credential" => save_https_credential(SaveHttpsCredentialParams, repos),
    "forget_https_credential" => forget_https_credential(RemoteNameParams, repos),
    "set_remote_auth_mode" => set_remote_auth_mode(SetRemoteAuthModeParams, repos),
    "set_current_upstream" => set_current_upstream(SetCurrentUpstreamParams, repos),
    "clear_current_upstream" => clear_current_upstream(RepoPathParams, repos),
    "list_tags" => list_tags(RepoPathParams, repos),
    "create_tag" => create_tag(CreateTagParams, repos),
    "delete_tag" => delete_tag(DeleteTagParams, repos),
    "fetch_remote" => fetch_remote(FetchRemoteParams, repos, stdout),
    "cancel_transfer" => cancel_transfer(CancelTransferParams, repos),
    "push_current_branch" => push_current_branch(PushParams, repos, stdout),
    "force_push_current_branch" => force_push_current_branch(PushParams, repos, stdout),
    "get_head_info" => get_head_info(RepoPathParams, repos),
    "amend_commit" => amend_commit(CommitMessageParams, repos, stdout),
    "get_undo_state" => get_undo_state(RepoPathParams, repos),
    "undo_last" => undo_last(RepoPathParams, repos),
    "redo_last" => redo_last(RepoPathParams, repos),
    "cherry_pick" => cherry_pick(PickParams, repos, stdout),
    "revert_commit" => revert_commit(PickParams, repos, stdout),
    "cherry_pick_many" => cherry_pick_many(CommitIdsParams, repos, stdout),
    "revert_many" => revert_many(CommitIdsParams, repos, stdout),
    "continue_sequence" => continue_sequence(ContinueSequenceParams, repos, stdout),
    "abort_sequence" => abort_sequence(AbortSequenceParams, repos),
    "reset_to_commit" => reset_to_commit(ResetParams, repos),
    "checkout_commit" => checkout_commit(CommitIdParams, repos, stdout),
    "push_tags" => push_tags(PushTagsParams, repos, stdout),
    "fetch_all_remotes" => fetch_all_remotes(FetchAllRemotesParams, repos, stdout),
    "background_fetch" => background_fetch(BackgroundFetchParams, repos, stdout),
    "push_branch" => push_branch(PushBranchParams, repos, stdout),
    "delete_remote_branch" => delete_remote_branch(DeleteRemoteBranchParams, repos, stdout),
    "delete_remote_tag" => delete_remote_tag(DeleteRemoteTagParams, repos, stdout),
    "pull_current_upstream" => pull_current_upstream(RepoPathParams, repos, stdout),
    "get_pull_strategy" => get_pull_strategy(RepoPathParams, repos),
    "set_pull_strategy" => set_pull_strategy(SetPullStrategyParams),
    "list_stashes" => list_stashes(RepoPathParams, repos),
    "save_stash" => save_stash(SaveStashParams, repos),
    "apply_stash" => apply_stash(StashApplyParams, repos),
    "pop_stash" => pop_stash(StashApplyParams, repos),
    "get_stash_files" => get_stash_files(StashFilesParams, repos),
    "drop_stash" => drop_stash(StashIndexParams, repos),
    "get_blame" => get_blame(GetBlameParams, repos),
    "start_merge" => start_merge(StartMergeParams, repos, stdout),
    "get_merge_options" => get_merge_options(RepoPathParams, repos),
    "set_merge_options" => set_merge_options(SetMergeOptionsParams),
    "get_conflict_hunks" => get_conflict_hunks(RepoFilePathParams, repos),
    "resolve_conflict" => resolve_conflict(ResolveConflictParams, repos),
    "abort_merge" => abort_merge(RepoPathParams, repos),
    "get_merge_message" => get_merge_message(RepoPathParams, repos),
    "resolve_add_delete_conflict" => resolve_add_delete_conflict(
        ResolveAddDeleteConflictParams, repos
    ),
    "commits_since" => commits_since(CommitsSinceParams, repos),
    "start_rebase" => start_rebase(StartRebaseParams, repos, stdout),
    "rebase_continue" => rebase_continue(RebaseContinueParams, repos, stdout),
    "fold_staged_into" => fold_staged_into(FoldStagedParams, repos, stdout),
    "abort_rebase" => abort_rebase(RepoPathParams, repos),
    "detect_forge_repository" => detect_forge_repository(RepoPathParams, repos),
    "save_forge_token" => save_forge_token(ForgeTokenParams, repos),
    "forget_forge_token" => forget_forge_token(ForgetForgeTokenParams, repos),
    "list_pull_requests" => list_pull_requests(ListPullRequestsParams, repos),
    "create_pull_request" => create_pull_request(CreatePullRequestParams, repos),
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
#[serde(rename_all = "camelCase")]
struct RepoPathParams {
    repo_path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitFilesParams {
    repo_path: String,
    commit_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoFilePathParams {
    repo_path: String,
    path: String,
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

#[cfg(test)]
mod wire_snapshots;

#[cfg(test)]
mod tests;
