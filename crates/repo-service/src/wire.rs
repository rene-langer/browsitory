//! The wire DTOs both frontends' backends send and receive (#306): the desktop app's Tauri
//! commands (`crates/tauri-app`) and the VSCode sidecar's JSON-RPC methods
//! (`crates/vscode-sidecar`) use these same types and `From` impls, so one Rust type and one
//! mapping define each shape. Only transport-specific types stay in those crates.
//!
//! Each transport keeps its own serialization: Tauri writes `serde_json::to_string` of the value
//! (fields in declaration order), the sidecar `to_value` first (fields sorted). The byte-exact
//! snapshots in `crates/tauri-app/tests/wire/` and `crates/vscode-sidecar/tests/wire/` pin both.
//!
//! With the `ts` feature (test builds only) every type also derives ts-rs's `TS`; the bindings
//! test below writes `frontend/src/ipc/generated/wire-dto.ts` from them.

use git_core::diff::DiffHunk;
use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase};
use serde::{Deserialize, Serialize};

use crate::worker::{HookEvent, HookSource, TransferEvent};

#[derive(Clone, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "TransferProgress"))]
#[serde(rename_all = "camelCase")]
pub struct TransferProgressDto {
    pub operation_id: String,
    pub operation: TransferOperation,
    pub phase: TransferProgressPhaseDto,
    pub error_kind: Option<TransferErrorKind>,
    pub current: usize,
    pub total: usize,
    pub received_bytes: usize,
    pub message: Option<String>,
    /// Only on `RemoteFailed`: the remote that failed during a Fetch all.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub remote_name: Option<String>,
    /// Local `pre-push` hook output on a `HookFailed` completion, `null` otherwise. Unlike
    /// `message` this comes from the user's own script, not the remote.
    pub hook_output: Option<String>,
}

/// `TransferProgress.phase`: git-core's `TransferPhase` plus the phases only the transfer relay
/// (`transfer_progress_dto`) emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "TransferProgressPhase"))]
pub enum TransferProgressPhaseDto {
    Starting,
    Receiving,
    Updating,
    RunningHook,
    RemoteFailed,
    Completed,
    Failed,
}

impl From<TransferPhase> for TransferProgressPhaseDto {
    fn from(phase: TransferPhase) -> Self {
        match phase {
            TransferPhase::Receiving => Self::Receiving,
            TransferPhase::Updating => Self::Updating,
            TransferPhase::RunningHook => Self::RunningHook,
        }
    }
}

impl From<git_core::remote::TransferProgress> for TransferProgressDto {
    fn from(progress: git_core::remote::TransferProgress) -> Self {
        Self {
            operation_id: progress.operation_id,
            operation: progress.operation,
            phase: progress.phase.into(),
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

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "StatusEntry"))]
#[serde(rename_all = "camelCase")]
pub struct StatusEntryDto {
    pub path: String,
    /// A staged rename's old path; always present (`null` otherwise).
    pub old_path: Option<String>,
    pub staged: bool,
    pub kind: git_core::status::StatusKind,
}

impl From<git_core::status::StatusEntry> for StatusEntryDto {
    fn from(entry: git_core::status::StatusEntry) -> Self {
        Self {
            path: entry.path,
            old_path: entry.old_path,
            staged: entry.staged,
            kind: entry.kind,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "BranchInfo"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "WorktreeInfo"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "Workspace"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "OpenRepoEntry"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OpenRepoEntryInput {
    pub path: String,
    pub workspace_id: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "SubmoduleInfo"))]
#[serde(rename_all = "camelCase")]
pub struct SubmoduleInfoDto {
    pub path: String,
    pub url: Option<String>,
    pub gitlink_id: Option<String>,
    pub initialized: bool,
    pub head_id: Option<String>,
    /// Absolute path of the checked-out submodule, the folder its tab opens; `null` when it
    /// isn't checked out (#445).
    pub workdir: Option<String>,
}

impl From<git_core::submodule::SubmoduleInfo> for SubmoduleInfoDto {
    fn from(submodule: git_core::submodule::SubmoduleInfo) -> Self {
        Self {
            path: submodule.path,
            url: submodule.url,
            gitlink_id: submodule.gitlink_id,
            initialized: submodule.initialized,
            head_id: submodule.head_id,
            workdir: submodule.workdir,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ReflogEntry"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RemoteInfo"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RemoteAuthMode"))]
pub enum RemoteAuthModeDto {
    HttpsToken,
    SshAgent,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "TagInfo"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "UpstreamInfo"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PullOutcome"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "StashEntry"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "BlameLine"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "GraphCommit"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FileHistoryEntry"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "SearchPage"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ChangedFile"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CompareRange"))]
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

/// "Go to commit" (#441): see `git_core::locate::CommitLocation`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitLocation"))]
#[serde(rename_all = "camelCase")]
pub struct CommitLocationDto {
    pub commit_id: String,
    pub position: Option<usize>,
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
}

impl From<git_core::locate::CommitLocation> for CommitLocationDto {
    fn from(l: git_core::locate::CommitLocation) -> Self {
        CommitLocationDto {
            commit_id: l.commit_id,
            position: l.position,
            branches: l.branches,
            remote_branches: l.remote_branches,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "HeadInfo"))]
#[serde(rename_all = "camelCase")]
pub struct HeadInfoDto {
    pub head_id: Option<String>,
    pub detached_at: Option<String>,
    pub head_pushed: bool,
    pub operation: Option<git_core::repo::OperationKind>,
    /// `operation`'s changes are already in HEAD; only git's state files are left (#240). The next
    /// commit or amend clears them, and the commit box isn't pre-filled with the old message.
    pub operation_leftover: bool,
    pub squash_message: Option<String>,
    pub commit_hooks: Vec<String>,
    pub sequence: Option<SequenceProgressDto>,
    pub rebase: Option<RebaseInfoDto>,
    /// A `git bisect` started in a terminal: `{ "startedFrom": "main" }`, or `null`.
    pub bisect: Option<BisectDto>,
    /// A `git cherry-pick A B C` / `git revert A B` started in a terminal and not finished, or
    /// `null` (#258).
    pub terminal_sequence: Option<TerminalSequenceDto>,
    /// A `git rebase` or `git am` started in a terminal and not finished, or `null` (#263).
    pub terminal_rebase: Option<TerminalRebaseDto>,
}

/// `{ "kind": "Rebase" | "Am", "branch", "onto", "currentStep", "totalSteps" }`: `branch` and
/// `onto` empty when unknown (always for `git am`), the steps 0 when unknown.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "TerminalRebase"))]
#[serde(rename_all = "camelCase")]
pub struct TerminalRebaseDto {
    pub kind: git_core::repo::TerminalRebaseKind,
    pub branch: String,
    pub onto: String,
    pub current_step: usize,
    pub total_steps: usize,
}

impl From<git_core::repo::TerminalRebase> for TerminalRebaseDto {
    fn from(rebase: git_core::repo::TerminalRebase) -> Self {
        Self {
            kind: rebase.kind,
            branch: rebase.branch,
            onto: rebase.onto,
            current_step: rebase.current_step,
            total_steps: rebase.total_steps,
        }
    }
}

/// What the user confirmed before aborting a terminal cherry-pick (`RepoClient.abortSequence`'s
/// `confirmed`): `{ "headId", "startedAt", "rewinds", "commitsToRemove", "pausedOnConflict" }`, as last shown by
/// `HeadInfo.terminalSequence` (#281 review).
#[derive(Deserialize, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ConfirmedAbort"))]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedAbortDto {
    pub head_id: String,
    pub started_at: String,
    pub rewinds: bool,
    pub commits_to_remove: usize,
    pub paused_on_conflict: bool,
}

impl From<ConfirmedAbortDto> for git_core::pick::ConfirmedAbort {
    fn from(dto: ConfirmedAbortDto) -> Self {
        Self {
            head_id: dto.head_id,
            started_at: dto.started_at,
            rewinds: dto.rewinds,
            commits_to_remove: dto.commits_to_remove,
            paused_on_conflict: dto.paused_on_conflict,
        }
    }
}

/// `{ "kind": "CherryPick" | "Revert", "pausedOnConflict", "startedAt", "startedAtSummary",
/// "rewinds", "commitsToRemove" }`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "TerminalSequence"))]
#[serde(rename_all = "camelCase")]
pub struct TerminalSequenceDto {
    pub kind: git_core::pick::SequenceKind,
    pub paused_on_conflict: bool,
    pub started_at: String,
    pub started_at_summary: String,
    pub rewinds: bool,
    pub commits_to_remove: usize,
}

impl From<git_core::pick::TerminalSequence> for TerminalSequenceDto {
    fn from(sequence: git_core::pick::TerminalSequence) -> Self {
        Self {
            kind: sequence.kind,
            paused_on_conflict: sequence.paused_on_conflict,
            started_at: sequence.started_at,
            started_at_summary: sequence.started_at_summary,
            rewinds: sequence.rewinds,
            commits_to_remove: sequence.commits_to_remove,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "BisectInfo"))]
#[serde(rename_all = "camelCase")]
pub struct BisectDto {
    /// A branch name, or a commit id when the bisect started on a detached HEAD.
    pub started_from: String,
}

impl From<git_core::repo::HeadInfo> for HeadInfoDto {
    fn from(info: git_core::repo::HeadInfo) -> Self {
        Self {
            head_id: info.head_id,
            detached_at: info.detached_at,
            head_pushed: info.head_pushed,
            operation: info.operation,
            operation_leftover: info.operation_leftover,
            squash_message: info.squash_message,
            commit_hooks: info.commit_hooks,
            sequence: info.sequence.map(SequenceProgressDto::from),
            rebase: info.rebase.map(RebaseInfoDto::from),
            bisect: info.bisect.map(|bisect| BisectDto {
                started_from: bisect.started_from,
            }),
            terminal_sequence: info.terminal_sequence.map(TerminalSequenceDto::from),
            terminal_rebase: info.terminal_rebase.map(TerminalRebaseDto::from),
        }
    }
}

/// A paused interactive rebase: `health` is `{ "kind": "Paused" }` or `{ "kind": "Diverged",
/// "reason": "HeadMoved" | "BranchMoved" | "BranchMissing" | "OtherOperation" }`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RebaseInfo"))]
#[serde(rename_all = "camelCase")]
pub struct RebaseInfoDto {
    pub branch: String,
    pub onto: String,
    pub current_step: usize,
    pub total_steps: usize,
    pub health: RebaseHealthDto,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RebaseHealth"))]
#[serde(tag = "kind", content = "reason")]
pub enum RebaseHealthDto {
    Paused,
    Diverged(git_core::rebase::RebaseDivergence),
}

impl From<git_core::repo::RebaseInfo> for RebaseInfoDto {
    fn from(info: git_core::repo::RebaseInfo) -> Self {
        use git_core::rebase::RebaseHealth;
        Self {
            branch: info.branch,
            onto: info.onto,
            current_step: info.current_step,
            total_steps: info.total_steps,
            health: match info.health {
                RebaseHealth::Paused => RebaseHealthDto::Paused,
                RebaseHealth::Diverged(reason) => RebaseHealthDto::Diverged(reason),
            },
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RebaseAbort"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "SequenceProgress"))]
#[serde(rename_all = "camelCase")]
pub struct SequenceProgressDto {
    pub kind: git_core::pick::SequenceKind,
    pub current_step: usize,
    pub total_steps: usize,
    pub current_commit: String,
}

impl From<git_core::pick::SequenceProgress> for SequenceProgressDto {
    fn from(progress: git_core::pick::SequenceProgress) -> Self {
        Self {
            kind: progress.kind,
            current_step: progress.current_step,
            total_steps: progress.total_steps,
            current_commit: progress.current_commit,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "SequenceOutcome"))]
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
    /// A commit hook failed (Continue's, #133, or a clean step's `prepare-commit-msg`, #183);
    /// the step is paused.
    HookFailed {
        hook: String,
        output: String,
        progress: SequenceProgressDto,
    },
    /// Cancel during a step's post-commit: that commit (`kept`, its summary) stands and the
    /// sequence stopped before `progress.currentCommit` (#183, D9-A).
    Stopped {
        kept: String,
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
            git_core::pick::SequenceOutcome::Stopped { kept, progress } => Self::Stopped {
                kept,
                progress: progress.into(),
            },
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "SequenceAbort"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ConflictSide"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitOutcome"))]
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

/// Payload of the `hook-progress` event / `hookProgress` notification (commit/amend/Continue,
/// checkout, merge and pick hooks; pushes and a pull's post-merge report `RunningHook` through
/// the transfer progress instead).
/// `source` (`"Commit" | "Checkout" | "Merge" | "Pick"`) tells the UI where to show it (#185).
#[derive(Clone, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "HookProgress"))]
#[serde(rename_all = "camelCase")]
pub struct HookProgressDto {
    pub repo_path: String,
    pub operation_id: String,
    pub hook: String,
    pub phase: HookProgressPhaseDto,
    pub source: HookSource,
}

/// `HookProgress.phase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "HookProgressPhase"))]
pub enum HookProgressPhaseDto {
    Started,
    Finished,
}

/// The payload of a hook event for `repo_path`: the desktop app's `hook-progress` event and
/// the sidecar's `hookProgress` notification.
pub fn hook_progress_dto(repo_path: &str, event: HookEvent) -> HookProgressDto {
    let (operation_id, hook, phase, source) = match event {
        HookEvent::Started {
            operation_id,
            hook,
            source,
        } => (operation_id, hook, HookProgressPhaseDto::Started, source),
        HookEvent::Finished {
            operation_id,
            hook,
            source,
        } => (operation_id, hook, HookProgressPhaseDto::Finished, source),
    };
    HookProgressDto {
        repo_path: repo_path.to_string(),
        operation_id,
        hook,
        phase,
        source,
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PickOutcome"))]
#[serde(tag = "kind")]
pub enum PickOutcomeDto {
    Committed {
        #[serde(rename = "commitId")]
        commit_id: String,
    },
    Conflicted {
        files: Vec<String>,
    },
    /// `prepare-commit-msg` failed; the pick stays staged and in progress (#183).
    HookFailed {
        hook: String,
        output: String,
    },
}

impl From<git_core::pick::PickOutcome> for PickOutcomeDto {
    fn from(outcome: git_core::pick::PickOutcome) -> Self {
        match outcome {
            git_core::pick::PickOutcome::Committed { commit_id } => Self::Committed { commit_id },
            git_core::pick::PickOutcome::Conflicted { files } => Self::Conflicted { files },
            git_core::pick::PickOutcome::HookFailed { hook, output } => {
                Self::HookFailed { hook, output }
            }
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "UndoState"))]
pub struct UndoStateDto {
    pub undo: Option<String>,
    pub redo: Option<String>,
}

impl From<crate::worker::UndoState> for UndoStateDto {
    fn from(state: crate::worker::UndoState) -> Self {
        Self {
            undo: state.undo,
            redo: state.redo,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "SignatureStatus"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "SignatureFormat"))]
#[serde(rename_all = "lowercase")]
pub enum SignatureFormatDto {
    Openpgp,
    Ssh,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitSignature"))]
pub struct CommitSignatureDto {
    pub status: SignatureStatusDto,
    pub format: Option<SignatureFormatDto>,
    pub signer: Option<String>,
    pub key: Option<String>,
    pub detail: Option<String>,
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "UndoOutcome"))]
#[serde(tag = "kind")]
pub enum UndoOutcomeDto {
    Done { label: String },
    Nothing,
    RepositoryChanged { label: String },
    BranchHeld { label: String, message: String },
    OperationInProgress,
}

impl From<crate::worker::UndoOutcome> for UndoOutcomeDto {
    fn from(outcome: crate::worker::UndoOutcome) -> Self {
        use crate::worker::UndoOutcome;
        match outcome {
            UndoOutcome::Done { label } => Self::Done { label },
            UndoOutcome::Nothing => Self::Nothing,
            UndoOutcome::RepositoryChanged { label } => Self::RepositoryChanged { label },
            UndoOutcome::BranchHeld { label, message } => Self::BranchHeld { label, message },
            UndoOutcome::OperationInProgress => Self::OperationInProgress,
        }
    }
}

#[derive(Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ResetMode"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FileSide"))]
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
                base64: crate::base64::encode(&bytes),
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FileSides"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "Trailer"))]
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

/// The trailers a commit or amend request carries, for the worker.
pub fn to_trailers(dtos: Vec<TrailerDto>) -> Vec<git_core::commit::Trailer> {
    dtos.into_iter().map(Into::into).collect()
}

/// A person: the amend author override (in) and a recent author (out).
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "SignatureInput"))]
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

/// Which two sides "Open in diff tool" compares (`RepoClient`'s `DiffSource`).
#[derive(Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "DiffSource"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "MarkResolvedOutcome"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ExternalToolSettings"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "GitConfigSource"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "GitToolInfo"))]
#[serde(rename_all = "camelCase")]
pub struct GitToolDto {
    pub name: String,
    pub command: Option<String>,
    pub source: GitConfigSourceDto,
    pub via_merge_tool: bool,
    pub problem: Option<String>,
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "GitTools"))]
#[serde(rename_all = "camelCase")]
pub struct GitToolsDto {
    pub diff_tool: Option<GitToolDto>,
    pub merge_tool: Option<GitToolDto>,
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "MergeOutcome"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FastForwardMode"))]
pub enum FastForwardDto {
    Allow,
    Never,
    Only,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "MergeOptions"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "StashOptions"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "StashApplyOutcome"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "StashFiles"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ConflictSegment"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FileConflictChoice"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RebasePlanCommit"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RebaseAction"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RebasePlanEntry"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RebaseStepResult"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "DiffLine"))]
pub struct DiffLineDto {
    pub origin: git_core::diff::DiffLineOrigin,
    pub content: String,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "DiffHunk"))]
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
                    origin: l.origin,
                    content: l.content,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ForgeProvider"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ForgeRepository"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PullRequest"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PullRequestList"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CreatePullRequest"))]
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

#[derive(Deserialize, Clone, Copy)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "DiffOptions"))]
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
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "DiffSettings"))]
#[serde(rename_all = "camelCase")]
pub struct DiffSettingsDto {
    pub ignore_whitespace: bool,
    pub context_lines: u32,
    pub syntax_highlight: bool,
    pub word_wrap: WordWrapDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "WordWrap"))]
pub enum WordWrapDto {
    Auto,
    On,
    Off,
}

impl From<config::WordWrap> for WordWrapDto {
    fn from(w: config::WordWrap) -> Self {
        match w {
            config::WordWrap::Auto => WordWrapDto::Auto,
            config::WordWrap::On => WordWrapDto::On,
            config::WordWrap::Off => WordWrapDto::Off,
        }
    }
}

impl From<WordWrapDto> for config::WordWrap {
    fn from(w: WordWrapDto) -> Self {
        match w {
            WordWrapDto::Auto => config::WordWrap::Auto,
            WordWrapDto::On => config::WordWrap::On,
            WordWrapDto::Off => config::WordWrap::Off,
        }
    }
}

impl From<config::DiffSettings> for DiffSettingsDto {
    fn from(s: config::DiffSettings) -> Self {
        DiffSettingsDto {
            ignore_whitespace: s.ignore_whitespace,
            context_lines: s.context_lines,
            syntax_highlight: s.syntax_highlight,
            word_wrap: s.word_wrap.into(),
        }
    }
}

impl From<DiffSettingsDto> for config::DiffSettings {
    fn from(d: DiffSettingsDto) -> Self {
        config::DiffSettings {
            ignore_whitespace: d.ignore_whitespace,
            context_lines: d.context_lines,
            syntax_highlight: d.syntax_highlight,
            word_wrap: d.word_wrap.into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitMessageSettings"))]
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

/// The `[fetch]` preferences: `autoFetchMinutes` is the app-wide "Fetch automatically"
/// interval, 0 for off.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FetchSettings"))]
#[serde(rename_all = "camelCase")]
pub struct FetchSettingsDto {
    pub auto_fetch_minutes: u32,
}

impl From<config::FetchSettings> for FetchSettingsDto {
    fn from(s: config::FetchSettings) -> Self {
        Self {
            auto_fetch_minutes: s.auto_fetch_minutes,
        }
    }
}

impl From<FetchSettingsDto> for config::FetchSettings {
    fn from(d: FetchSettingsDto) -> Self {
        Self {
            auto_fetch_minutes: d.auto_fetch_minutes,
        }
    }
}

/// The payload of a transfer event: the desktop app's `transfer-progress`/`transfer-complete`
/// events and the sidecar's `transferProgress` notification. `phase` adds `Starting`,
/// `RemoteFailed`, `Completed` and `Failed` to the `TransferPhase` names.
pub fn transfer_progress_dto(event: TransferEvent) -> TransferProgressDto {
    match event {
        TransferEvent::Started {
            operation_id,
            operation,
        } => TransferProgressDto {
            operation_id,
            operation,
            phase: TransferProgressPhaseDto::Starting,
            error_kind: None,
            current: 0,
            total: 0,
            received_bytes: 0,
            message: None,
            remote_name: None,
            hook_output: None,
        },
        TransferEvent::Progress(progress) => TransferProgressDto::from(progress),
        TransferEvent::RemoteFailed {
            operation_id,
            operation,
            remote_name,
            error,
        } => TransferProgressDto {
            operation_id,
            operation,
            phase: TransferProgressPhaseDto::RemoteFailed,
            error_kind: Some(error),
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
                operation,
                phase: if failed {
                    TransferProgressPhaseDto::Failed
                } else {
                    TransferProgressPhaseDto::Completed
                },
                error_kind: error,
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

/// What every `WorkerHandle` method answers when the repository's worker thread has stopped
/// (crashed) before it took the command; [`WORKER_STOPPED_BEFORE_REPLYING`] when it stopped while
/// the command ran. Both are [`ErrorKind::WorkerStopped`].
pub const WORKER_STOPPED: &str = "worker thread stopped";
/// See [`WORKER_STOPPED`].
pub const WORKER_STOPPED_BEFORE_REPLYING: &str = "worker thread stopped before replying";
/// What a pull answers for a failure its terminal transfer event reports with a
/// `TransferErrorKind` (a transport error may quote a URL with credentials, so the reply keeps
/// none of its text). [`ErrorKind::PullFailed`].
pub const PULL_FAILED: &str = "pull failed";

/// The kind of a rejected request, for the failures the frontend tells apart (#496). Every
/// request that fails answers an [`ErrorDto`]; the frontend switches on its `kind` and never on
/// the message text, which stays free to change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
pub enum ErrorKind {
    /// Anything the frontend shows as it is.
    Other,
    /// The repository's worker thread has stopped; nothing on this repository works until it is
    /// closed and reopened ([`WORKER_STOPPED`]).
    WorkerStopped,
    /// The operating-system credential store failed while saving or forgetting an HTTPS token,
    /// or while changing a remote's authentication.
    CredentialStoreFailure,
    /// A pull refused to start over uncommitted changes.
    DirtyWorktree,
    /// A pull failed and its terminal transfer event says why ([`PULL_FAILED`]).
    PullFailed,
    /// The user cancelled the transfer (a pull's); its terminal transfer event says so too.
    Cancelled,
    /// `get_conflict_hunks` on an add/delete conflict, which has no text hunks.
    AddDeleteConflict,
}

impl ErrorKind {
    /// The kind of `message`, a request's error text. Each kind is recognised by the constant or
    /// typed error its producer formats, never by a copy of the text, so rewording one keeps its
    /// kind. Exact matches only, apart from the parts the producer fills in.
    pub fn of(message: &str) -> Self {
        use git_core::remote::RemoteError;
        if message == WORKER_STOPPED || message == WORKER_STOPPED_BEFORE_REPLYING {
            Self::WorkerStopped
        } else if message == PULL_FAILED {
            Self::PullFailed
        } else if message == RemoteError::DirtyWorktree.to_string() {
            Self::DirtyWorktree
        } else if message == RemoteError::Cancelled.to_string() {
            Self::Cancelled
        } else if fills(message, |diagnostic| {
            credential_store_failure_message(&diagnostic)
        }) {
            Self::CredentialStoreFailure
        } else if fills(message, |path| {
            git_core::merge::MergeError::NotATextConflict(path).to_string()
        }) {
            Self::AddDeleteConflict
        } else {
            Self::Other
        }
    }
}

/// The error text of a credential store failure while saving or forgetting a token, or while
/// changing a remote's authentication: [`ErrorKind::CredentialStoreFailure`].
pub fn credential_store_failure_message(diagnostic: &str) -> String {
    format!(
        "{}: {diagnostic}",
        git_core::remote::CREDENTIAL_STORE_FAILURE_ERROR
    )
}

/// Whether `message` is what `render` makes of some value: the text around the value must
/// match exactly.
fn fills(message: &str, render: impl Fn(String) -> String) -> bool {
    const SLOT: char = '\u{0}';
    let template = render(SLOT.to_string());
    let Some((before, after)) = template.split_once(SLOT) else {
        return message == template;
    };
    message.len() >= before.len() + after.len()
        && message.starts_with(before)
        && message.ends_with(after)
}

/// A rejected request (#496): the desktop app sends it as the command's error, the sidecar as
/// the JSON-RPC error's `data` (next to the same `message`). Built from the error text with
/// [`ErrorKind::of`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RepoErrorPayload"))]
pub struct ErrorDto {
    pub kind: ErrorKind,
    pub message: String,
}

impl From<String> for ErrorDto {
    fn from(message: String) -> Self {
        Self {
            kind: ErrorKind::of(&message),
            message,
        }
    }
}

impl From<&str> for ErrorDto {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

/// #306: the TypeScript shape of every wire DTO above, checked in as
/// `frontend/src/ipc/generated/wire-dto.ts`. `frontend/src/ipc/dtoContract.ts` checks it against
/// the hand-written types in `RepoClient.ts`, so a renamed field fails the frontend type check.
/// Regenerate with `UPDATE_TRANSPORT_METHODS=1 cargo test -p repo-service wire_bindings_are_current`.
#[cfg(test)]
mod ts_bindings {
    use super::*;

    #[test]
    fn wire_bindings_are_current() {
        let bindings = test_support::ts_declarations!(
            "// Generated from the shared wire DTOs (crates/repo-service/src/wire.rs) by ts-rs.\n\
             // Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p repo-service wire_bindings_are_current\n";
            git_core::diff::DiffLineOrigin,
            git_core::pick::SequenceKind,
            git_core::rebase::RebaseDivergence,
            git_core::remote::TransferErrorKind,
            git_core::remote::TransferOperation,
            git_core::repo::OperationKind,
            git_core::repo::TerminalRebaseKind,
            git_core::status::StatusKind,
            ErrorKind,
            ErrorDto,
            crate::worker::HookSource,
            TransferProgressDto,
            TransferProgressPhaseDto,
            StatusEntryDto,
            BranchInfoDto,
            WorktreeInfoDto,
            WorkspaceDto,
            OpenRepoEntryDto,
            OpenRepoEntryInput,
            SubmoduleInfoDto,
            ReflogEntryDto,
            RemoteInfoDto,
            RemoteAuthModeDto,
            TagInfoDto,
            UpstreamInfoDto,
            PullOutcomeDto,
            StashEntryDto,
            BlameLineDto,
            GraphCommitDto,
            FileHistoryEntryDto,
            SearchPageDto,
            ChangedFileDto,
            CompareRangeDto,
            CommitLocationDto,
            HeadInfoDto,
            TerminalRebaseDto,
            ConfirmedAbortDto,
            TerminalSequenceDto,
            BisectDto,
            RebaseInfoDto,
            RebaseHealthDto,
            RebaseAbortDto,
            SequenceProgressDto,
            SequenceOutcomeDto,
            SequenceAbortDto,
            ConflictSideDto,
            CommitOutcomeDto,
            HookProgressDto,
            HookProgressPhaseDto,
            PickOutcomeDto,
            UndoStateDto,
            SignatureStatusDto,
            SignatureFormatDto,
            CommitSignatureDto,
            UndoOutcomeDto,
            ResetModeDto,
            FileSideDto,
            FileSidesDto,
            TrailerDto,
            SignatureInputDto,
            DiffSourceDto,
            MarkResolvedOutcomeDto,
            ExternalToolSettingsDto,
            GitConfigSourceDto,
            GitToolDto,
            GitToolsDto,
            MergeOutcomeDto,
            FastForwardDto,
            MergeOptionsDto,
            StashOptionsDto,
            StashApplyOutcomeDto,
            StashFilesDto,
            ConflictSegmentDto,
            FileConflictChoiceDto,
            RebasePlanCommitDto,
            RebaseActionDto,
            RebasePlanEntryDto,
            RebaseStepResultDto,
            DiffLineDto,
            DiffHunkDto,
            ForgeProviderDto,
            ForgeRepositoryDto,
            PullRequestDto,
            PullRequestListDto,
            CreatePullRequestDto,
            DiffOptionsDto,
            DiffSettingsDto,
            WordWrapDto,
            CommitMessageSettingsDto,
            FetchSettingsDto,
        );
        test_support::assert_generated_file(
            &test_support::repo_root().join("frontend/src/ipc/generated/wire-dto.ts"),
            &bindings,
        );
    }
}

/// #496: each `ErrorKind` comes from its producer's own text, and nothing else gets one.
#[cfg(test)]
mod error_kinds {
    use super::*;
    use git_core::merge::MergeError;
    use git_core::remote::RemoteError;

    #[test]
    fn each_producer_gets_its_kind() {
        let cases = [
            (WORKER_STOPPED.to_string(), ErrorKind::WorkerStopped),
            (
                WORKER_STOPPED_BEFORE_REPLYING.to_string(),
                ErrorKind::WorkerStopped,
            ),
            (PULL_FAILED.to_string(), ErrorKind::PullFailed),
            (
                RemoteError::DirtyWorktree.to_string(),
                ErrorKind::DirtyWorktree,
            ),
            (RemoteError::Cancelled.to_string(), ErrorKind::Cancelled),
            (
                credential_store_failure_message("CredWrite failed: locked"),
                ErrorKind::CredentialStoreFailure,
            ),
            (
                MergeError::NotATextConflict("dir/it's: here.bin".into()).to_string(),
                ErrorKind::AddDeleteConflict,
            ),
        ];
        for (message, kind) in cases {
            assert_eq!(ErrorKind::of(&message), kind, "{message}");
        }
    }

    /// Texts that only contain a recognised one, or only look like it, stay `Other`. The
    /// frontend's old substring checks matched the first two.
    #[test]
    fn near_misses_stay_other() {
        let messages = [
            MergeError::NotAnAddDeleteConflict("x".into()).to_string(),
            format!("hook said: {}", RemoteError::DirtyWorktree),
            format!("{WORKER_STOPPED}!"),
            "Pull failed".to_string(),
            git_core::remote::CREDENTIAL_STORE_FAILURE_ERROR.to_string(),
            RemoteError::NoUpstream.to_string(),
            String::new(),
        ];
        for message in messages {
            assert_eq!(ErrorKind::of(&message), ErrorKind::Other, "{message}");
        }
    }

    #[test]
    fn the_envelope_keeps_the_message() {
        let error = ErrorDto::from(WORKER_STOPPED);

        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"kind":"WorkerStopped","message":"worker thread stopped"}"#
        );
    }
}
