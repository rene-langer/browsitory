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
//! test below writes `frontend/src/ipc/generated/wire-dto.ts` from them, and
//! `wire-constants.ts` from the backend's limits and settings defaults (#497).

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
    /// Only on an untracked folder listed as one entry (#845): `true`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub untracked_folder: Option<bool>,
}

impl From<git_core::status::StatusEntry> for StatusEntryDto {
    fn from(entry: git_core::status::StatusEntry) -> Self {
        Self {
            path: entry.path,
            old_path: entry.old_path,
            staged: entry.staged,
            kind: entry.kind,
            untracked_folder: entry.untracked_folder.then_some(true),
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

/// A repository's custom tab name (#625), from `config::RepoAlias`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RepoAlias"))]
#[serde(rename_all = "camelCase")]
pub struct RepoAliasDto {
    pub repo_path: String,
    pub alias: String,
}

impl From<config::RepoAlias> for RepoAliasDto {
    fn from(alias: config::RepoAlias) -> Self {
        Self {
            repo_path: alias.repo_path.to_string_lossy().into_owned(),
            alias: alias.alias,
        }
    }
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
    FastForwarded {
        upstream_ref: String,
    },
    Diverged {
        upstream_ref: String,
    },
    /// Could fast-forward, but git config asks for a merge commit (#740); nothing changed.
    NoFastForward {
        upstream_ref: String,
    },
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
            git_core::remote::PullOutcome::NoFastForward { upstream_ref } => {
                PullOutcomeDto::NoFastForward { upstream_ref }
            }
        }
    }
}

/// How a diverged pull reconciles (#652): what `get_pull_strategy` answers (the choice
/// remembered for the repository, else its git config, else `null`) and `set_pull_strategy`
/// takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PullStrategy"))]
pub enum PullStrategyDto {
    Merge,
    Rebase,
    FastForwardOnly,
}

impl From<git_core::remote::PullStrategy> for PullStrategyDto {
    fn from(strategy: git_core::remote::PullStrategy) -> Self {
        use git_core::remote::PullStrategy;
        match strategy {
            PullStrategy::Merge => Self::Merge,
            PullStrategy::Rebase => Self::Rebase,
            PullStrategy::FastForwardOnly => Self::FastForwardOnly,
        }
    }
}

impl From<config::PullStrategyPreference> for PullStrategyDto {
    fn from(strategy: config::PullStrategyPreference) -> Self {
        use config::PullStrategyPreference;
        match strategy {
            PullStrategyPreference::Merge => Self::Merge,
            PullStrategyPreference::Rebase => Self::Rebase,
            PullStrategyPreference::FastForwardOnly => Self::FastForwardOnly,
        }
    }
}

impl From<PullStrategyDto> for config::PullStrategyPreference {
    fn from(strategy: PullStrategyDto) -> Self {
        match strategy {
            PullStrategyDto::Merge => Self::Merge,
            PullStrategyDto::Rebase => Self::Rebase,
            PullStrategyDto::FastForwardOnly => Self::FastForwardOnly,
        }
    }
}

/// What "Stash and switch" did with the stashed changes (#653).
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "StashSwitchOutcome"))]
#[serde(tag = "kind")]
pub enum StashSwitchOutcomeDto {
    Stashed,
    Reapplied,
    Conflicted { files: Vec<String> },
    NotReapplied { reason: String },
}

impl From<git_core::branch::StashSwitchOutcome> for StashSwitchOutcomeDto {
    fn from(outcome: git_core::branch::StashSwitchOutcome) -> Self {
        use git_core::branch::StashSwitchOutcome;
        match outcome {
            StashSwitchOutcome::Stashed => Self::Stashed,
            StashSwitchOutcome::Reapplied => Self::Reapplied,
            StashSwitchOutcome::Conflicted { files } => Self::Conflicted { files },
            StashSwitchOutcome::NotReapplied { reason } => Self::NotReapplied { reason },
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
    pub branch: Option<String>,
    pub timestamp: i64,
    pub file_count: usize,
    pub has_staged_changes: bool,
}

impl From<git_core::stash::StashEntry> for StashEntryDto {
    fn from(s: git_core::stash::StashEntry) -> Self {
        StashEntryDto {
            index: s.index,
            message: s.message,
            commit_id: s.commit_id,
            branch: s.branch,
            timestamp: s.timestamp,
            file_count: s.file_count,
            has_staged_changes: s.has_staged_changes,
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
    /// Only on a merge row of a first-parent graph (#617): the commits its other parents brought
    /// in, which `getMergedCommits` lists. Left out everywhere else.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub merged_count: Option<usize>,
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
            merged_count: c.merged_count,
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

/// One answer of `get_file_history` (#860).
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FileHistoryPage"))]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryPageDto {
    /// The first `limit` entries, or fewer when the call's time budget ran out first.
    pub entries: Vec<FileHistoryEntryDto>,
    /// The file's history ended: no older entry exists.
    pub complete: bool,
}

impl From<git_core::history::FileHistoryPage> for FileHistoryPageDto {
    fn from(page: git_core::history::FileHistoryPage) -> Self {
        FileHistoryPageDto {
            entries: page.entries.into_iter().map(Into::into).collect(),
            complete: page.complete,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "LineHistoryEntry"))]
#[serde(rename_all = "camelCase")]
pub struct LineHistoryEntryDto {
    pub commit: GraphCommitDto,
    pub path: String,
    // Serialized as `null` (not omitted) — the TS type is `string | null`.
    pub old_path: Option<String>,
    /// The tracked lines in this commit's version of the file, 1-based and inclusive.
    pub start_line: usize,
    pub end_line: usize,
}

impl From<git_core::line_history::LineHistoryEntry> for LineHistoryEntryDto {
    fn from(e: git_core::line_history::LineHistoryEntry) -> Self {
        LineHistoryEntryDto {
            commit: GraphCommitDto::from(e.commit),
            path: e.path,
            old_path: e.old_path,
            start_line: e.lines.start,
            end_line: e.lines.end,
        }
    }
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "LineHistory"))]
#[serde(rename_all = "camelCase")]
pub struct LineHistoryDto {
    pub entries: Vec<LineHistoryEntryDto>,
    /// The walk stopped at its commit cap, so older changes may be missing.
    pub truncated: bool,
    /// No older entry will come: the range's history ended, or the walk stopped at its cap.
    /// Fewer than `limit` entries without it: the call's time budget ran out first (#860).
    pub complete: bool,
}

impl From<git_core::line_history::LineHistory> for LineHistoryDto {
    fn from(h: git_core::line_history::LineHistory) -> Self {
        LineHistoryDto {
            entries: h.entries.into_iter().map(Into::into).collect(),
            truncated: h.truncated,
            complete: h.complete,
        }
    }
}

/// One page of the commit graph (#685).
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "GraphPage"))]
#[serde(rename_all = "camelCase")]
pub struct GraphPageDto {
    pub commits: Vec<GraphCommitDto>,
    /// `null` once the walk is done.
    pub next_cursor: Option<String>,
    /// The cursor belonged to another walk: no commits, load the graph from the start.
    pub stale: bool,
    /// The walk and the badges' version the rows come from: a refresh passes them back as
    /// `LoadedGraph` (#835).
    pub walk: String,
    pub badges: String,
    /// Only for a refresh with `loaded` that could continue the loaded rows: how the refreshed
    /// rows are made of `commits` and those rows. `null`: `commits` are the rows.
    pub refresh: Option<GraphRefreshDto>,
}

impl From<git_core::graph::GraphPage> for GraphPageDto {
    fn from(p: git_core::graph::GraphPage) -> Self {
        GraphPageDto {
            commits: p.commits.into_iter().map(GraphCommitDto::from).collect(),
            next_cursor: p.next_cursor,
            stale: p.stale,
            walk: p.walk,
            badges: p.badges,
            refresh: p.refresh.map(GraphRefreshDto::from),
        }
    }
}

/// How a refresh's rows are made (#835): `layout` in order, then `badges` replaces the badges
/// of kept rows whose branches or tags changed. See `git_core::graph::GraphRefresh`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "GraphRefresh"))]
#[serde(rename_all = "camelCase")]
pub struct GraphRefreshDto {
    pub layout: Vec<GraphSegmentDto>,
    pub badges: Vec<RowBadgesDto>,
}

impl From<git_core::graph::GraphRefresh> for GraphRefreshDto {
    fn from(r: git_core::graph::GraphRefresh) -> Self {
        GraphRefreshDto {
            layout: r.layout.into_iter().map(GraphSegmentDto::from).collect(),
            badges: r.badges.into_iter().map(RowBadgesDto::from).collect(),
        }
    }
}

/// `New`: the next `count` rows of the page's `commits`; `Kept`: the loaded rows
/// `from..from + count`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "GraphSegment"))]
#[serde(tag = "kind")]
pub enum GraphSegmentDto {
    New { count: usize },
    Kept { from: usize, count: usize },
}

impl From<git_core::graph::GraphSegment> for GraphSegmentDto {
    fn from(segment: git_core::graph::GraphSegment) -> Self {
        match segment {
            git_core::graph::GraphSegment::New { count } => Self::New { count },
            git_core::graph::GraphSegment::Kept { from, count } => Self::Kept { from, count },
        }
    }
}

/// A kept row's badges as they are now (#835), named as on `GraphCommit`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RowBadges"))]
#[serde(rename_all = "camelCase")]
pub struct RowBadgesDto {
    pub id: String,
    pub branch_refs: Vec<String>,
    pub remote_branch_refs: Vec<String>,
    pub tags: Vec<String>,
}

impl From<git_core::graph::RowBadges> for RowBadgesDto {
    fn from(b: git_core::graph::RowBadges) -> Self {
        RowBadgesDto {
            id: b.id,
            branch_refs: b.branch_refs,
            remote_branch_refs: b.remote_branch_refs,
            tags: b.tags,
        }
    }
}

/// The rows a refresh already has (#835), passed as `loaded` to `getCommitGraph`: the `walk`
/// and `badges` of the pages they came from, and how many there are.
#[derive(Deserialize, Clone)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "LoadedGraph"))]
pub struct LoadedGraphDto {
    pub walk: String,
    pub badges: String,
    pub rows: usize,
}

impl From<LoadedGraphDto> for git_core::graph::LoadedGraph {
    fn from(dto: LoadedGraphDto) -> Self {
        git_core::graph::LoadedGraph {
            walk: dto.walk,
            badges: dto.badges,
            rows: dto.rows,
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

/// The branches the commit graph walks, passed as one `selection` argument to `getCommitGraph`,
/// `locateCommit` and `searchCommits` (#498): see `git_core::graph::GraphSelection`.
#[derive(Deserialize, Clone)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "GraphSelection"))]
pub struct GraphSelectionDto {
    /// Local branch names: `null` walks every local branch, `[]` none of them.
    pub local: Option<Vec<String>>,
    /// Remote-tracking branches as `<remote>/<branch>`; required, `[]` walks no remote branch.
    pub remote: Vec<String>,
    /// "Collapse merges" (#617): the graph and `locateCommit` follow first parents from every
    /// selected tip; `searchCommits` ignores it. Absent is `false`.
    #[serde(default, rename = "firstParent")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub first_parent: Option<bool>,
}

impl From<GraphSelectionDto> for git_core::graph::GraphSelection {
    fn from(dto: GraphSelectionDto) -> Self {
        git_core::graph::GraphSelection {
            local: dto.local,
            remote: dto.remote,
            first_parent: dto.first_parent.unwrap_or(false),
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
    /// The collapsed merge that holds the commit (#617); left out when there is none.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(any(test, feature = "ts"), ts(optional))]
    pub merge: Option<CollapsedMergeDto>,
}

impl From<git_core::locate::CommitLocation> for CommitLocationDto {
    fn from(l: git_core::locate::CommitLocation) -> Self {
        CommitLocationDto {
            commit_id: l.commit_id,
            position: l.position,
            branches: l.branches,
            remote_branches: l.remote_branches,
            merge: l.merge.map(CollapsedMergeDto::from),
        }
    }
}

/// See `git_core::locate::CollapsedMerge` (#617).
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CollapsedMerge"))]
#[serde(rename_all = "camelCase")]
pub struct CollapsedMergeDto {
    pub merge_id: String,
    pub merge_position: usize,
    pub offset: usize,
}

impl From<git_core::locate::CollapsedMerge> for CollapsedMergeDto {
    fn from(m: git_core::locate::CollapsedMerge) -> Self {
        CollapsedMergeDto {
            merge_id: m.merge_id,
            merge_position: m.merge_position,
            offset: m.offset,
        }
    }
}

/// One file's line counts in a commit (#620): see `git_core::commit_stats::FileStats`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FileStats"))]
#[serde(rename_all = "camelCase")]
pub struct FileStatsDto {
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
    pub binary: bool,
}

impl From<git_core::commit_stats::FileStats> for FileStatsDto {
    fn from(f: git_core::commit_stats::FileStats) -> Self {
        FileStatsDto {
            path: f.path,
            insertions: f.insertions,
            deletions: f.deletions,
            binary: f.binary,
        }
    }
}

/// A commit's line counts (#620): see `git_core::commit_stats::CommitStats`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitStats"))]
#[serde(rename_all = "camelCase")]
pub struct CommitStatsDto {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    /// Empty unless `withFiles` asked for them.
    pub files: Vec<FileStatsDto>,
}

impl From<git_core::commit_stats::CommitStats> for CommitStatsDto {
    fn from(s: git_core::commit_stats::CommitStats) -> Self {
        CommitStatsDto {
            files_changed: s.files_changed,
            insertions: s.insertions,
            deletions: s.deletions,
            files: s.files.into_iter().map(FileStatsDto::from).collect(),
        }
    }
}

/// One answered commit of `getCommitStats` (#620): `stats` is `null` for a commit over the cap
/// or one that couldn't be read.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitStatsEntry"))]
#[serde(rename_all = "camelCase")]
pub struct CommitStatsEntryDto {
    pub commit_id: String,
    pub stats: Option<CommitStatsDto>,
}

impl From<git_core::commit_stats::CommitStatsEntry> for CommitStatsEntryDto {
    fn from(e: git_core::commit_stats::CommitStatsEntry) -> Self {
        CommitStatsEntryDto {
            commit_id: e.commit_id,
            stats: e.stats.map(CommitStatsDto::from),
        }
    }
}

/// An author or committer of a commit (#641): see `git_core::commit_details::Person`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitPerson"))]
#[serde(rename_all = "camelCase")]
pub struct CommitPersonDto {
    pub name: String,
    pub email: String,
    pub timestamp: i64,
}

impl From<git_core::commit_details::Person> for CommitPersonDto {
    fn from(p: git_core::commit_details::Person) -> Self {
        CommitPersonDto {
            name: p.name,
            email: p.email,
            timestamp: p.timestamp,
        }
    }
}

/// `getCommitDetails` (#641): see `git_core::commit_details::CommitDetails`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitDetails"))]
#[serde(rename_all = "camelCase")]
pub struct CommitDetailsDto {
    pub author: CommitPersonDto,
    pub committer: CommitPersonDto,
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub tags: Vec<String>,
    pub complete: bool,
}

impl From<git_core::commit_details::CommitDetails> for CommitDetailsDto {
    fn from(d: git_core::commit_details::CommitDetails) -> Self {
        CommitDetailsDto {
            author: d.author.into(),
            committer: d.committer.into(),
            branches: d.branches,
            remote_branches: d.remote_branches,
            tags: d.tags,
            complete: d.complete,
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
    /// Full id of the commit an Edit step stopped at (#882); `null` at any other stop.
    pub editing: Option<String>,
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
            editing: info.editing,
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

/// What applying a patch file did (#747): `HookFailed` when `applypatch-msg` or
/// `pre-applypatch` rejected the mail `subject` (nothing was applied), with the hook's output.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchApplyOutcome"))]
#[serde(tag = "kind")]
pub enum PatchApplyOutcomeDto {
    Applied,
    HookFailed {
        hook: String,
        output: String,
        subject: String,
    },
}

impl From<git_core::patch::PatchApplyOutcome> for PatchApplyOutcomeDto {
    fn from(outcome: git_core::patch::PatchApplyOutcome) -> Self {
        match outcome {
            git_core::patch::PatchApplyOutcome::Applied => Self::Applied,
            git_core::patch::PatchApplyOutcome::HookFailed {
                hook,
                output,
                subject,
            } => Self::HookFailed {
                hook,
                output,
                subject,
            },
        }
    }
}

/// Payload of the `hook-progress` event / `hookProgress` notification (commit/amend/Continue,
/// checkout, merge and pick hooks; pushes and a pull's post-merge report `RunningHook` through
/// the transfer progress instead).
/// `source` (`"Commit" | "Checkout" | "Merge" | "Pick" | "Patch"`) tells the UI where to show it (#185).
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

/// Payload of the `repo-watch` event / `repoWatch` notification (#756): the watcher of the
/// repository the frontend watches as `repo_path` saw a change (`Changed`), or stopped
/// (`Stopped`) and the frontend should poll instead.
#[derive(Clone, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RepoWatch"))]
#[serde(rename_all = "camelCase")]
pub struct RepoWatchDto {
    pub repo_path: String,
    pub kind: crate::watch::WatchEvent,
}

/// The payload of a watcher event for `repo_path`: the desktop app's `repo-watch` event and the
/// sidecar's `repoWatch` notification.
pub fn repo_watch_dto(repo_path: &str, event: crate::watch::WatchEvent) -> RepoWatchDto {
    RepoWatchDto {
        repo_path: repo_path.to_string(),
        kind: event,
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

/// A repository's change stamp (#650) in its two parts (#685); see `git_core::change_stamp`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "ChangeStamp"))]
#[serde(rename_all = "camelCase")]
pub struct ChangeStampDto {
    pub refs: String,
    pub work_tree: String,
}

/// A refresh's status and change stamp from one scan (#841); see
/// `git_core::change_stamp::status_with_stamp`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "StatusWithStamp"))]
#[serde(rename_all = "camelCase")]
pub struct StatusWithStampDto {
    pub status: Vec<StatusEntryDto>,
    pub stamp: ChangeStampDto,
}

impl From<git_core::change_stamp::StatusWithStamp> for StatusWithStampDto {
    fn from(read: git_core::change_stamp::StatusWithStamp) -> Self {
        Self {
            status: read.status.into_iter().map(StatusEntryDto::from).collect(),
            stamp: read.stamp.into(),
        }
    }
}

impl From<git_core::change_stamp::ChangeStamp> for ChangeStampDto {
    fn from(stamp: git_core::change_stamp::ChangeStamp) -> Self {
        Self {
            refs: stamp.refs,
            work_tree: stamp.work_tree,
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

/// One entry of a commit's tree (#446).
#[derive(Debug, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "TreeEntry"))]
#[serde(rename_all = "camelCase")]
pub struct TreeEntryDto {
    pub path: String,
    pub name: String,
    pub kind: git_core::tree::TreeEntryKind,
    pub size: Option<u64>,
}

impl From<git_core::tree::TreeEntry> for TreeEntryDto {
    fn from(entry: git_core::tree::TreeEntry) -> Self {
        Self {
            path: entry.path,
            name: entry.name,
            kind: entry.kind,
            size: entry.size,
        }
    }
}

/// One folder of a commit's tree, or a filter's matches (`truncated`: there were more).
#[derive(Debug, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "TreeListing"))]
pub struct TreeListingDto {
    pub entries: Vec<TreeEntryDto>,
    pub truncated: bool,
}

impl From<git_core::tree::TreeListing> for TreeListingDto {
    fn from(listing: git_core::tree::TreeListing) -> Self {
        Self {
            entries: listing.entries.into_iter().map(Into::into).collect(),
            truncated: listing.truncated,
        }
    }
}

/// A file's content at a commit for the read-only viewer (#446). A binary file's bytes are sent
/// only when they are a previewable image (`Image`, the type sniffed from the bytes).
#[derive(Debug, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FileContent"))]
#[serde(tag = "kind")]
pub enum FileContentDto {
    Text {
        size: u64,
        text: String,
    },
    Image {
        size: u64,
        base64: String,
        mime: &'static str,
    },
    Binary {
        size: u64,
    },
    TooLarge {
        size: u64,
    },
}

impl From<git_core::tree::FileContent> for FileContentDto {
    fn from(content: git_core::tree::FileContent) -> Self {
        use git_core::tree::FileContent;
        match content {
            FileContent::Text { text, size } => Self::Text { size, text },
            FileContent::TooLarge { size } => Self::TooLarge { size },
            FileContent::Binary { bytes } => {
                let size = bytes.len() as u64;
                match git_core::diff::sniff_image_mime(&bytes) {
                    Some(mime) => Self::Image {
                        size,
                        base64: crate::base64::encode(&bytes),
                        mime,
                    },
                    None => Self::Binary { size },
                }
            }
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
    Clean {
        content: String,
    },
    Conflict {
        ours: String,
        base: String,
        theirs: String,
    },
}

impl From<git_core::merge::ConflictSegment> for ConflictSegmentDto {
    fn from(segment: git_core::merge::ConflictSegment) -> Self {
        match segment {
            git_core::merge::ConflictSegment::Clean { content } => {
                ConflictSegmentDto::Clean { content }
            }
            git_core::merge::ConflictSegment::Conflict { ours, base, theirs } => {
                ConflictSegmentDto::Conflict { ours, base, theirs }
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

/// `RepoClient.startRebase`'s optional `edit`: the one-click history edit from the commit menu
/// the rebase runs, `{ "kind", "commitId" }`, so its undo entry names it (#473).
#[derive(Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "HistoryEditTarget"))]
#[serde(rename_all = "camelCase")]
pub struct HistoryEditTargetDto {
    pub kind: HistoryEditKindDto,
    pub commit_id: String,
}

#[derive(Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "HistoryEditKind"))]
pub enum HistoryEditKindDto {
    Reword,
    Squash,
    Fixup,
    MoveUp,
    MoveDown,
    Drop,
    Edit,
}

impl From<HistoryEditTargetDto> for crate::worker::HistoryEdit {
    fn from(dto: HistoryEditTargetDto) -> Self {
        use crate::worker::HistoryEditKind as Kind;
        Self {
            kind: match dto.kind {
                HistoryEditKindDto::Reword => Kind::Reword,
                HistoryEditKindDto::Squash => Kind::Squash,
                HistoryEditKindDto::Fixup => Kind::Fixup,
                HistoryEditKindDto::MoveUp => Kind::MoveUp,
                HistoryEditKindDto::MoveDown => Kind::MoveDown,
                HistoryEditKindDto::Drop => Kind::Drop,
                HistoryEditKindDto::Edit => Kind::Edit,
            },
            commit_id: dto.commit_id,
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
    pub algorithm: DiffAlgorithmDto,
}

impl From<DiffOptionsDto> for git_core::diff::DiffOptions {
    fn from(dto: DiffOptionsDto) -> Self {
        git_core::diff::DiffOptions {
            ignore_whitespace: dto.ignore_whitespace,
            context_lines: dto.context_lines,
            algorithm: dto.algorithm.into(),
        }
    }
}

/// The diff algorithm (#469): a diff setting, sent with every diff and with every hunk or line
/// action, which re-diffs with the displayed diff's algorithm to find the hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "DiffAlgorithm"))]
pub enum DiffAlgorithmDto {
    Default,
    Patience,
    Minimal,
}

impl From<DiffAlgorithmDto> for git_core::diff::DiffAlgorithm {
    fn from(a: DiffAlgorithmDto) -> Self {
        match a {
            DiffAlgorithmDto::Default => git_core::diff::DiffAlgorithm::Default,
            DiffAlgorithmDto::Patience => git_core::diff::DiffAlgorithm::Patience,
            DiffAlgorithmDto::Minimal => git_core::diff::DiffAlgorithm::Minimal,
        }
    }
}

impl From<config::DiffAlgorithm> for DiffAlgorithmDto {
    fn from(a: config::DiffAlgorithm) -> Self {
        match a {
            config::DiffAlgorithm::Default => DiffAlgorithmDto::Default,
            config::DiffAlgorithm::Patience => DiffAlgorithmDto::Patience,
            config::DiffAlgorithm::Minimal => DiffAlgorithmDto::Minimal,
        }
    }
}

impl From<DiffAlgorithmDto> for config::DiffAlgorithm {
    fn from(a: DiffAlgorithmDto) -> Self {
        match a {
            DiffAlgorithmDto::Default => config::DiffAlgorithm::Default,
            DiffAlgorithmDto::Patience => config::DiffAlgorithm::Patience,
            DiffAlgorithmDto::Minimal => config::DiffAlgorithm::Minimal,
        }
    }
}

/// The options a displayed hunk was read with; hunk and line actions re-diff with them.
pub fn hunk_options(
    context_lines: u32,
    algorithm: DiffAlgorithmDto,
) -> git_core::diff::HunkOptions {
    git_core::diff::HunkOptions {
        context_lines,
        algorithm: algorithm.into(),
    }
}

/// A hunk of a commit's diff as the commit view showed it (#448): `(oldStart, newStart)` read
/// with `contextLines` and `algorithm`, whitespace significant.
#[derive(Deserialize, Clone, Copy)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitHunk"))]
#[serde(rename_all = "camelCase")]
pub struct CommitHunkDto {
    pub old_start: u32,
    pub new_start: u32,
    pub context_lines: u32,
    pub algorithm: DiffAlgorithmDto,
}

/// A commit's file, or one hunk of it, to revert or apply in the working tree (#448).
#[derive(Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "CommitChangeRequest"))]
#[serde(rename_all = "camelCase")]
pub struct CommitChangeRequestDto {
    pub commit_id: String,
    pub path: String,
    pub old_path: Option<String>,
    /// `null`: the whole file.
    pub hunk: Option<CommitHunkDto>,
    /// Revert the change rather than apply it.
    pub reverse: bool,
    /// Also stage the result.
    pub stage: bool,
}

impl From<CommitChangeRequestDto> for crate::worker::CommitChange {
    fn from(dto: CommitChangeRequestDto) -> Self {
        Self {
            commit_id: dto.commit_id,
            path: dto.path,
            old_path: dto.old_path,
            hunk: dto.hunk.map(|h| git_core::patch::CommitHunk {
                old_start: h.old_start,
                new_start: h.new_start,
                options: hunk_options(h.context_lines, h.algorithm),
            }),
            reverse: dto.reverse,
            stage: dto.stage,
        }
    }
}

/// What "Save as patch…" writes (#448).
#[derive(Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchSource"))]
#[serde(tag = "kind")]
pub enum PatchSourceDto {
    /// Oldest first.
    #[serde(rename_all = "camelCase")]
    Commits {
        commit_ids: Vec<String>,
    },
    #[serde(rename_all = "camelCase")]
    CommitFile {
        commit_id: String,
        path: String,
        old_path: Option<String>,
    },
    Staged,
}

impl From<PatchSourceDto> for crate::worker::PatchSource {
    fn from(dto: PatchSourceDto) -> Self {
        match dto {
            PatchSourceDto::Commits { commit_ids } => Self::Commits(commit_ids),
            PatchSourceDto::CommitFile {
                commit_id,
                path,
                old_path,
            } => Self::CommitFile {
                commit_id,
                path,
                old_path,
            },
            PatchSourceDto::Staged => Self::Staged,
        }
    }
}

/// A patch's bytes (#448), base64 because a patch may hold any bytes (file content in another
/// encoding).
#[derive(Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchContent"))]
pub struct PatchContentDto {
    pub base64: String,
}

impl PatchContentDto {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            base64: crate::base64::encode(bytes),
        }
    }

    /// The bytes, or an error for text that isn't base64.
    pub fn into_bytes(self) -> Result<Vec<u8>, String> {
        crate::base64::decode(&self.base64)
            .ok_or_else(|| "The patch content is not valid base64.".to_string())
    }
}

/// How "Apply patch…" applies a patch file (#658).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchApply"))]
pub enum PatchApplyDto {
    /// To the working tree, as unstaged changes.
    WorkTree,
    /// To the working tree and the index.
    Stage,
    /// Each mail as a commit with its author, date and message.
    Commits,
}

impl From<PatchApplyDto> for git_core::patch::PatchApply {
    fn from(dto: PatchApplyDto) -> Self {
        match dto {
            PatchApplyDto::WorkTree => Self::WorkTree,
            PatchApplyDto::Stage => Self::Stage,
            PatchApplyDto::Commits => Self::Commits,
        }
    }
}

/// What a patch file holds and whether it applies (#658): `git_core::patch::PatchPreview`.
#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchPreview"))]
#[serde(rename_all = "camelCase")]
pub struct PatchPreviewDto {
    /// One per mail, in order (one for a plain diff).
    pub parts: Vec<PatchPreviewPartDto>,
    /// Why it doesn't apply to the working tree; only the first mail is checked against it.
    pub problem: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchPreviewPart"))]
#[serde(rename_all = "camelCase")]
pub struct PatchPreviewPartDto {
    /// `null` for a plain diff.
    pub mail: Option<PatchMailDto>,
    pub files: Vec<PatchPreviewFileDto>,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchMail"))]
#[serde(rename_all = "camelCase")]
pub struct PatchMailDto {
    pub subject: String,
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    /// Seconds since the epoch; `null` when the mail's date couldn't be read.
    pub timestamp: Option<i64>,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchPreviewFile"))]
#[serde(rename_all = "camelCase")]
pub struct PatchPreviewFileDto {
    pub path: String,
    pub old_path: Option<String>,
    pub status: PatchFileStatusDto,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
}

#[derive(Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "PatchFileStatus"))]
pub enum PatchFileStatusDto {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
}

impl From<git_core::patch::PatchPreview> for PatchPreviewDto {
    fn from(preview: git_core::patch::PatchPreview) -> Self {
        use git_core::patch::PreviewStatus;
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Self {
            parts: preview
                .parts
                .into_iter()
                .map(|part| PatchPreviewPartDto {
                    mail: part.mail.map(|mail| PatchMailDto {
                        subject: mail.subject,
                        body: mail.body,
                        author_name: mail.author_name,
                        author_email: mail.author_email,
                        timestamp: mail.timestamp,
                    }),
                    files: part
                        .files
                        .into_iter()
                        .map(|file| PatchPreviewFileDto {
                            path: file.path,
                            old_path: file.old_path,
                            status: match file.status {
                                PreviewStatus::Added => PatchFileStatusDto::Added,
                                PreviewStatus::Deleted => PatchFileStatusDto::Deleted,
                                PreviewStatus::Modified => PatchFileStatusDto::Modified,
                                PreviewStatus::Renamed => PatchFileStatusDto::Renamed,
                                PreviewStatus::Copied => PatchFileStatusDto::Copied,
                            },
                            additions: count(file.additions),
                            deletions: count(file.deletions),
                            binary: file.binary,
                        })
                        .collect(),
                })
                .collect(),
            problem: preview.problem,
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
    pub algorithm: DiffAlgorithmDto,
    pub show_whitespace: bool,
    pub expand_untracked: bool,
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
            algorithm: s.algorithm.into(),
            show_whitespace: s.show_whitespace,
            expand_untracked: s.expand_untracked,
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
            algorithm: d.algorithm.into(),
            show_whitespace: d.show_whitespace,
            expand_untracked: d.expand_untracked,
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

/// The `[appearance]` preferences (#615): `codeFont` and `codeFontSize` are `null` for the
/// built-in look; `codeFontSize` and `tabSize` are clamped to `CODE_FONT_SIZES` and `TAB_SIZES`.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "AppearanceSettings"))]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettingsDto {
    pub code_font: Option<String>,
    pub code_font_size: Option<u32>,
    pub ui_size: UiSizeDto,
    pub tab_size: u32,
    pub date_format: DateFormatDto,
}

/// `config::DateFormat` (#675).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "DateFormat"))]
pub enum DateFormatDto {
    Iso,
    System,
}

impl From<config::DateFormat> for DateFormatDto {
    fn from(f: config::DateFormat) -> Self {
        match f {
            config::DateFormat::Iso => DateFormatDto::Iso,
            config::DateFormat::System => DateFormatDto::System,
        }
    }
}

impl From<DateFormatDto> for config::DateFormat {
    fn from(f: DateFormatDto) -> Self {
        match f {
            DateFormatDto::Iso => config::DateFormat::Iso,
            DateFormatDto::System => config::DateFormat::System,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "UiSize"))]
pub enum UiSizeDto {
    Small,
    Normal,
    Large,
}

impl From<config::UiSize> for UiSizeDto {
    fn from(s: config::UiSize) -> Self {
        match s {
            config::UiSize::Small => UiSizeDto::Small,
            config::UiSize::Normal => UiSizeDto::Normal,
            config::UiSize::Large => UiSizeDto::Large,
        }
    }
}

impl From<UiSizeDto> for config::UiSize {
    fn from(s: UiSizeDto) -> Self {
        match s {
            UiSizeDto::Small => config::UiSize::Small,
            UiSizeDto::Normal => config::UiSize::Normal,
            UiSizeDto::Large => config::UiSize::Large,
        }
    }
}

impl From<config::AppearanceSettings> for AppearanceSettingsDto {
    fn from(s: config::AppearanceSettings) -> Self {
        Self {
            code_font: s.code_font,
            code_font_size: s.code_font_size,
            ui_size: s.ui_size.into(),
            tab_size: s.tab_size,
            date_format: s.date_format.into(),
        }
    }
}

impl From<AppearanceSettingsDto> for config::AppearanceSettings {
    fn from(d: AppearanceSettingsDto) -> Self {
        Self {
            code_font: d.code_font,
            code_font_size: d.code_font_size,
            ui_size: d.ui_size.into(),
            tab_size: d.tab_size,
            date_format: d.date_format.into(),
        }
    }
}

/// An installed font family (#858), for the code font suggestions: the name CSS knows it by,
/// and whether it's monospaced.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "FontFamily"))]
#[serde(rename_all = "camelCase")]
pub struct FontFamilyDto {
    pub name: String,
    pub monospace: bool,
}

impl From<crate::fonts::FontFamily> for FontFamilyDto {
    fn from(f: crate::fonts::FontFamily) -> Self {
        Self {
            name: f.name,
            monospace: f.monospace,
        }
    }
}

/// The `[updates]` preferences (desktop app only, #630): `checkAutomatically` checks for a new
/// version at start and every few hours; off, only "Check for updates" checks.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "UpdateSettings"))]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsDto {
    pub check_automatically: bool,
}

impl From<config::UpdateSettings> for UpdateSettingsDto {
    fn from(s: config::UpdateSettings) -> Self {
        Self {
            check_automatically: s.check_automatically,
        }
    }
}

impl From<UpdateSettingsDto> for config::UpdateSettings {
    fn from(d: UpdateSettingsDto) -> Self {
        Self {
            check_automatically: d.check_automatically,
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

/// The message of [`ErrorDto::worker_stopped`]: the repository's worker thread has stopped
/// (crashed) before it took the command.
pub const WORKER_STOPPED: &str = "worker thread stopped";
/// The message of [`ErrorDto::worker_stopped_before_replying`]: the worker stopped while the
/// command ran.
pub const WORKER_STOPPED_BEFORE_REPLYING: &str = "worker thread stopped before replying";
/// The message of [`ErrorDto::pull_failed`].
pub const PULL_FAILED: &str = "pull failed";

/// The kind of a rejected request, for the failures the frontend tells apart (#496). Every
/// request that fails answers an [`ErrorDto`]; the frontend switches on its `kind` and never on
/// the message text, which stays free to change. The kind is decided where the error is made
/// (#527): by the constructor that builds it, or from the typed error with [`ErrorKind::of_error`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
pub enum ErrorKind {
    /// Anything the frontend shows as it is.
    Other,
    /// The repository's worker thread has stopped; nothing on this repository works until it is
    /// closed and reopened ([`ErrorDto::worker_stopped`]).
    WorkerStopped,
    /// The operating-system credential store failed while saving, reading or forgetting a token
    /// (a remote's HTTPS token or a forge token), or while changing a remote's authentication.
    CredentialStoreFailure,
    /// A pull refused to start over uncommitted changes.
    DirtyWorktree,
    /// A pull failed and its terminal transfer event says why ([`ErrorDto::pull_failed`]).
    PullFailed,
    /// The user cancelled the transfer (a pull's); its terminal transfer event says so too.
    Cancelled,
    /// `get_conflict_hunks` on an add/delete conflict, which has no text hunks.
    AddDeleteConflict,
    /// A commit search whose query doesn't parse or names something that isn't there (#450);
    /// the search field shows its message.
    InvalidSearchQuery,
    /// The operating system refused a file access for lack of permission (#527).
    PermissionDenied,
    /// A switch (or a create-and-switch) refused because its checkout would overwrite local
    /// changes; `get_switch_conflicts` names the files and `stash_and_switch` offers a way on
    /// (#653).
    LocalChanges,
    /// A commit or amend refused because git has no `user.name`/`user.email`; `set_identity`
    /// sets them (#689).
    MissingIdentity,
}

impl ErrorKind {
    /// The kind of a typed error: from its type, or from the first error along its `source()`
    /// chain that has one. Never from its text.
    pub fn of_error(error: &(dyn std::error::Error + 'static)) -> Self {
        let mut current = Some(error);
        while let Some(error) = current {
            if let Some(kind) = Self::of_one(error) {
                return kind;
            }
            current = error.source();
        }
        if git_core::os_error::is_permission_denied(error) {
            Self::PermissionDenied
        } else {
            Self::Other
        }
    }

    fn of_one(error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        use crate::credentials::CredentialStoreError;
        use git_core::branch::BranchError;
        use git_core::commit::CommitError;
        use git_core::merge::MergeError;
        use git_core::remote::RemoteError;
        use git_core::search::SearchError;
        if let Some(BranchError::LocalChanges(_)) = error.downcast_ref::<BranchError>() {
            Some(Self::LocalChanges)
        } else if let Some(CommitError::MissingIdentity) = error.downcast_ref::<CommitError>() {
            Some(Self::MissingIdentity)
        } else if let Some(error) = error.downcast_ref::<RemoteError>() {
            match error {
                RemoteError::DirtyWorktree => Some(Self::DirtyWorktree),
                RemoteError::Cancelled => Some(Self::Cancelled),
                _ => None,
            }
        } else if let Some(MergeError::NotATextConflict(_)) = error.downcast_ref::<MergeError>() {
            Some(Self::AddDeleteConflict)
        } else if let Some(SearchError::InvalidQuery { .. }) = error.downcast_ref::<SearchError>() {
            Some(Self::InvalidSearchQuery)
        } else if let Some(CredentialStoreError::Keychain(_)) =
            error.downcast_ref::<CredentialStoreError>()
        {
            Some(Self::CredentialStoreFailure)
        } else {
            None
        }
    }
}

/// A rejected request (#496): what every `WorkerHandle` method and every transport handler
/// answers, the desktop app as the command's error, the sidecar as the JSON-RPC error's `data`
/// (next to the same `message`). Its kind is decided where it is made (#527): the constructors
/// below for the kinds no typed error carries, [`ErrorDto::from_error`] for a typed error. Text
/// (`From<String>`, `From<&str>`) is always [`ErrorKind::Other`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RepoErrorPayload"))]
pub struct ErrorDto {
    pub kind: ErrorKind,
    pub message: String,
}

impl ErrorDto {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// The worker thread has stopped before it took the command.
    pub fn worker_stopped() -> Self {
        Self::new(ErrorKind::WorkerStopped, WORKER_STOPPED)
    }

    /// The worker thread stopped while the command ran.
    pub fn worker_stopped_before_replying() -> Self {
        Self::new(ErrorKind::WorkerStopped, WORKER_STOPPED_BEFORE_REPLYING)
    }

    /// A pull failure its terminal transfer event reports with a `TransferErrorKind` (a
    /// transport error may quote a URL with credentials, so the reply keeps none of its text).
    pub fn pull_failed() -> Self {
        Self::new(ErrorKind::PullFailed, PULL_FAILED)
    }

    /// `error`'s text, with the kind [`ErrorKind::of_error`] gives it.
    pub fn from_error<E: std::error::Error + 'static>(error: E) -> Self {
        Self::new(ErrorKind::of_error(&error), error.to_string())
    }
}

/// A credential store failure (`Keychain`) is [`ErrorKind::CredentialStoreFailure`], whichever
/// token it was about (a remote's HTTPS token or a forge token), with the same text for both.
impl From<crate::credentials::CredentialStoreError> for ErrorDto {
    fn from(error: crate::credentials::CredentialStoreError) -> Self {
        use crate::credentials::CredentialStoreError;
        match error {
            CredentialStoreError::Keychain(diagnostic) => Self::new(
                ErrorKind::CredentialStoreFailure,
                credential_store_failure_message(&diagnostic),
            ),
            CredentialStoreError::InvalidHttpsUrl => Self::from_error(error),
        }
    }
}

/// The error text of a credential store failure: [`ErrorKind::CredentialStoreFailure`].
pub fn credential_store_failure_message(diagnostic: &str) -> String {
    format!(
        "{}: {diagnostic}",
        git_core::remote::CREDENTIAL_STORE_FAILURE_ERROR
    )
}

impl std::fmt::Display for ErrorDto {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<String> for ErrorDto {
    fn from(message: String) -> Self {
        Self::new(ErrorKind::Other, message)
    }
}

impl From<&str> for ErrorDto {
    fn from(message: &str) -> Self {
        Self::new(ErrorKind::Other, message)
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
            git_core::tree::TreeEntryKind,
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
            RepoAliasDto,
            SubmoduleInfoDto,
            ReflogEntryDto,
            RemoteInfoDto,
            RemoteAuthModeDto,
            TagInfoDto,
            UpstreamInfoDto,
            PullOutcomeDto,
            PullStrategyDto,
            StashSwitchOutcomeDto,
            StashEntryDto,
            BlameLineDto,
            GraphCommitDto,
            FileHistoryEntryDto,
            FileHistoryPageDto,
            LineHistoryEntryDto,
            LineHistoryDto,
            SearchPageDto,
            GraphPageDto,
            GraphRefreshDto,
            GraphSegmentDto,
            RowBadgesDto,
            ChangedFileDto,
            CompareRangeDto,
            CommitLocationDto,
            CollapsedMergeDto,
            FileStatsDto,
            CommitStatsDto,
            CommitStatsEntryDto,
            CommitPersonDto,
            CommitDetailsDto,
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
            crate::watch::WatchEvent,
            RepoWatchDto,
            PickOutcomeDto,
            UndoStateDto,
            ChangeStampDto,
            StatusWithStampDto,
            SignatureStatusDto,
            SignatureFormatDto,
            CommitSignatureDto,
            UndoOutcomeDto,
            ResetModeDto,
            FileSideDto,
            FileSidesDto,
            TreeEntryDto,
            TreeListingDto,
            FileContentDto,
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
            HistoryEditTargetDto,
            HistoryEditKindDto,
            RebaseStepResultDto,
            DiffLineDto,
            DiffHunkDto,
            ForgeProviderDto,
            ForgeRepositoryDto,
            PullRequestDto,
            PullRequestListDto,
            CreatePullRequestDto,
            DiffOptionsDto,
            GraphSelectionDto,
            LoadedGraphDto,
            DiffAlgorithmDto,
            CommitHunkDto,
            CommitChangeRequestDto,
            PatchSourceDto,
            PatchContentDto,
            PatchApplyDto,
            PatchPreviewDto,
            PatchApplyOutcomeDto,
            PatchPreviewPartDto,
            PatchMailDto,
            PatchPreviewFileDto,
            PatchFileStatusDto,
            DiffSettingsDto,
            WordWrapDto,
            CommitMessageSettingsDto,
            FetchSettingsDto,
            AppearanceSettingsDto,
            FontFamilyDto,
            UiSizeDto,
            DateFormatDto,
            UpdateSettingsDto,
        );
        test_support::assert_generated_file(
            &test_support::repo_root().join("frontend/src/ipc/generated/wire-dto.ts"),
            &bindings,
        );
        test_support::assert_generated_file(
            &test_support::repo_root().join("frontend/src/ipc/generated/wire-constants.ts"),
            &wire_constants(),
        );
    }

    /// #497: the limits and defaults the backend enforces, for the frontend to use instead of
    /// its own copies. The settings defaults are each `config` type's `Default`, serialized as
    /// its DTO.
    fn wire_constants() -> String {
        fn json(value: impl Serialize) -> String {
            serde_json::to_string(&value).unwrap()
        }
        let columns = config::COMMIT_MESSAGE_COLUMNS;
        let font_sizes = config::CODE_FONT_SIZES;
        let tab_sizes = config::TAB_SIZES;
        let lines = [
            "// Generated from the backend's limits and defaults (crates/repo-service/src/wire.rs).".to_string(),
            "// Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p repo-service wire_bindings_are_current".to_string(),
            "import type { AppearanceSettings, CommitMessageSettings, DiffSettings, FetchSettings, UpdateSettings } from \"./wire-dto\";".to_string(),
            String::new(),
            "/** `git_core::diff::MAX_CONTEXT_LINES`: more context is clamped to it. */".to_string(),
            format!(
                "export const MAX_CONTEXT_LINES = {};",
                git_core::diff::MAX_CONTEXT_LINES
            ),
            "/** `git_core::diff::WHOLE_FILE_CONTEXT_LINES`: the context that shows the whole file. */".to_string(),
            format!(
                "export const WHOLE_FILE_CONTEXT_LINES = {};",
                git_core::diff::WHOLE_FILE_CONTEXT_LINES
            ),
            "/** `config::COMMIT_MESSAGE_COLUMNS`: the range commit message columns are clamped to. */".to_string(),
            format!(
                "export const COMMIT_MESSAGE_COLUMNS = {{ min: {}, max: {} }} as const;",
                columns.start(),
                columns.end()
            ),
            "/** `config::DiffSettings::default()`. */".to_string(),
            format!(
                "export const DEFAULT_DIFF_SETTINGS: DiffSettings = {};",
                json(DiffSettingsDto::from(config::DiffSettings::default()))
            ),
            "/** `config::CommitMessageSettings::default()`. */".to_string(),
            format!(
                "export const DEFAULT_COMMIT_MESSAGE_SETTINGS: CommitMessageSettings = {};",
                json(CommitMessageSettingsDto::from(
                    config::CommitMessageSettings::default()
                ))
            ),
            "/** `config::FetchSettings::default()`. */".to_string(),
            format!(
                "export const DEFAULT_FETCH_SETTINGS: FetchSettings = {};",
                json(FetchSettingsDto::from(config::FetchSettings::default()))
            ),
            "/** `config::AppearanceSettings::default()`. */".to_string(),
            format!(
                "export const DEFAULT_APPEARANCE_SETTINGS: AppearanceSettings = {};",
                json(AppearanceSettingsDto::from(
                    config::AppearanceSettings::default()
                ))
            ),
            "/** `config::CODE_FONT_SIZES`: the range the code font size is clamped to, in px. */".to_string(),
            format!(
                "export const CODE_FONT_SIZES = {{ min: {}, max: {} }} as const;",
                font_sizes.start(),
                font_sizes.end()
            ),
            "/** `config::TAB_SIZES`: the range the tab size is clamped to, in columns. */".to_string(),
            format!(
                "export const TAB_SIZES = {{ min: {}, max: {} }} as const;",
                tab_sizes.start(),
                tab_sizes.end()
            ),
            "/** `config::UpdateSettings::default()`. */".to_string(),
            format!(
                "export const DEFAULT_UPDATE_SETTINGS: UpdateSettings = {};",
                json(UpdateSettingsDto::from(config::UpdateSettings::default()))
            ),
            "/** `git_core::search::KEYS`: the keys a commit search `key:value` term can use. */".to_string(),
            format!(
                "export const SEARCH_KEYS = {} as const;",
                json(git_core::search::KEYS)
            ),
        ];
        lines.join("\n") + "\n"
    }
}

/// #496, #527: each `ErrorKind` comes from its producer's constructor or typed error, and
/// nothing else gets one.
/// #630: the settings a client saves reach `config` as sent, both ways.
#[cfg(test)]
mod update_settings_conversion {
    use super::UpdateSettingsDto;

    #[test]
    fn a_saved_choice_reaches_config_unchanged() {
        for check_automatically in [false, true] {
            let settings = config::UpdateSettings::from(UpdateSettingsDto {
                check_automatically,
            });
            assert_eq!(settings.check_automatically, check_automatically);
            assert_eq!(
                UpdateSettingsDto::from(settings).check_automatically,
                check_automatically
            );
        }
    }
}

#[cfg(test)]
mod error_kinds {
    use super::*;
    use crate::credentials::CredentialStoreError;
    use git_core::merge::MergeError;
    use git_core::remote::RemoteError;
    use git_core::search::SearchError;

    #[test]
    fn each_producer_gets_its_kind() {
        let permission = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let cases = [
            (ErrorDto::worker_stopped(), ErrorKind::WorkerStopped),
            (
                ErrorDto::worker_stopped_before_replying(),
                ErrorKind::WorkerStopped,
            ),
            (ErrorDto::pull_failed(), ErrorKind::PullFailed),
            (
                ErrorDto::from_error(RemoteError::DirtyWorktree),
                ErrorKind::DirtyWorktree,
            ),
            (
                ErrorDto::from_error(RemoteError::Cancelled),
                ErrorKind::Cancelled,
            ),
            (
                ErrorDto::from(CredentialStoreError::Keychain("locked".into())),
                ErrorKind::CredentialStoreFailure,
            ),
            (
                ErrorDto::from_error(CredentialStoreError::Keychain("locked".into())),
                ErrorKind::CredentialStoreFailure,
            ),
            (
                ErrorDto::from_error(MergeError::NotATextConflict("dir/it's: here.bin".into())),
                ErrorKind::AddDeleteConflict,
            ),
            (
                ErrorDto::from_error(git_core::search::parse_query("a or").unwrap_err()),
                ErrorKind::InvalidSearchQuery,
            ),
            (
                ErrorDto::from_error(MergeError::Io(permission)),
                ErrorKind::PermissionDenied,
            ),
            (
                ErrorDto::from_error(git_core::branch::BranchError::LocalChanges("x".into())),
                ErrorKind::LocalChanges,
            ),
            (
                ErrorDto::from_error(git_core::commit::CommitError::MissingIdentity),
                ErrorKind::MissingIdentity,
            ),
        ];
        for (error, kind) in cases {
            assert_eq!(error.kind, kind, "{}", error.message);
        }
    }

    /// Text never has a kind, not even a recognised producer's: the kind is decided where the
    /// error is made. Neither do the producers' other errors.
    #[test]
    fn text_and_other_errors_stay_other() {
        let texts = [
            WORKER_STOPPED.to_string(),
            PULL_FAILED.to_string(),
            RemoteError::DirtyWorktree.to_string(),
            RemoteError::Cancelled.to_string(),
            credential_store_failure_message("locked"),
            MergeError::NotATextConflict("a.bin".into()).to_string(),
            "Permission denied".to_string(),
        ];
        for text in texts {
            assert_eq!(
                ErrorDto::from(text.as_str()).kind,
                ErrorKind::Other,
                "{text}"
            );
            assert_eq!(
                ErrorDto::from(text.clone()).kind,
                ErrorKind::Other,
                "{text}"
            );
        }
        let errors = [
            ErrorDto::from_error(MergeError::NotAnAddDeleteConflict("x".into())),
            ErrorDto::from_error(RemoteError::NoUpstream),
            ErrorDto::from_error(SearchError::StaleCursor),
            ErrorDto::from(CredentialStoreError::InvalidHttpsUrl),
            ErrorDto::from_error(MergeError::Io(std::io::Error::from(
                std::io::ErrorKind::NotFound,
            ))),
        ];
        for error in errors {
            assert_eq!(error.kind, ErrorKind::Other, "{}", error.message);
        }
    }

    /// A kind is found along the source chain: a pull's cancel wrapped in another error is still
    /// `Cancelled`.
    #[test]
    fn the_kind_is_found_along_the_source_chain() {
        #[derive(Debug)]
        struct Outer(RemoteError);
        impl std::fmt::Display for Outer {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "outer: {}", self.0)
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        assert_eq!(
            ErrorDto::from_error(Outer(RemoteError::Cancelled)).kind,
            ErrorKind::Cancelled
        );
    }

    #[test]
    fn the_envelope_keeps_the_message() {
        let error = ErrorDto::worker_stopped();

        assert_eq!(
            serde_json::to_string(&error).unwrap(),
            r#"{"kind":"WorkerStopped","message":"worker thread stopped"}"#
        );
        // Shown as its message alone, as the old `String` errors were.
        assert_eq!(error.to_string(), WORKER_STOPPED);
    }
}
