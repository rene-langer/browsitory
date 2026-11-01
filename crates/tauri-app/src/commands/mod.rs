use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use git_core::diff::DiffHunk;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use repo_service::worker::{HookEvent, TransferEvent, Worker, WorkerHandle};

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
    /// Only on `RemoteFailed`: the remote that failed during a Fetch all.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_name: Option<String>,
    /// Local `pre-push` hook output on a `HookFailed` completion, `null` otherwise. Unlike
    /// `message` this comes from the user's own script, not the remote.
    pub hook_output: Option<String>,
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
            remote_name: None,
            hook_output: None,
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
                remote_name: None,
                hook_output: None,
            },
        ),
        TransferEvent::Progress(progress) => {
            ("transfer-progress", TransferProgressDto::from(progress))
        }
        TransferEvent::RemoteFailed {
            operation_id,
            remote_name,
            error,
        } => (
            "transfer-progress",
            TransferProgressDto {
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
        ),
        TransferEvent::Completed {
            operation_id,
            operation,
            error,
            hook_output,
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
                    remote_name: None,
                    hook_output,
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
#[serde(rename_all = "camelCase")]
pub struct StatusEntryDto {
    pub path: String,
    /// A staged rename's old path; always present (`null` otherwise).
    pub old_path: Option<String>,
    pub staged: bool,
    pub kind: String,
}

impl From<git_core::status::StatusEntry> for StatusEntryDto {
    fn from(entry: git_core::status::StatusEntry) -> Self {
        Self {
            path: entry.path,
            old_path: entry.old_path,
            staged: entry.staged,
            kind: format!("{:?}", entry.kind),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfoDto {
    pub name: String,
    pub is_current: bool,
    // Nulls are part of the wire contract: no `skip_serializing_if`.
    pub upstream: Option<String>,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
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
    /// Effective git-config prune default for this remote (the UI's Prune toggle fallback).
    pub prune: bool,
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
    pub commit_id: Option<String>,
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
            commit_id: tag.commit_id,
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
            prune: remote.prune,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamInfoDto {
    pub local_branch: String,
    pub remote_name: String,
    pub remote_branch: String,
    pub ahead: Option<usize>,
    pub behind: Option<usize>,
    /// Where Push lands under `push.default`; null = ask with the Push to… dialog.
    pub push_branch: Option<String>,
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
            ahead: upstream.ahead,
            behind: upstream.behind,
            push_branch: upstream.push_branch,
        }
    }
}

impl From<git_core::branch::BranchInfo> for BranchInfoDto {
    fn from(b: git_core::branch::BranchInfo) -> Self {
        BranchInfoDto {
            name: b.name,
            is_current: b.is_current,
            upstream: b.upstream,
            ahead: b.ahead,
            behind: b.behind,
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
    pub remote_branch_refs: Vec<String>,
    pub tags: Vec<String>,
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
            remote_branch_refs: c.remote_branch_refs,
            tags: c.tags,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryEntryDto {
    pub commit: GraphCommitDto,
    pub path: String,
    // Serialized as `null` (not omitted) — the TS type is `string | null`.
    pub old_path: Option<String>,
}

impl From<git_core::history::FileHistoryEntry> for FileHistoryEntryDto {
    fn from(e: git_core::history::FileHistoryEntry) -> Self {
        FileHistoryEntryDto {
            commit: GraphCommitDto::from(e.commit),
            path: e.path,
            old_path: e.old_path,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPageDto {
    pub commits: Vec<GraphCommitDto>,
    // Serialized as `null` (not omitted) once the walk is exhausted — the TS type is
    // `string | null`.
    pub next_cursor: Option<String>,
    pub scanned: usize,
}

impl From<git_core::search::SearchPage> for SearchPageDto {
    fn from(p: git_core::search::SearchPage) -> Self {
        SearchPageDto {
            commits: p.commits.into_iter().map(GraphCommitDto::from).collect(),
            next_cursor: p.next_cursor,
            scanned: p.scanned,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFileDto {
    pub path: String,
    // Serialized as `null` (not omitted) — the TS type is `string | null`.
    pub old_path: Option<String>,
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
pub struct CompareRangeDto {
    pub from_id: String,
    pub to_id: String,
}

impl From<git_core::diff::CompareRange> for CompareRangeDto {
    fn from(r: git_core::diff::CompareRange) -> Self {
        CompareRangeDto {
            from_id: r.from_id,
            to_id: r.to_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadInfoDto {
    pub head_id: Option<String>,
    pub detached_at: Option<String>,
    pub head_pushed: bool,
    pub operation: Option<&'static str>,
    pub squash_message: Option<String>,
    pub commit_hooks: Vec<String>,
    pub sequence: Option<SequenceProgressDto>,
    pub rebase: Option<RebaseInfoDto>,
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

/// A paused interactive rebase: `health` is `{ "kind": "Paused" }` or `{ "kind": "Diverged",
/// "reason": "HeadMoved" | "BranchMoved" | "BranchMissing" | "OtherOperation" }`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebaseInfoDto {
    pub branch: String,
    pub onto: String,
    pub current_step: usize,
    pub total_steps: usize,
    pub health: RebaseHealthDto,
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "reason")]
pub enum RebaseHealthDto {
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
pub struct RebaseAbortDto {
    pub restored: bool,
}

impl From<git_core::rebase::RebaseAbort> for RebaseAbortDto {
    fn from(abort: git_core::rebase::RebaseAbort) -> Self {
        Self {
            restored: abort.restored,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceProgressDto {
    pub kind: &'static str,
    pub current_step: usize,
    pub total_steps: usize,
    pub current_commit: String,
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
pub enum SequenceOutcomeDto {
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
pub struct SequenceAbortDto {
    pub rewound: bool,
}

impl From<git_core::pick::SequenceAbort> for SequenceAbortDto {
    fn from(abort: git_core::pick::SequenceAbort) -> Self {
        Self {
            rewound: abort.rewound,
        }
    }
}

#[derive(Deserialize)]
pub enum ConflictSideDto {
    Ours,
    Theirs,
}

impl From<ConflictSideDto> for git_core::merge::ConflictSide {
    fn from(side: ConflictSideDto) -> Self {
        match side {
            ConflictSideDto::Ours => Self::Ours,
            ConflictSideDto::Theirs => Self::Theirs,
        }
    }
}

/// A commit/amend result: a failing gate hook is data the UI renders, not an error.
#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum CommitOutcomeDto {
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

/// Payload of the `hook-progress` event (commit/amend hooks; pushes report `RunningHook`
/// through `transfer-progress` instead).
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookProgressDto {
    pub repo_path: String,
    pub operation_id: String,
    pub hook: String,
    pub phase: &'static str,
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

/// Like `emit_transfer_events`: drains on its own thread until the worker drops its sender.
fn emit_hook_events(app: AppHandle, repo_path: String, events: mpsc::Receiver<HookEvent>) {
    thread::spawn(move || {
        for event in events {
            let _ = app.emit("hook-progress", hook_progress_dto(&repo_path, event));
        }
    });
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum PickOutcomeDto {
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
pub struct UndoStateDto {
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
pub enum SignatureStatusDto {
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
pub enum SignatureFormatDto {
    Openpgp,
    Ssh,
}

#[derive(Serialize)]
pub struct CommitSignatureDto {
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
pub enum UndoOutcomeDto {
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
pub enum ResetModeDto {
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

/// One side of a file for the image preview. `Loaded` carries the bytes as base64 and the
/// image type sniffed from them (`null` when they are not a previewable image).
#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
pub enum FileSideDto {
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

#[derive(Debug, Serialize)]
pub struct FileSidesDto {
    pub old: FileSideDto,
    pub new: FileSideDto,
}

impl From<git_core::diff::FileSides> for FileSidesDto {
    fn from(sides: git_core::diff::FileSides) -> Self {
        Self {
            old: sides.old.into(),
            new: sides.new.into(),
        }
    }
}

/// A `Key: value` trailer for a commit/amend message (`RepoClient`'s `Trailer`).
#[derive(Debug, Deserialize)]
pub struct TrailerDto {
    pub key: String,
    pub value: String,
}

impl From<TrailerDto> for git_core::commit::Trailer {
    fn from(dto: TrailerDto) -> Self {
        Self {
            key: dto.key,
            value: dto.value,
        }
    }
}

/// A person: the amend author override (in) and a recent author (out).
#[derive(Debug, Serialize, Deserialize)]
pub struct SignatureInputDto {
    pub name: String,
    pub email: String,
}

impl From<SignatureInputDto> for git_core::commit::SignatureInput {
    fn from(dto: SignatureInputDto) -> Self {
        Self {
            name: dto.name,
            email: dto.email,
        }
    }
}

impl From<git_core::commit::SignatureInput> for SignatureInputDto {
    fn from(input: git_core::commit::SignatureInput) -> Self {
        Self {
            name: input.name,
            email: input.email,
        }
    }
}

pub(crate) fn trailers_from(dtos: Option<Vec<TrailerDto>>) -> Vec<git_core::commit::Trailer> {
    dtos.unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect()
}

/// Which two sides "Open in diff tool" compares (`RepoClient`'s `DiffSource`).
#[derive(Deserialize)]
#[serde(tag = "kind")]
pub enum DiffSourceDto {
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

#[derive(Serialize)]
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

#[derive(Serialize)]
pub enum MarkResolvedOutcomeDto {
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

#[derive(Serialize, Deserialize)]
pub struct ExternalToolSettingsDto {
    pub editor: Option<String>,
    pub terminal: Option<String>,
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

#[derive(Serialize)]
pub enum GitConfigSourceDto {
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
pub struct GitToolDto {
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
pub struct GitToolsDto {
    diff_tool: Option<GitToolDto>,
    merge_tool: Option<GitToolDto>,
}

impl From<git_core::tools::GitTools> for GitToolsDto {
    fn from(tools: git_core::tools::GitTools) -> Self {
        Self {
            diff_tool: tools.diff_tool.map(Into::into),
            merge_tool: tools.merge_tool.map(Into::into),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum MergeOutcomeDto {
    UpToDate,
    FastForwarded,
    Merged,
    Squashed,
    Conflicted { files: Vec<String> },
}

impl From<git_core::merge::MergeOutcome> for MergeOutcomeDto {
    fn from(outcome: git_core::merge::MergeOutcome) -> Self {
        match outcome {
            git_core::merge::MergeOutcome::UpToDate => MergeOutcomeDto::UpToDate,
            git_core::merge::MergeOutcome::FastForwarded => MergeOutcomeDto::FastForwarded,
            git_core::merge::MergeOutcome::Merged => MergeOutcomeDto::Merged,
            git_core::merge::MergeOutcome::Squashed => MergeOutcomeDto::Squashed,
            git_core::merge::MergeOutcome::Conflicted { files } => {
                MergeOutcomeDto::Conflicted { files }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FastForwardDto {
    Allow,
    Never,
    Only,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOptionsDto {
    pub fast_forward: FastForwardDto,
    pub squash: bool,
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

impl From<config::MergePreference> for MergeOptionsDto {
    fn from(preference: config::MergePreference) -> Self {
        Self {
            fast_forward: match preference.fast_forward {
                config::MergeFastForward::Allow => FastForwardDto::Allow,
                config::MergeFastForward::Never => FastForwardDto::Never,
                config::MergeFastForward::Only => FastForwardDto::Only,
            },
            squash: preference.squash,
        }
    }
}

impl From<FastForwardDto> for config::MergeFastForward {
    fn from(dto: FastForwardDto) -> Self {
        match dto {
            FastForwardDto::Allow => Self::Allow,
            FastForwardDto::Never => Self::Never,
            FastForwardDto::Only => Self::Only,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashOptionsDto {
    pub message: Option<String>,
    pub keep_index: bool,
    pub include_untracked: bool,
    pub paths: Vec<String>,
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

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum StashApplyOutcomeDto {
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashFilesDto {
    pub tracked: Vec<String>,
    pub untracked: Vec<String>,
    pub untracked_commit_id: Option<String>,
}

impl From<git_core::stash::StashFiles> for StashFilesDto {
    fn from(files: git_core::stash::StashFiles) -> Self {
        Self {
            tracked: files.tracked,
            untracked: files.untracked,
            untracked_commit_id: files.untracked_commit_id,
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

/// A page of listed pull requests, plus whether the provider indicated more exist beyond this
/// page (see `repo_service::pull_requests::PullRequestList`) — the frontend uses `truncated` to show
/// an explicit "more available" notice instead of silently displaying a partial list.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestListDto {
    pub pull_requests: Vec<PullRequestDto>,
    pub truncated: bool,
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

/// The fields a caller supplies to open a new pull request. Never carries a token — see
/// `repo_service::pull_requests::CreatePullRequest`'s doc comment, which this DTO mirrors field-for-
/// field.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePullRequestDto {
    pub title: String,
    pub description: Option<String>,
    pub source_branch: String,
    pub target_branch: String,
}

impl From<CreatePullRequestDto> for repo_service::pull_requests::CreatePullRequest {
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
    /// One gate per open repository, keyed like `workers` (see `RepoWorker::call`, #181).
    gates: Mutex<HashMap<String, Arc<RepoGate>>>,
}

/// Lets one call per repository at a time take a blocking-pool thread (see `RepoWorker::call`).
type RepoGate = tauri::async_runtime::Mutex<()>;

#[tauri::command]
pub async fn open_repo(path: String, state: State<'_, AppState>) -> Result<(), String> {
    // `ensure_worker` evicts a dead worker (its thread having exited, e.g. from a panic inside
    // a `Command` match arm) before deciding whether to reuse or respawn — a plain
    // `contains_key` fast path would keep handing back a dead entry forever. See
    // AUD-2026-09-05-CONC-001.
    repo_service::worker::ensure_worker(&state.workers, &path)?;
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

/// Drops `repo_path`'s worker and gate. Calls already holding a clone of either finish as usual.
fn forget_repo(state: &AppState, repo_path: &str) {
    // Same lock order as `repo_worker` (workers, then gates), so a lookup racing a close can't
    // leave a gate behind for a repository that is no longer open.
    let mut workers = state.workers.lock().unwrap_or_else(|e| e.into_inner());
    workers.remove(repo_path);
    state
        .gates
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(repo_path);
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

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct DiffOptionsDto {
    pub ignore_whitespace: bool,
    pub context_lines: u32,
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
pub struct DiffSettingsDto {
    pub ignore_whitespace: bool,
    pub context_lines: u32,
    pub syntax_highlight: bool,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitMessageSettingsDto {
    pub summary_limit: u32,
    pub body_ruler: u32,
    pub show_ruler: bool,
}

impl From<config::CommitMessageSettings> for CommitMessageSettingsDto {
    fn from(s: config::CommitMessageSettings) -> Self {
        Self {
            summary_limit: s.summary_limit,
            body_ruler: s.body_ruler,
            show_ruler: s.show_ruler,
        }
    }
}

impl From<CommitMessageSettingsDto> for config::CommitMessageSettings {
    fn from(d: CommitMessageSettingsDto) -> Self {
        Self {
            summary_limit: d.summary_limit,
            body_ruler: d.body_ruler,
            show_ruler: d.show_ruler,
        }
    }
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

/// Looks up `repo_path`'s worker and gate (creating the gate on first use). Clones the worker's
/// channel handle and drops the guards before any round trip: holding the mutex across one would
/// serialize every command behind it and let a wedged worker hold the lock forever.
pub(crate) fn repo_worker(state: &AppState, repo_path: &str) -> Result<RepoWorker, String> {
    let workers = state.workers.lock().unwrap_or_else(|e| e.into_inner());
    let handle = workers
        .get(repo_path)
        .map(Worker::handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))?;
    let gate = state
        .gates
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(repo_path.to_string())
        .or_default()
        .clone();
    Ok(RepoWorker { handle, gate })
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

    /// An `AppState` with a worker open for each of `dirs`, keyed by path as `open_repo` does.
    fn state_with_repos(dirs: &[&std::path::Path]) -> (super::AppState, Vec<String>) {
        let state = super::AppState::default();
        let keys = dirs
            .iter()
            .map(|dir| {
                let key = dir.to_string_lossy().into_owned();
                repo_service::worker::ensure_worker(&state.workers, &key).unwrap();
                key
            })
            .collect();
        (state, keys)
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
        let handle = state.workers.lock().unwrap()[&keys[0]].handle();
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
        assert!(state.gates.lock().unwrap()[key].try_lock().is_err());
        commit
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
        assert!(super::repo_worker(&state, &keys[0]).is_err());
    }
    use std::path::PathBuf;

    use config::{OpenRepoEntry, Workspace};
    use git_core::graph::GraphCommit;
    use git_core::reflog::ReflogEntry;
    use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase, TransferProgress};
    use git_core::submodule::SubmoduleInfo;
    use git_core::worktree::WorktreeInfo;

    use repo_service::worker::{HookEvent, TransferEvent};

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
            },
        );
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({"repoPath": "/repo", "operationId": "commit-1", "hook": "pre-commit", "phase": "Started"})
        );
        let dto = hook_progress_dto(
            "/repo",
            HookEvent::Finished {
                operation_id: "commit-1".into(),
                hook: "pre-commit".into(),
            },
        );
        assert_eq!(serde_json::to_value(dto).unwrap()["phase"], "Finished");
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
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({"headId": "abc", "detachedAt": null, "headPushed": true, "operation": "CherryPick", "squashMessage": null, "commitHooks": ["pre-commit"], "sequence": null, "rebase": null})
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
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap()["squashMessage"],
            serde_json::json!("Squashed commit of the following:\n")
        );
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
        }))
        .unwrap()
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
