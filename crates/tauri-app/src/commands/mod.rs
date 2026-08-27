use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::thread;

use git_core::diff::DiffHunk;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::worker::{TransferEvent, Worker};

mod branch;
mod forge;
mod merge;
mod rebase;
mod reflog;
mod remote;
mod stash;
mod status;
mod submodule;
mod tag;
mod worktree;

pub use branch::{create_branch, delete_branch, list_branches, rename_branch, switch_branch};
pub use forge::{
    create_pull_request, detect_forge_repository, forget_forge_token, list_pull_requests,
    save_forge_token,
};
pub use merge::{
    abort_merge, get_conflict_hunks, get_merge_message, resolve_add_delete_conflict,
    resolve_conflict, start_merge,
};
pub use rebase::{abort_rebase, commits_since, get_rebase_progress, rebase_continue, start_rebase};
pub use reflog::{get_reflog, list_reflog_refs, restore_reflog_entry};
pub use remote::*;
pub use stash::{apply_stash, drop_stash, list_stashes, save_stash};
pub use status::{
    commit, discard_hunk, get_blame, get_commit_diff, get_commit_files, get_commit_graph,
    get_graph_branch_selection, get_status, get_working_diff, set_graph_branch_selection,
    stage_file, stage_hunk, unstage_file, unstage_hunk,
};
pub use submodule::{init_submodule, list_submodules, update_submodule};
pub use tag::{create_tag, delete_tag, list_tags};
pub use worktree::{create_worktree, list_worktrees, prune_worktrees, remove_worktree};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgressDto {
    pub operation_id: String,
    pub operation: String,
    pub phase: String,
    pub error_kind: Option<String>,
    pub current: usize,
    pub total: usize,
    pub received_bytes: usize,
    pub message: Option<String>,
}

impl From<git_core::remote::TransferProgress> for TransferProgressDto {
    fn from(progress: git_core::remote::TransferProgress) -> Self {
        Self {
            operation_id: progress.operation_id,
            operation: format!("{:?}", progress.operation),
            phase: format!("{:?}", progress.phase),
            error_kind: None,
            current: progress.current,
            total: progress.total,
            received_bytes: progress.received_bytes,
            // Sideband and reference-update text comes from the remote. It is not safe to
            // expose over IPC, even when it looks like ordinary progress output.
            message: None,
        }
    }
}

fn transfer_event_payload(event: TransferEvent) -> (&'static str, TransferProgressDto) {
    match event {
        TransferEvent::Started {
            operation_id,
            operation,
        } => (
            "transfer-progress",
            TransferProgressDto {
                operation_id,
                operation: format!("{operation:?}"),
                phase: "Starting".to_string(),
                error_kind: None,
                current: 0,
                total: 0,
                received_bytes: 0,
                message: None,
            },
        ),
        TransferEvent::Progress(progress) => {
            ("transfer-progress", TransferProgressDto::from(progress))
        }
        TransferEvent::Completed {
            operation_id,
            operation,
            error,
        } => {
            let failed = error.is_some();
            (
                "transfer-complete",
                TransferProgressDto {
                    operation_id,
                    operation: format!("{operation:?}"),
                    phase: if failed { "Failed" } else { "Completed" }.to_string(),
                    error_kind: error.map(|kind| format!("{kind:?}")),
                    current: 0,
                    total: 0,
                    received_bytes: 0,
                    message: None,
                },
            )
        }
    }
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

#[derive(Serialize)]
pub struct StatusEntryDto {
    pub path: String,
    pub staged: bool,
    pub kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfoDto {
    pub name: String,
    pub is_current: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfoDto {
    pub name: String,
    pub path: String,
    pub head: Option<String>,
    pub is_main: bool,
    pub is_locked: bool,
    pub is_prunable: bool,
}

impl From<git_core::worktree::WorktreeInfo> for WorktreeInfoDto {
    fn from(worktree: git_core::worktree::WorktreeInfo) -> Self {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDto {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub member_paths: Vec<String>,
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
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRepoEntryDto {
    pub path: String,
    pub workspace_id: Option<String>,
}

impl From<config::OpenRepoEntry> for OpenRepoEntryDto {
    fn from(entry: config::OpenRepoEntry) -> Self {
        Self {
            path: entry.path.to_string_lossy().into_owned(),
            workspace_id: entry.workspace_id,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenRepoEntryInput {
    pub path: String,
    pub workspace_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmoduleInfoDto {
    pub path: String,
    pub url: Option<String>,
    pub gitlink_id: Option<String>,
    pub initialized: bool,
    pub head_id: Option<String>,
}

impl From<git_core::submodule::SubmoduleInfo> for SubmoduleInfoDto {
    fn from(submodule: git_core::submodule::SubmoduleInfo) -> Self {
        Self {
            path: submodule.path,
            url: submodule.url,
            gitlink_id: submodule.gitlink_id,
            initialized: submodule.initialized,
            head_id: submodule.head_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflogEntryDto {
    pub reference: String,
    pub old_id: String,
    pub new_id: String,
    pub committer_name: String,
    pub committer_email: String,
    pub timestamp: i64,
    pub message: String,
    pub summary: Option<String>,
}

impl From<git_core::reflog::ReflogEntry> for ReflogEntryDto {
    fn from(entry: git_core::reflog::ReflogEntry) -> Self {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfoDto {
    pub name: String,
    pub fetch_url: String,
    pub push_url: Option<String>,
    pub auth_mode: Option<RemoteAuthModeDto>,
    pub auth_username: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub enum RemoteAuthModeDto {
    HttpsToken,
    SshAgent,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagInfoDto {
    pub name: String,
    pub target_id: String,
    pub annotated: bool,
    pub message: Option<String>,
    pub tagger_name: Option<String>,
    pub timestamp: Option<i64>,
}

impl From<git_core::remote::TagInfo> for TagInfoDto {
    fn from(tag: git_core::remote::TagInfo) -> Self {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamInfoDto {
    pub local_branch: String,
    pub remote_name: String,
    pub remote_branch: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
pub enum PullOutcomeDto {
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

impl From<git_core::remote::UpstreamInfo> for UpstreamInfoDto {
    fn from(upstream: git_core::remote::UpstreamInfo) -> Self {
        Self {
            local_branch: upstream.local_branch,
            remote_name: upstream.remote_name,
            remote_branch: upstream.remote_branch,
        }
    }
}

impl From<git_core::branch::BranchInfo> for BranchInfoDto {
    fn from(b: git_core::branch::BranchInfo) -> Self {
        BranchInfoDto {
            name: b.name,
            is_current: b.is_current,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntryDto {
    pub index: usize,
    pub message: String,
    pub commit_id: String,
}

impl From<git_core::stash::StashEntry> for StashEntryDto {
    fn from(s: git_core::stash::StashEntry) -> Self {
        StashEntryDto {
            index: s.index,
            message: s.message,
            commit_id: s.commit_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameLineDto {
    pub line_number: usize,
    pub content: String,
    pub commit_id: String,
    pub short_id: String,
    pub author_name: String,
    pub timestamp: i64,
}

impl From<git_core::blame::BlameLine> for BlameLineDto {
    fn from(l: git_core::blame::BlameLine) -> Self {
        BlameLineDto {
            line_number: l.line_number,
            content: l.content,
            commit_id: l.commit_id,
            short_id: l.short_id,
            author_name: l.author_name,
            timestamp: l.timestamp,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphCommitDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parent_ids: Vec<String>,
    pub branch_refs: Vec<String>,
}

impl From<git_core::graph::GraphCommit> for GraphCommitDto {
    fn from(c: git_core::graph::GraphCommit) -> Self {
        GraphCommitDto {
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

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum MergeOutcomeDto {
    UpToDate,
    FastForwarded,
    Merged,
    Conflicted { files: Vec<String> },
}

impl From<git_core::merge::MergeOutcome> for MergeOutcomeDto {
    fn from(outcome: git_core::merge::MergeOutcome) -> Self {
        match outcome {
            git_core::merge::MergeOutcome::UpToDate => MergeOutcomeDto::UpToDate,
            git_core::merge::MergeOutcome::FastForwarded => MergeOutcomeDto::FastForwarded,
            git_core::merge::MergeOutcome::Merged => MergeOutcomeDto::Merged,
            git_core::merge::MergeOutcome::Conflicted { files } => {
                MergeOutcomeDto::Conflicted { files }
            }
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum ConflictSegmentDto {
    Clean { content: String },
    Conflict { ours: String, theirs: String },
}

impl From<git_core::merge::ConflictSegment> for ConflictSegmentDto {
    fn from(segment: git_core::merge::ConflictSegment) -> Self {
        match segment {
            git_core::merge::ConflictSegment::Clean { content } => {
                ConflictSegmentDto::Clean { content }
            }
            git_core::merge::ConflictSegment::Conflict { ours, theirs } => {
                ConflictSegmentDto::Conflict { ours, theirs }
            }
        }
    }
}

#[derive(Deserialize)]
pub enum FileConflictChoiceDto {
    Ours,
    Theirs,
    Delete,
}

impl From<FileConflictChoiceDto> for git_core::merge::FileConflictChoice {
    fn from(dto: FileConflictChoiceDto) -> Self {
        match dto {
            FileConflictChoiceDto::Ours => git_core::merge::FileConflictChoice::Ours,
            FileConflictChoiceDto::Theirs => git_core::merge::FileConflictChoice::Theirs,
            FileConflictChoiceDto::Delete => git_core::merge::FileConflictChoice::Delete,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlanCommitDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub timestamp: i64,
}

impl From<git_core::rebase::RebasePlanCommit> for RebasePlanCommitDto {
    fn from(c: git_core::rebase::RebasePlanCommit) -> Self {
        RebasePlanCommitDto {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            timestamp: c.timestamp,
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
pub enum RebaseActionDto {
    Pick,
    Reword { message: String },
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl From<RebaseActionDto> for git_core::rebase::RebaseAction {
    fn from(dto: RebaseActionDto) -> Self {
        match dto {
            RebaseActionDto::Pick => git_core::rebase::RebaseAction::Pick,
            RebaseActionDto::Reword { message } => {
                git_core::rebase::RebaseAction::Reword { message }
            }
            RebaseActionDto::Edit => git_core::rebase::RebaseAction::Edit,
            RebaseActionDto::Squash => git_core::rebase::RebaseAction::Squash,
            RebaseActionDto::Fixup => git_core::rebase::RebaseAction::Fixup,
            RebaseActionDto::Drop => git_core::rebase::RebaseAction::Drop,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebasePlanEntryDto {
    pub commit_id: String,
    pub action: RebaseActionDto,
    pub combined_message: Option<String>,
}

impl From<RebasePlanEntryDto> for git_core::rebase::RebasePlanEntry {
    fn from(dto: RebasePlanEntryDto) -> Self {
        git_core::rebase::RebasePlanEntry {
            commit_id: dto.commit_id,
            action: dto.action.into(),
            combined_message: dto.combined_message,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum RebaseStepResultDto {
    Conflicted { files: Vec<String> },
    PausedForEdit,
    Advanced,
    Done,
}

impl From<git_core::rebase::RebaseStepResult> for RebaseStepResultDto {
    fn from(result: git_core::rebase::RebaseStepResult) -> Self {
        match result {
            git_core::rebase::RebaseStepResult::Conflicted { files } => {
                RebaseStepResultDto::Conflicted { files }
            }
            git_core::rebase::RebaseStepResult::PausedForEdit => RebaseStepResultDto::PausedForEdit,
            git_core::rebase::RebaseStepResult::Advanced => RebaseStepResultDto::Advanced,
            git_core::rebase::RebaseStepResult::Done => RebaseStepResultDto::Done,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseProgressDto {
    pub current_step: usize,
    pub total_steps: usize,
}

#[derive(Serialize)]
pub struct DiffLineDto {
    pub origin: String,
    pub content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunkDto {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLineDto>,
}

impl From<DiffHunk> for DiffHunkDto {
    fn from(h: DiffHunk) -> Self {
        DiffHunkDto {
            old_start: h.old_start,
            old_lines: h.old_lines,
            new_start: h.new_start,
            new_lines: h.new_lines,
            lines: h
                .lines
                .into_iter()
                .map(|l| DiffLineDto {
                    origin: format!("{:?}", l.origin),
                    content: l.content,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ForgeProviderDto {
    GitHub,
    Bitbucket,
}

impl From<git_core::forge::ForgeProvider> for ForgeProviderDto {
    fn from(provider: git_core::forge::ForgeProvider) -> Self {
        match provider {
            git_core::forge::ForgeProvider::GitHub => ForgeProviderDto::GitHub,
            git_core::forge::ForgeProvider::Bitbucket => ForgeProviderDto::Bitbucket,
        }
    }
}

impl From<ForgeProviderDto> for git_core::forge::ForgeProvider {
    fn from(provider: ForgeProviderDto) -> Self {
        match provider {
            ForgeProviderDto::GitHub => git_core::forge::ForgeProvider::GitHub,
            ForgeProviderDto::Bitbucket => git_core::forge::ForgeProvider::Bitbucket,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeRepositoryDto {
    pub provider: ForgeProviderDto,
    pub host: String,
    pub owner: String,
    pub name: String,
    pub remote_name: String,
}

impl From<git_core::forge::ForgeRepository> for ForgeRepositoryDto {
    fn from(repository: git_core::forge::ForgeRepository) -> Self {
        Self {
            provider: repository.provider.into(),
            host: repository.host,
            owner: repository.owner,
            name: repository.name,
            remote_name: repository.remote_name,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestDto {
    pub id: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: String,
    pub source_branch: String,
    pub target_branch: String,
    pub state: String,
}

impl From<crate::pull_requests::PullRequest> for PullRequestDto {
    fn from(pull_request: crate::pull_requests::PullRequest) -> Self {
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

/// A page of listed pull requests, plus whether the provider indicated more exist beyond this
/// page (see `crate::pull_requests::PullRequestList`) — the frontend uses `truncated` to show
/// an explicit "more available" notice instead of silently displaying a partial list.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestListDto {
    pub pull_requests: Vec<PullRequestDto>,
    pub truncated: bool,
}

impl From<crate::pull_requests::PullRequestList> for PullRequestListDto {
    fn from(list: crate::pull_requests::PullRequestList) -> Self {
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

/// The fields a caller supplies to open a new pull request. Never carries a token — see
/// `crate::pull_requests::CreatePullRequest`'s doc comment, which this DTO mirrors field-for-
/// field.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePullRequestDto {
    pub title: String,
    pub description: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
}

impl From<CreatePullRequestDto> for crate::pull_requests::CreatePullRequest {
    fn from(dto: CreatePullRequestDto) -> Self {
        Self {
            title: dto.title,
            description: dto.description,
            source_branch: dto.source_branch,
            target_branch: dto.target_branch,
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub workers: Mutex<HashMap<String, Worker>>,
}

#[tauri::command]
pub async fn open_repo(path: String, state: State<'_, AppState>) -> Result<(), String> {
    {
        let guard = state.workers.lock().unwrap_or_else(|e| e.into_inner());
        if guard.contains_key(&path) {
            drop(guard);
            let _ = config::add_recent_repo(Path::new(&path));
            return Ok(());
        }
    }
    let worker = Worker::spawn(PathBuf::from(&path))?;
    state
        .workers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(path.clone(), worker);
    // Best-effort: a repo that opened successfully should count as "recent" even if we can't
    // persist that fact (e.g. an unwritable config dir) — don't fail the whole open_repo call
    // over it.
    let _ = config::add_recent_repo(Path::new(&path));
    Ok(())
}

#[tauri::command]
pub async fn close_repo(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    state
        .workers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&repo_path);
    Ok(())
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

// Clone the worker's channel handle and drop the guard before blocking on the reply —
// holding the mutex across the round-trip would serialize every command behind this one
// and let a wedged worker hold the lock forever.
fn worker_handle(
    state: &State<AppState>,
    repo_path: &str,
) -> Result<crate::worker::WorkerHandle, String> {
    let guard = state.workers.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(repo_path)
        .map(Worker::handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))
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
    use std::path::PathBuf;

    use config::{OpenRepoEntry, Workspace};
    use git_core::diff::DiffLineOrigin;
    use git_core::reflog::ReflogEntry;
    use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};
    use git_core::status::StatusKind;
    use git_core::submodule::SubmoduleInfo;
    use git_core::worktree::WorktreeInfo;

    use crate::worker::TransferEvent;

    use super::{
        transfer_event_payload, ForgeProviderDto, OpenRepoEntryDto, OpenRepoEntryInput,
        PullOutcomeDto, ReflogEntryDto, RemoteAuthModeDto, SubmoduleInfoDto, WorkspaceDto,
        WorktreeInfoDto,
    };

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
        });

        assert_eq!(event_name, "transfer-complete");
        assert_eq!(failed.phase, "Failed");
        assert_eq!(failed.operation, "Fetch");
        assert_eq!(failed.error_kind.as_deref(), Some("TransferFailed"));
        assert_eq!(failed.message, None);
    }

    #[test]
    fn missing_credential_failure_is_emitted_as_a_safe_terminal_kind() {
        let (_, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::MissingCredential),
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
        });

        assert_eq!(failed.operation, "PushBranch");
        assert_eq!(failed.error_kind.as_deref(), Some("NonFastForward"));
        assert_eq!(failed.message, None);
    }

    fn expected_transfer_phase_wire_value(phase: TransferPhase) -> &'static str {
        match phase {
            TransferPhase::Receiving => "Receiving",
            TransferPhase::Updating => "Updating",
        }
    }

    #[test]
    fn transfer_phase_wire_values_match_the_typescript_union() {
        for phase in [TransferPhase::Receiving, TransferPhase::Updating] {
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
        });
        let (_, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "fetch-42".into(),
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::TransferFailed),
        });
        assert_eq!(
            [started.phase, completed.phase, failed.phase],
            ["Starting", "Completed", "Failed"]
        );
    }

    /// The `kind` field of `StatusEntryDto` is produced by `format!("{:?}", kind)`, so the
    /// `Debug` output *is* the wire format. Its counterpart contract is the `StatusKind`
    /// union in `frontend/src/ipc/RepoClient.ts`
    /// (`"New" | "Modified" | "Deleted" | "Renamed" | "TypeChange"`) — these must stay in
    /// sync. The match below is exhaustive on purpose: adding a `StatusKind` variant breaks
    /// compilation here, which is the reminder to extend the TypeScript union too.
    fn expected_wire_value(kind: StatusKind) -> &'static str {
        match kind {
            StatusKind::New => "New",
            StatusKind::Modified => "Modified",
            StatusKind::Deleted => "Deleted",
            StatusKind::Renamed => "Renamed",
            StatusKind::TypeChange => "TypeChange",
            StatusKind::Conflicted => "Conflicted",
        }
    }

    #[test]
    fn status_kind_wire_values_match_the_typescript_union() {
        for kind in [
            StatusKind::New,
            StatusKind::Modified,
            StatusKind::Deleted,
            StatusKind::Renamed,
            StatusKind::TypeChange,
            StatusKind::Conflicted,
        ] {
            assert_eq!(format!("{:?}", kind), expected_wire_value(kind));
        }
    }

    /// `DiffLineDto::origin` is produced by `format!("{:?}", origin)`, so the `Debug`
    /// output *is* the wire format. Counterpart contract: the `DiffLineOrigin` union in
    /// `frontend/src/ipc/RepoClient.ts` (`"Add" | "Remove" | "Context"`) — these must stay
    /// in sync. Exhaustive on purpose: adding a `DiffLineOrigin` variant breaks compilation
    /// here, which is the reminder to extend the TypeScript union too.
    fn expected_diff_origin_wire_value(origin: DiffLineOrigin) -> &'static str {
        match origin {
            DiffLineOrigin::Add => "Add",
            DiffLineOrigin::Remove => "Remove",
            DiffLineOrigin::Context => "Context",
        }
    }

    #[test]
    fn diff_line_origin_wire_values_match_the_typescript_union() {
        for origin in [
            DiffLineOrigin::Add,
            DiffLineOrigin::Remove,
            DiffLineOrigin::Context,
        ] {
            assert_eq!(
                format!("{:?}", origin),
                expected_diff_origin_wire_value(origin)
            );
        }
    }

    #[test]
    fn two_open_repos_have_independent_worker_state() {
        use crate::worker::Worker;
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
}
