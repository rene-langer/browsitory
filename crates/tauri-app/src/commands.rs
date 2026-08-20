use std::collections::HashMap;
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
pub fn list_open_repos() -> Result<(Vec<String>, Option<String>), String> {
    let (paths, active) = config::list_open_repos().map_err(|e| e.to_string())?;
    Ok((
        paths.into_iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        active.map(|p| p.to_string_lossy().into_owned()),
    ))
}

#[tauri::command]
pub fn persist_open_repos(paths: Vec<String>, active_path: Option<String>) -> Result<(), String> {
    config::set_open_repos(
        &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
        active_path.as_deref().map(Path::new),
    )
    .map_err(|e| e.to_string())
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
fn worker_handle(state: &State<AppState>, repo_path: &str) -> Result<crate::worker::WorkerHandle, String> {
    let guard = state.workers.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get(repo_path)
        .map(Worker::handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))
}

#[tauri::command]
pub async fn get_status(repo_path: String, state: State<'_, AppState>) -> Result<Vec<StatusEntryDto>, String> {
    let entries = worker_handle(&state, &repo_path)?.get_status()?;
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
    repo_path: String,
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<GraphCommitDto>, String> {
    let commits = worker_handle(&state, &repo_path)?.get_commit_graph(limit)?;
    Ok(commits.into_iter().map(GraphCommitDto::from).collect())
}

#[tauri::command]
pub async fn get_working_diff(
    repo_path: String,
    path: String,
    staged: bool,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    let hunks = worker_handle(&state, &repo_path)?.get_working_diff(path, staged)?;
    Ok(hunks.into_iter().map(DiffHunkDto::from).collect())
}

#[tauri::command]
pub async fn get_commit_diff(
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    let hunks = worker_handle(&state, &repo_path)?.get_commit_diff(commit_id, path)?;
    Ok(hunks.into_iter().map(DiffHunkDto::from).collect())
}

#[tauri::command]
pub async fn get_commit_files(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    worker_handle(&state, &repo_path)?.get_commit_files(commit_id)
}

#[tauri::command]
pub async fn get_blame(
    repo_path: String,
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BlameLineDto>, String> {
    let lines = worker_handle(&state, &repo_path)?.get_blame(commit_id, path)?;
    Ok(lines.into_iter().map(BlameLineDto::from).collect())
}

#[tauri::command]
pub async fn stage_file(repo_path: String, path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.stage_file(path)
}

#[tauri::command]
pub async fn unstage_file(repo_path: String, path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.unstage_file(path)
}

#[tauri::command]
pub async fn commit(repo_path: String, message: String, state: State<'_, AppState>) -> Result<String, String> {
    worker_handle(&state, &repo_path)?.commit(message)
}

#[tauri::command]
pub async fn list_branches(repo_path: String, state: State<'_, AppState>) -> Result<Vec<BranchInfoDto>, String> {
    let branches = worker_handle(&state, &repo_path)?.list_branches()?;
    Ok(branches.into_iter().map(BranchInfoDto::from).collect())
}

#[tauri::command]
pub async fn list_worktrees(repo_path: String, state: State<'_, AppState>) -> Result<Vec<WorktreeInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .list_worktrees()?
        .into_iter()
        .map(WorktreeInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn create_worktree(
    repo_path: String,
    name: String,
    path: String,
    branch: String,
    start_point: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.create_worktree(name, PathBuf::from(path), branch, start_point)
}

#[tauri::command]
pub async fn remove_worktree(repo_path: String, name: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.remove_worktree(name)
}

#[tauri::command]
pub async fn prune_worktrees(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.prune_worktrees()
}

#[tauri::command]
pub async fn list_submodules(repo_path: String, state: State<'_, AppState>) -> Result<Vec<SubmoduleInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .list_submodules()?
        .into_iter()
        .map(SubmoduleInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn init_submodule(repo_path: String, path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.init_submodule(path)
}

#[tauri::command]
pub async fn update_submodule(
    repo_path: String,
    path: String,
    recursive: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.update_submodule(path, recursive)
}

#[tauri::command]
pub async fn list_reflog_refs(repo_path: String, state: State<'_, AppState>) -> Result<Vec<String>, String> {
    worker_handle(&state, &repo_path)?.list_reflog_refs()
}

#[tauri::command]
pub async fn get_reflog(
    repo_path: String,
    reference: String,
    state: State<'_, AppState>,
) -> Result<Vec<ReflogEntryDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_reflog(reference)?
        .into_iter()
        .map(ReflogEntryDto::from)
        .collect())
}

#[tauri::command]
pub async fn restore_reflog_entry(
    repo_path: String,
    reference: String,
    new_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.restore_reflog_entry(reference, new_id)
}

#[tauri::command]
pub async fn create_branch(
    repo_path: String,
    name: String,
    start_point: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.create_branch(name, start_point)
}

#[tauri::command]
pub async fn switch_branch(repo_path: String, name: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.switch_branch(name)
}

#[tauri::command]
pub async fn delete_branch(
    repo_path: String,
    name: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.delete_branch(name, force)
}

#[tauri::command]
pub async fn rename_branch(
    repo_path: String,
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.rename_branch(old_name, new_name)
}

#[tauri::command]
pub async fn list_remotes(repo_path: String, state: State<'_, AppState>) -> Result<Vec<RemoteInfoDto>, String> {
    let worker = worker_handle(&state, &repo_path)?;
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
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Option<UpstreamInfoDto>, String> {
    let upstream = worker_handle(&state, &repo_path)?.get_current_upstream()?;
    Ok(upstream.map(UpstreamInfoDto::from))
}
#[tauri::command]
pub async fn get_remote_upstreams(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<Vec<UpstreamInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .get_remote_upstreams(name)?
        .into_iter()
        .map(UpstreamInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn add_remote(
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.add_remote(name, fetch_url, push_url)
}

#[tauri::command]
pub async fn rename_remote(
    repo_path: String,
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.rename_remote(old_name, new_name)
}

#[tauri::command]
pub async fn update_remote_urls(
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.update_remote_urls(name, fetch_url, push_url)
}

#[tauri::command]
pub async fn remove_remote(
    repo_path: String,
    name: String,
    clear_upstreams: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.remove_remote(name, clear_upstreams)
}

#[tauri::command]
pub async fn save_https_credential(
    repo_path: String,
    remote_name: String,
    username: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.save_https_credential(remote_name, username, token)
}

#[tauri::command]
pub async fn forget_https_credential(
    repo_path: String,
    remote_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.forget_https_credential(remote_name)
}

#[tauri::command]
pub async fn set_remote_auth_mode(
    repo_path: String,
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
    worker_handle(&state, &repo_path)?.set_remote_auth_mode(remote_name, mode)
}

#[tauri::command]
pub async fn set_current_upstream(
    repo_path: String,
    remote_name: String,
    remote_branch: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.set_current_upstream(remote_name, remote_branch)
}

#[tauri::command]
pub async fn clear_current_upstream(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.clear_current_upstream()
}

#[tauri::command]
pub async fn list_tags(repo_path: String, state: State<'_, AppState>) -> Result<Vec<TagInfoDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .list_tags()?
        .into_iter()
        .map(TagInfoDto::from)
        .collect())
}

#[tauri::command]
pub async fn create_tag(
    repo_path: String,
    name: String,
    message: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.create_tag(name, message)
}

#[tauri::command]
pub async fn delete_tag(repo_path: String, name: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.delete_tag(name)
}

#[tauri::command]
pub async fn fetch_remote(
    repo_path: String,
    remote_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state, &repo_path)?.fetch_remote(remote_name, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn push_current_branch(
    repo_path: String,
    remote_name: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state, &repo_path)?.push_current_branch(remote_name, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn push_tags(
    repo_path: String,
    remote_name: String,
    names: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = worker_handle(&state, &repo_path)?.push_tags(remote_name, names, event_tx)?;
    emit_transfer_events(app, event_rx);
    Ok(operation_id)
}

#[tauri::command]
pub async fn pull_current_upstream(
    repo_path: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PullOutcomeDto, String> {
    let (event_tx, event_rx) = mpsc::channel();
    emit_transfer_events(app, event_rx);
    let handle = worker_handle(&state, &repo_path)?;
    Ok(
        tauri::async_runtime::spawn_blocking(move || handle.pull_current_upstream(event_tx))
            .await
            .map_err(|_| "pull worker task stopped".to_string())??
            .into(),
    )
}

#[tauri::command]
pub async fn list_stashes(repo_path: String, state: State<'_, AppState>) -> Result<Vec<StashEntryDto>, String> {
    let stashes = worker_handle(&state, &repo_path)?.list_stashes()?;
    Ok(stashes.into_iter().map(StashEntryDto::from).collect())
}

#[tauri::command]
pub async fn save_stash(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.save_stash()
}

#[tauri::command]
pub async fn apply_stash(repo_path: String, index: usize, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.apply_stash(index)
}

#[tauri::command]
pub async fn drop_stash(repo_path: String, index: usize, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.drop_stash(index)
}

#[tauri::command]
pub async fn start_merge(
    repo_path: String,
    branch_name: String,
    state: State<'_, AppState>,
) -> Result<MergeOutcomeDto, String> {
    let outcome = worker_handle(&state, &repo_path)?.start_merge(branch_name)?;
    Ok(MergeOutcomeDto::from(outcome))
}

#[tauri::command]
pub async fn get_conflict_hunks(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ConflictSegmentDto>, String> {
    let segments = worker_handle(&state, &repo_path)?.get_conflict_hunks(path)?;
    Ok(segments.into_iter().map(ConflictSegmentDto::from).collect())
}

#[tauri::command]
pub async fn resolve_conflict(
    repo_path: String,
    path: String,
    resolved_content: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.resolve_conflict(path, resolved_content)
}

#[tauri::command]
pub async fn abort_merge(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.abort_merge()
}

#[tauri::command]
pub async fn get_merge_message(repo_path: String, state: State<'_, AppState>) -> Result<Option<String>, String> {
    worker_handle(&state, &repo_path)?.get_merge_message()
}

#[tauri::command]
pub async fn resolve_add_delete_conflict(
    repo_path: String,
    path: String,
    choice: FileConflictChoiceDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.resolve_add_delete_conflict(path, choice.into())
}

#[tauri::command]
pub async fn commits_since(
    repo_path: String,
    onto: String,
    state: State<'_, AppState>,
) -> Result<Vec<RebasePlanCommitDto>, String> {
    let commits = worker_handle(&state, &repo_path)?.commits_since(onto)?;
    Ok(commits.into_iter().map(RebasePlanCommitDto::from).collect())
}

#[tauri::command]
pub async fn start_rebase(
    repo_path: String,
    onto: String,
    plan: Vec<RebasePlanEntryDto>,
    state: State<'_, AppState>,
) -> Result<RebaseStepResultDto, String> {
    let plan = plan.into_iter().map(Into::into).collect();
    let result = worker_handle(&state, &repo_path)?.start_rebase(onto, plan)?;
    Ok(RebaseStepResultDto::from(result))
}

#[tauri::command]
pub async fn rebase_continue(repo_path: String, state: State<'_, AppState>) -> Result<RebaseStepResultDto, String> {
    let result = worker_handle(&state, &repo_path)?.rebase_continue()?;
    Ok(RebaseStepResultDto::from(result))
}

#[tauri::command]
pub async fn abort_rebase(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.abort_rebase()
}

#[tauri::command]
pub async fn get_rebase_progress(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Option<RebaseProgressDto>, String> {
    let progress = worker_handle(&state, &repo_path)?.get_rebase_progress()?;
    Ok(
        progress.map(|(current_step, total_steps)| RebaseProgressDto {
            current_step,
            total_steps,
        }),
    )
}

#[tauri::command]
pub async fn detect_forge_repository(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ForgeRepositoryDto>, String> {
    Ok(worker_handle(&state, &repo_path)?
        .detect_forge_repository()?
        .into_iter()
        .map(ForgeRepositoryDto::from)
        .collect())
}

#[tauri::command]
pub async fn save_forge_token(
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
    token: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.save_forge_token(provider.into(), account, token)
}

#[tauri::command]
pub async fn forget_forge_token(
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.forget_forge_token(provider.into(), account)
}

#[tauri::command]
pub async fn list_pull_requests(
    repo_path: String,
    remote_name: String,
    account: String,
    state: State<'_, AppState>,
) -> Result<PullRequestListDto, String> {
    Ok(PullRequestListDto::from(
        worker_handle(&state, &repo_path)?.list_pull_requests(remote_name, account)?,
    ))
}

#[tauri::command]
pub async fn create_pull_request(
    repo_path: String,
    remote_name: String,
    account: String,
    pull_request: CreatePullRequestDto,
    state: State<'_, AppState>,
) -> Result<PullRequestDto, String> {
    let created =
        worker_handle(&state, &repo_path)?.create_pull_request(remote_name, account, pull_request.into())?;
    Ok(PullRequestDto::from(created))
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

    use git_core::diff::DiffLineOrigin;
    use git_core::reflog::ReflogEntry;
    use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};
    use git_core::status::StatusKind;
    use git_core::submodule::SubmoduleInfo;
    use git_core::worktree::WorktreeInfo;

    use crate::worker::TransferEvent;

    use super::{
        transfer_event_payload, ForgeProviderDto, PullOutcomeDto, ReflogEntryDto,
        RemoteAuthModeDto, SubmoduleInfoDto, WorktreeInfoDto,
    };

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
        use std::collections::HashMap;
        use crate::worker::Worker;

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
