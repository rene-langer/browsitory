use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex};
use std::thread;

use git_core::diff::DiffHunk;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::worker::{TransferEvent, Worker};

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

#[derive(Default)]
pub struct AppState {
    pub worker: Mutex<Option<Worker>>,
}

#[tauri::command]
pub async fn open_repo(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let worker = Worker::spawn(PathBuf::from(&path))?;
    // A poisoned lock is recoverable here: the worker thread never touches this mutex, so
    // the `Option<Worker>` behind it can't have been left half-updated.
    *state.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
    // Best-effort: a repo that opened successfully should count as "recent" even if we can't
    // persist that fact (e.g. an unwritable config dir) — don't fail the whole open_repo call
    // over it.
    let _ = config::add_recent_repo(Path::new(&path));
    Ok(())
}

#[tauri::command]
pub async fn pick_repo_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|path| path.to_string())
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

// Clone the worker's channel handle and drop the guard before blocking on the reply —
// holding the mutex across the round-trip would serialize every command behind this one
// and let a wedged worker hold the lock forever.
fn worker_handle(state: &State<AppState>) -> Result<crate::worker::WorkerHandle, String> {
    let guard = state.worker.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .map(Worker::handle)
        .ok_or_else(|| "no repo open".to_string())
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<Vec<StatusEntryDto>, String> {
    let entries = worker_handle(&state)?.get_status()?;
    Ok(entries
        .into_iter()
        .map(|e| StatusEntryDto {
            path: e.path,
            staged: e.staged,
            kind: format!("{:?}", e.kind),
        })
        .collect())
}

#[tauri::command]
pub async fn get_commit_graph(
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<GraphCommitDto>, String> {
    let commits = worker_handle(&state)?.get_commit_graph(limit)?;
    Ok(commits.into_iter().map(GraphCommitDto::from).collect())
}

#[tauri::command]
pub async fn get_working_diff(
    path: String,
    staged: bool,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    let hunks = worker_handle(&state)?.get_working_diff(path, staged)?;
    Ok(hunks.into_iter().map(DiffHunkDto::from).collect())
}

#[tauri::command]
pub async fn get_commit_diff(
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    let hunks = worker_handle(&state)?.get_commit_diff(commit_id, path)?;
    Ok(hunks.into_iter().map(DiffHunkDto::from).collect())
}

#[tauri::command]
pub async fn get_commit_files(
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    worker_handle(&state)?.get_commit_files(commit_id)
}

#[tauri::command]
pub async fn get_blame(
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BlameLineDto>, String> {
    let lines = worker_handle(&state)?.get_blame(commit_id, path)?;
    Ok(lines.into_iter().map(BlameLineDto::from).collect())
}

#[tauri::command]
pub async fn stage_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.stage_file(path)
}

#[tauri::command]
pub async fn unstage_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.unstage_file(path)
}

#[tauri::command]
pub async fn commit(message: String, state: State<'_, AppState>) -> Result<String, String> {
    worker_handle(&state)?.commit(message)
}

#[tauri::command]
pub async fn list_branches(state: State<'_, AppState>) -> Result<Vec<BranchInfoDto>, String> {
    let branches = worker_handle(&state)?.list_branches()?;
    Ok(branches.into_iter().map(BranchInfoDto::from).collect())
}

#[tauri::command]
pub async fn list_worktrees(state: State<'_, AppState>) -> Result<Vec<WorktreeInfoDto>, String> {
    Ok(worker_handle(&state)?
        .list_worktrees()?
        .into_iter()
        .map(WorktreeInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn create_worktree(
    name: String,
    path: String,
    branch: String,
    start_point: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.create_worktree(name, PathBuf::from(path), branch, start_point)
}

#[tauri::command]
pub async fn remove_worktree(name: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.remove_worktree(name)
}

#[tauri::command]
pub async fn prune_worktrees(state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.prune_worktrees()
}

#[tauri::command]
pub async fn create_branch(
    name: String,
    start_point: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.create_branch(name, start_point)
}

#[tauri::command]
pub async fn switch_branch(name: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.switch_branch(name)
}

#[tauri::command]
pub async fn delete_branch(
    name: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.delete_branch(name, force)
}

#[tauri::command]
pub async fn rename_branch(
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.rename_branch(old_name, new_name)
}

#[tauri::command]
pub async fn list_remotes(state: State<'_, AppState>) -> Result<Vec<RemoteInfoDto>, String> {
    let worker = worker_handle(&state)?;
    worker
        .list_remotes()?
        .into_iter()
        .map(|remote| {
            let profile = worker.get_remote_auth_mode(remote.name.clone())?;
            Ok(RemoteInfoDto::from((remote, profile)))
        })
        .collect()
}

#[tauri::command]
pub async fn get_current_upstream(
    state: State<'_, AppState>,
) -> Result<Option<UpstreamInfoDto>, String> {
    let upstream = worker_handle(&state)?.get_current_upstream()?;
    Ok(upstream.map(UpstreamInfoDto::from))
}
#[tauri::command]
pub async fn get_remote_upstreams(
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<UpstreamInfoDto>, String> {
    Ok(worker_handle(&state)?
        .get_remote_upstreams(name)?
        .into_iter()
        .map(UpstreamInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn add_remote(
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.add_remote(name, fetch_url, push_url)
}

#[tauri::command]
pub async fn rename_remote(
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.rename_remote(old_name, new_name)
}

#[tauri::command]
pub async fn update_remote_urls(
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.update_remote_urls(name, fetch_url, push_url)
}

#[tauri::command]
pub async fn remove_remote(
    name: String,
    clear_upstreams: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.remove_remote(name, clear_upstreams)
}

#[tauri::command]
pub async fn save_https_credential(
    remote_name: String,
    username: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.save_https_credential(remote_name, username, token)
}

#[tauri::command]
pub async fn forget_https_credential(
    remote_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.forget_https_credential(remote_name)
}

#[tauri::command]
pub async fn set_remote_auth_mode(
    remote_name: String,
    mode: RemoteAuthModeDto,
    username: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mode = match mode {
        RemoteAuthModeDto::HttpsToken => git_core::remote::RemoteAuthMode::HttpsToken {
            username: username
                .filter(|username| !username.trim().is_empty())
                .ok_or_else(|| "HTTPS username is required".to_string())?,
        },
        RemoteAuthModeDto::SshAgent => git_core::remote::RemoteAuthMode::SshAgent,
    };
    worker_handle(&state)?.set_remote_auth_mode(remote_name, mode)
}

#[tauri::command]
pub async fn set_current_upstream(
    remote_name: String,
    remote_branch: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.set_current_upstream(remote_name, remote_branch)
}

#[tauri::command]
pub async fn clear_current_upstream(state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.clear_current_upstream()
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<TagInfoDto>, String> {
    Ok(worker_handle(&state)?
        .list_tags()?
        .into_iter()
        .map(TagInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn create_tag(
    name: String,
    message: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.create_tag(name, message)
}

#[tauri::command]
pub async fn delete_tag(name: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.delete_tag(name)
}

#[tauri::command]
pub async fn fetch_remote(
    remote_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state)?.fetch_remote(remote_name, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn push_current_branch(
    remote_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state)?.push_current_branch(remote_name, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn push_tags(
    remote_name: String,
    names: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state)?.push_tags(remote_name, names, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn pull_current_upstream(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PullOutcomeDto, String> {
    let (event_tx, event_rx) = mpsc::channel();
    emit_transfer_events(app, event_rx);
    let handle = worker_handle(&state)?;
    Ok(
        tauri::async_runtime::spawn_blocking(move || handle.pull_current_upstream(event_tx))
            .await
            .map_err(|_| "pull worker task stopped".to_string())??
            .into(),
    )
}

#[tauri::command]
pub async fn list_stashes(state: State<'_, AppState>) -> Result<Vec<StashEntryDto>, String> {
    let stashes = worker_handle(&state)?.list_stashes()?;
    Ok(stashes.into_iter().map(StashEntryDto::from).collect())
}

#[tauri::command]
pub async fn save_stash(state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.save_stash()
}

#[tauri::command]
pub async fn apply_stash(index: usize, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.apply_stash(index)
}

#[tauri::command]
pub async fn drop_stash(index: usize, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.drop_stash(index)
}

#[tauri::command]
pub async fn start_merge(
    branch_name: String,
    state: State<'_, AppState>,
) -> Result<MergeOutcomeDto, String> {
    let outcome = worker_handle(&state)?.start_merge(branch_name)?;
    Ok(MergeOutcomeDto::from(outcome))
}

#[tauri::command]
pub async fn get_conflict_hunks(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ConflictSegmentDto>, String> {
    let segments = worker_handle(&state)?.get_conflict_hunks(path)?;
    Ok(segments.into_iter().map(ConflictSegmentDto::from).collect())
}

#[tauri::command]
pub async fn resolve_conflict(
    path: String,
    resolved_content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.resolve_conflict(path, resolved_content)
}

#[tauri::command]
pub async fn abort_merge(state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.abort_merge()
}

#[tauri::command]
pub async fn get_merge_message(state: State<'_, AppState>) -> Result<Option<String>, String> {
    worker_handle(&state)?.get_merge_message()
}

#[tauri::command]
pub async fn resolve_add_delete_conflict(
    path: String,
    choice: FileConflictChoiceDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.resolve_add_delete_conflict(path, choice.into())
}

#[tauri::command]
pub async fn commits_since(
    onto: String,
    state: State<'_, AppState>,
) -> Result<Vec<RebasePlanCommitDto>, String> {
    let commits = worker_handle(&state)?.commits_since(onto)?;
    Ok(commits.into_iter().map(RebasePlanCommitDto::from).collect())
}

#[tauri::command]
pub async fn start_rebase(
    onto: String,
    plan: Vec<RebasePlanEntryDto>,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, String> {
    let plan = plan.into_iter().map(Into::into).collect();
    let result = worker_handle(&state)?.start_rebase(onto, plan)?;
    Ok(RebaseStepResultDto::from(result))
}

#[tauri::command]
pub async fn rebase_continue(state: State<'_, AppState>) -> Result<RebaseStepResultDto, String> {
    let result = worker_handle(&state)?.rebase_continue()?;
    Ok(RebaseStepResultDto::from(result))
}

#[tauri::command]
pub async fn abort_rebase(state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.abort_rebase()
}

#[tauri::command]
pub async fn get_rebase_progress(
    state: State<'_, AppState>,
) -> Result<Option<RebaseProgressDto>, String> {
    let progress = worker_handle(&state)?.get_rebase_progress()?;
    Ok(
        progress.map(|(current_step, total_steps)| RebaseProgressDto {
            current_step,
            total_steps,
        }),
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use git_core::diff::DiffLineOrigin;
    use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};
    use git_core::status::StatusKind;
    use git_core::worktree::WorktreeInfo;

    use crate::worker::TransferEvent;

    use super::{transfer_event_payload, PullOutcomeDto, RemoteAuthModeDto, WorktreeInfoDto};

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
}
