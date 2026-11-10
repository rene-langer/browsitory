use crate::wire::ErrorDto;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;

use git_core::blame::BlameLine;
use git_core::branch::BranchInfo;
use git_core::diff::DiffHunk;
use git_core::forge::{ForgeProvider, ForgeRepository};
use git_core::graph::{GraphCommit, GraphSelection};
use git_core::merge::{ConflictSegment, FileConflictChoice, MergeOutcome};
use git_core::rebase::{RebasePlanCommit, RebasePlanEntry, RebaseStepResult};
use git_core::reflog::ReflogEntry;
use git_core::remote::{
    PullOutcome, RemoteInfo, TagInfo, TransferErrorKind, TransferOperation, UpstreamInfo,
};
use git_core::stash::StashEntry;
use git_core::status::StatusEntry;
use git_core::submodule::SubmoduleInfo;
use git_core::worktree::WorktreeInfo;

use crate::credentials::CredentialStore;
#[cfg(not(feature = "forge-fixture-override"))]
use crate::credentials::KeyringCredentialStore;
use crate::pull_requests::{
    CreatePullRequest, ForgeApi, PullRequest, PullRequestList, ReqwestForgeApi,
};

mod atomic_file;
mod background_fetch;
mod branch;
mod cancel;
mod compare;
mod fetch_all;
mod forge;
mod gate;
mod history;
mod hooks;
mod merge;
mod patch;
mod policy;
mod rebase;
mod rebase_store;
mod reflog;
mod registry;
mod remote;
mod search;
mod sequence_store;
mod stash;
mod state_file;
mod state_lock;
mod status;
mod submodule;
mod tag;
mod tools;
mod transfer_thread;
mod tree;
mod undo;
mod undo_store;
mod worker_loop;
mod worktree;

pub use hooks::{HookEvent, HookSource};
pub use patch::{CommitChange, PatchSource};
pub use rebase::{HistoryEdit, HistoryEditKind};
pub use registry::{repo_key, WorkerRegistry};
pub use undo::{UndoOutcome, UndoState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    Started {
        operation_id: String,
        operation: TransferOperation,
    },
    Progress(git_core::remote::TransferProgress),
    /// One remote of a Fetch all failed; sent before the operation's `Completed`.
    RemoteFailed {
        operation_id: String,
        /// `FetchAll`, or `BackgroundFetch` for the fetch the app starts on its own.
        operation: TransferOperation,
        remote_name: String,
        error: TransferErrorKind,
    },
    Completed {
        operation_id: String,
        operation: TransferOperation,
        error: Option<TransferErrorKind>,
        /// The failed `pre-push` hook's output (`error` is `HookFailed`); `None` for every other
        /// outcome. Unlike remote sideband text this comes from the user's own script.
        hook_output: Option<String>,
    },
}

pub(crate) enum Command {
    /// Liveness probe (see `WorkerHandle::is_alive`). Carries no repository work — the dispatch
    /// (`WorkerLoop::dispatch`) only echoes back on `reply`. Used by callers that hold a repo-path -> `Worker`
    /// map (`tauri-app`'s `AppState`, `vscode-sidecar`'s repo map) to detect a worker thread that
    /// already exited, most notably after a panic inside a `Command` match arm unwound the
    /// thread and dropped its `Receiver` (see AUD-2026-09-05-CONC-001).
    Ping {
        reply: Sender<()>,
    },
    /// Test-only: panics immediately inside the dispatch loop, simulating a `git-core` call that
    /// panics inside a `Command` match arm. See `WorkerHandle::crash_for_test`.
    #[cfg(test)]
    PanicNow,
    GetStatus {
        reply: Sender<Result<Vec<StatusEntry>, ErrorDto>>,
    },
    GetCommitGraph {
        limit: usize,
        selection: GraphSelection,
        reply: Sender<Result<Vec<GraphCommit>, ErrorDto>>,
    },
    LocateCommit {
        revision: String,
        selection: GraphSelection,
        reply: Sender<Result<Option<git_core::locate::CommitLocation>, ErrorDto>>,
    },
    GetWorkingDiff {
        path: String,
        staged: bool,
        opts: git_core::diff::DiffOptions,
        old_path: Option<String>,
        reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
    },
    GetCommitDiff {
        commit_id: String,
        path: String,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
    },
    GetCommitFiles {
        commit_id: String,
        reply: Sender<Result<Vec<String>, ErrorDto>>,
    },
    GetCommitMessage {
        commit_id: String,
        reply: Sender<Result<String, ErrorDto>>,
    },
    GetCommitSignature {
        commit_id: String,
        reply: Sender<Result<git_core::signing::CommitSignature, ErrorDto>>,
    },
    GetBlame {
        commit_id: String,
        path: String,
        reply: Sender<Result<Vec<BlameLine>, ErrorDto>>,
    },
    GetFileHistory {
        path: String,
        limit: usize,
        reply: Sender<Result<Vec<git_core::history::FileHistoryEntry>, ErrorDto>>,
    },
    /// The commits that changed lines `start_line..=end_line` of `path` as it is in `revision`
    /// (a commit id or "HEAD"), at most `limit` (#447).
    GetLineHistory {
        revision: String,
        path: String,
        start_line: usize,
        end_line: usize,
        limit: usize,
        reply: Sender<Result<git_core::line_history::LineHistory, ErrorDto>>,
    },
    /// One page of a commit search. `generation` is the `SearchGeneration` value claimed when
    /// the command was queued; once a newer search or `cancel_search` bumps the counter, this
    /// one stops and replies `Search cancelled.`.
    SearchCommits {
        query: String,
        selection: GraphSelection,
        cursor: Option<String>,
        generation: u64,
        reply: Sender<Result<git_core::search::SearchPage, ErrorDto>>,
    },
    GetCommitRenameDiff {
        commit_id: String,
        old_path: String,
        new_path: String,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
    },
    GetRangeFiles {
        from_id: String,
        to_id: String,
        reply: Sender<Result<Vec<git_core::diff::ChangedFile>, ErrorDto>>,
    },
    GetRangeDiff {
        from_id: String,
        to_id: String,
        path: String,
        old_path: Option<String>,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
    },
    GetWorkingTreeCompareFiles {
        commit_id: String,
        reply: Sender<Result<Vec<git_core::diff::ChangedFile>, ErrorDto>>,
    },
    GetWorkingTreeCompareDiff {
        commit_id: String,
        path: String,
        old_path: Option<String>,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
    },
    GetBranchCompareRange {
        ref_name: String,
        reply: Sender<Result<git_core::diff::CompareRange, ErrorDto>>,
    },
    StageFile {
        path: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    UnstageFile {
        path: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    StageHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        reply: Sender<Result<(), ErrorDto>>,
    },
    UnstageHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        reply: Sender<Result<(), ErrorDto>>,
    },
    DiscardHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        reply: Sender<Result<(), ErrorDto>>,
    },
    StageLines {
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    UnstageLines {
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    DiscardLines {
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    DiscardFile {
        path: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    DiscardAllUnstaged {
        paths: Vec<String>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    /// #448: a commit's file or hunk reverted or applied in the working tree.
    ApplyCommitChange {
        change: CommitChange,
        reply: Sender<Result<(), ErrorDto>>,
    },
    GetPatch {
        source: PatchSource,
        reply: Sender<Result<Vec<u8>, ErrorDto>>,
    },
    ApplyPatch {
        content: Vec<u8>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    IgnorePath {
        pattern: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    Commit {
        message: String,
        no_verify: bool,
        /// Appended to `message` before the hooks run (`Co-authored-by` from the commit box).
        trailers: Vec<git_core::commit::Trailer>,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::commit::CommitOutcome, ErrorDto>>,
    },
    ListBranches {
        reply: Sender<Result<Vec<BranchInfo>, ErrorDto>>,
    },
    CreateBranch {
        name: String,
        start_point: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    /// `operation_id` (`switch-<n>`) is what `cancel_transfer` stops; `events` reports
    /// `post-checkout` (#185).
    SwitchBranch {
        name: String,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    DeleteBranch {
        name: String,
        force: bool,
        reply: Sender<Result<(), ErrorDto>>,
    },
    RenameBranch {
        old_name: String,
        new_name: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ListWorktrees {
        reply: Sender<Result<Vec<WorktreeInfo>, ErrorDto>>,
    },
    CreateWorktree {
        name: String,
        path: PathBuf,
        branch: String,
        start_point: Option<String>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    RemoveWorktree {
        name: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    PruneWorktrees {
        reply: Sender<Result<(), ErrorDto>>,
    },
    ListSubmodules {
        reply: Sender<Result<Vec<SubmoduleInfo>, ErrorDto>>,
    },
    InitSubmodule {
        path: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    UpdateSubmodule {
        path: String,
        recursive: bool,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ListReflogRefs {
        reply: Sender<Result<Vec<String>, ErrorDto>>,
    },
    GetReflog {
        reference: String,
        reply: Sender<Result<Vec<ReflogEntry>, ErrorDto>>,
    },
    RestoreReflogEntry {
        reference: String,
        new_id: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ListRemotes {
        reply: Sender<Result<Vec<RemoteInfo>, ErrorDto>>,
    },
    ListRemoteBranches {
        remote_name: String,
        reply: Sender<Result<Vec<String>, ErrorDto>>,
    },
    GetCurrentUpstream {
        reply: Sender<Result<Option<UpstreamInfo>, ErrorDto>>,
    },
    GetRemoteUpstreams {
        name: String,
        reply: Sender<Result<Vec<UpstreamInfo>, ErrorDto>>,
    },
    GetRemoteAuthMode {
        name: String,
        reply: Sender<Result<Option<git_core::remote::RemoteAuthMode>, ErrorDto>>,
    },
    SaveHttpsCredential {
        remote_name: String,
        username: String,
        token: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ForgetHttpsCredential {
        remote_name: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    SetRemoteAuthMode {
        remote_name: String,
        mode: git_core::remote::RemoteAuthMode,
        reply: Sender<Result<(), ErrorDto>>,
    },
    AddRemote {
        name: String,
        fetch_url: String,
        push_url: Option<String>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    RenameRemote {
        old_name: String,
        new_name: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    UpdateRemoteUrls {
        name: String,
        fetch_url: String,
        push_url: Option<String>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    RemoveRemote {
        name: String,
        clear_upstreams: bool,
        reply: Sender<Result<(), ErrorDto>>,
    },
    SetCurrentUpstream {
        remote_name: String,
        remote_branch: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ClearCurrentUpstream {
        reply: Sender<Result<(), ErrorDto>>,
    },
    ListTags {
        reply: Sender<Result<Vec<TagInfo>, ErrorDto>>,
    },
    CreateTag {
        name: String,
        message: Option<String>,
        target: Option<String>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    DeleteTag {
        name: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ListStashes {
        reply: Sender<Result<Vec<StashEntry>, ErrorDto>>,
    },
    SaveStash {
        options: git_core::stash::StashOptions,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ApplyStash {
        index: usize,
        commit_id: String,
        restore_index: bool,
        reply: Sender<Result<git_core::stash::StashApplyOutcome, ErrorDto>>,
    },
    PopStash {
        index: usize,
        commit_id: String,
        restore_index: bool,
        reply: Sender<Result<git_core::stash::StashApplyOutcome, ErrorDto>>,
    },
    GetStashFiles {
        commit_id: String,
        reply: Sender<Result<git_core::stash::StashFiles, ErrorDto>>,
    },
    DropStash {
        index: usize,
        commit_id: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    /// `operation_id` (`merge-<n>`) and `events`: a fast-forward's or squash's `post-merge`
    /// (#185).
    StartMerge {
        branch_name: String,
        options: git_core::merge::MergeOptions,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<MergeOutcome, ErrorDto>>,
    },
    GetMergeDefaults {
        reply: Sender<Result<git_core::merge::MergeOptions, ErrorDto>>,
    },
    #[allow(dead_code)]
    GetConflictHunks {
        path: String,
        reply: Sender<Result<Vec<ConflictSegment>, ErrorDto>>,
    },
    #[allow(dead_code)]
    ResolveConflict {
        path: String,
        resolved_content: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    #[allow(dead_code)]
    AbortMerge {
        reply: Sender<Result<(), ErrorDto>>,
    },
    #[allow(dead_code)]
    GetMergeMessage {
        reply: Sender<Result<Option<String>, ErrorDto>>,
    },
    ResolveAddDeleteConflict {
        path: String,
        choice: FileConflictChoice,
        reply: Sender<Result<(), ErrorDto>>,
    },
    MarkResolved {
        path: String,
        force: bool,
        reply: Sender<Result<git_core::merge::MarkResolvedOutcome, ErrorDto>>,
    },
    ResolveWholeFile {
        path: String,
        side: git_core::merge::ConflictSide,
        reply: Sender<Result<(), ErrorDto>>,
    },
    PrepareDiffTool {
        path: String,
        source: git_core::tools::DiffSource,
        reply: Sender<Result<git_core::tools::PreparedDiffTool, ErrorDto>>,
    },
    PrepareMergeTool {
        path: String,
        reply: Sender<Result<git_core::tools::PreparedMergeTool, ErrorDto>>,
    },
    GetGitTools {
        reply: Sender<Result<git_core::tools::GitTools, ErrorDto>>,
    },
    /// Both sides of `path` in `source`, for the image preview (`MAX_PREVIEW_BYTES` each).
    GetFileSides {
        path: String,
        source: git_core::tools::DiffSource,
        reply: Sender<Result<git_core::diff::FileSides, ErrorDto>>,
    },
    ListRecentAuthors {
        reply: Sender<Result<Vec<git_core::commit::SignatureInput>, ErrorDto>>,
    },
    CheckMergeTarget {
        path: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ReadFileVersion {
        path: String,
        version: git_core::tools::FileVersion,
        reply: Sender<Result<Option<Vec<u8>>, ErrorDto>>,
    },
    /// One folder of a commit's tree, or a filter's matches (#446).
    GetCommitTree {
        commit_id: String,
        dir: String,
        filter: Option<String>,
        reply: Sender<Result<git_core::tree::TreeListing, ErrorDto>>,
    },
    GetFileAtCommit {
        commit_id: String,
        path: String,
        reply: Sender<Result<git_core::tree::FileContent, ErrorDto>>,
    },
    SaveFileVersion {
        commit_id: String,
        path: String,
        destination: std::path::PathBuf,
        reply: Sender<Result<(), ErrorDto>>,
    },
    RestoreFileVersion {
        commit_id: String,
        path: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    #[allow(dead_code)]
    CommitsSince {
        onto: String,
        reply: Sender<Result<Vec<RebasePlanCommit>, ErrorDto>>,
    },
    #[allow(dead_code)]
    StartRebase {
        onto: String,
        plan: Vec<RebasePlanEntry>,
        /// The one-click history edit this rebase runs, for its undo label (#473).
        edit: Option<HistoryEdit>,
        reply: Sender<Result<RebaseStepResult, ErrorDto>>,
    },
    #[allow(dead_code)]
    RebaseContinue {
        reply: Sender<Result<RebaseStepResult, ErrorDto>>,
    },
    #[allow(dead_code)]
    AbortRebase {
        reply: Sender<Result<git_core::rebase::RebaseAbort, ErrorDto>>,
    },
    FetchRemote {
        remote_name: String,
        prune: Option<bool>,
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, ErrorDto>>,
    },
    PullCurrentUpstream {
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<PullOutcome, ErrorDto>>,
    },
    GetHeadInfo {
        reply: Sender<Result<git_core::repo::HeadInfo, ErrorDto>>,
    },
    AmendCommit {
        message: String,
        no_verify: bool,
        /// Replaces the author's name and email (the author time is kept).
        author: Option<git_core::commit::SignatureInput>,
        trailers: Vec<git_core::commit::Trailer>,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::commit::CommitOutcome, ErrorDto>>,
    },
    /// The pick commands' `operation_id` (`pick-<n>`) and `events`: each clean step's
    /// `prepare-commit-msg`, signing and `post-commit` (#183).
    CherryPick {
        commit_id: String,
        mainline: Option<u32>,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::PickOutcome, ErrorDto>>,
    },
    RevertCommit {
        commit_id: String,
        mainline: Option<u32>,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::PickOutcome, ErrorDto>>,
    },
    CherryPickMany {
        commit_ids: Vec<String>,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::SequenceOutcome, ErrorDto>>,
    },
    RevertMany {
        commit_ids: Vec<String>,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::SequenceOutcome, ErrorDto>>,
    },
    /// Commits the resolved step through the commit hooks (#133): `no_verify` skips the
    /// skippable ones, `operation_id` (`continue-<n>`) is what `cancel_transfer` stops, and
    /// `events` reports each hook.
    ContinueSequence {
        no_verify: bool,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::SequenceOutcome, ErrorDto>>,
    },
    AbortSequence {
        /// For a terminal sequence: what the user confirmed (#281 review); a stored sequence
        /// ignores it.
        confirmed: Option<git_core::pick::ConfirmedAbort>,
        reply: Sender<Result<git_core::pick::SequenceAbort, ErrorDto>>,
    },
    ResetToCommit {
        commit_id: String,
        mode: git_core::reset::ResetMode,
        reply: Sender<Result<(), ErrorDto>>,
    },
    /// `operation_id` (`checkout-<n>`) and `events`: `post-checkout` (#185).
    CheckoutCommit {
        commit_id: String,
        operation_id: cancel::OperationId,
        events: Sender<HookEvent>,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ForcePushCurrentBranch {
        remote_name: String,
        no_verify: bool,
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, ErrorDto>>,
    },
    PushCurrentBranch {
        remote_name: String,
        no_verify: bool,
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, ErrorDto>>,
    },
    PushTags {
        remote_name: String,
        names: Vec<String>,
        no_verify: bool,
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, ErrorDto>>,
    },
    FetchAllRemotes {
        prune: Option<bool>,
        /// `None` for the user's Fetch all; `Some` for the "Fetch automatically" tick (see
        /// `WorkerHandle::background_fetch`), whose `prune` is the command's.
        background: Option<background_fetch::BackgroundFetchOptions>,
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, ErrorDto>>,
    },
    PushBranch {
        remote_name: String,
        local_branch: String,
        remote_branch: String,
        set_upstream: bool,
        no_verify: bool,
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, ErrorDto>>,
    },
    DeleteRemoteBranch {
        remote_name: String,
        branch_name: String,
        no_verify: bool,
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, ErrorDto>>,
    },
    DeleteRemoteTag {
        remote_name: String,
        tag_name: String,
        no_verify: bool,
        operation_id: cancel::OperationId,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, ErrorDto>>,
    },
    DetectForgeRepository {
        reply: Sender<Result<Vec<ForgeRepository>, ErrorDto>>,
    },
    SaveForgeToken {
        provider: ForgeProvider,
        account: String,
        token: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ForgetForgeToken {
        provider: ForgeProvider,
        account: String,
        reply: Sender<Result<(), ErrorDto>>,
    },
    ListPullRequests {
        remote_name: String,
        account: String,
        reply: Sender<Result<PullRequestList, ErrorDto>>,
    },
    CreatePullRequest {
        remote_name: String,
        account: String,
        create: CreatePullRequest,
        reply: Sender<Result<PullRequest, ErrorDto>>,
    },
    GetUndoState {
        reply: Sender<Result<UndoState, ErrorDto>>,
    },
    Undo {
        reply: Sender<Result<UndoOutcome, ErrorDto>>,
    },
    Redo {
        reply: Sender<Result<UndoOutcome, ErrorDto>>,
    },
}

pub use cancel::CancelRegistry;

/// The generation of the newest commit search. A running search whose generation is no longer
/// current stops, so a newer search (or `cancel_search`) supersedes any older one, queued or
/// running. A side channel for the same reason as `CancelRegistry`: a cancel sent as a
/// `Command` would queue behind the very search it is meant to stop.
pub type SearchGeneration = Arc<AtomicU64>;

pub struct Worker {
    tx: Sender<Command>,
    cancelled: CancelRegistry,
    search_generation: SearchGeneration,
    workdir: Option<PathBuf>,
}

/// Cheap, cloneable handle to a `Worker`'s command channel.
///
/// Callers clone this out of shared state and drop the lock *before* blocking on a
/// reply, so a slow (or wedged) repository operation can't serialize unrelated commands.
#[derive(Clone)]
pub struct WorkerHandle {
    tx: Sender<Command>,
    /// Shared with the worker thread — see `CancelRegistry`.
    cancelled: CancelRegistry,
    /// Shared with the worker thread — see `SearchGeneration`.
    search_generation: SearchGeneration,
    /// The work tree root, captured at spawn (`None` for a bare repository).
    workdir: Option<PathBuf>,
}

/// Finds the `ForgeRepository` for a named remote. Returns a secret-free, static error (never
/// the raw remote URL or any detection detail) when the remote isn't a supported, unambiguous
/// forge repository — callers must check this before ever touching `PullRequestService`, so an
/// unsupported/ambiguous remote never causes an HTTP request.
impl Worker {
    /// Production entry point: constructs the real OS-keychain credential store. Behind the
    /// `forge-fixture-override` feature (E2E builds only — see `Cargo.toml`'s doc comment on
    /// that feature and `credentials.rs`'s `InMemoryCredentialStore`), an in-memory store is
    /// used instead so the E2E binary never touches a real OS keychain/D-Bus secrets service.
    /// Without the feature this function is exactly the line below — same as before that
    /// feature existed — so a release build's behavior is unchanged.
    #[cfg(not(feature = "forge-fixture-override"))]
    pub fn spawn(path: PathBuf) -> Result<Self, String> {
        Self::spawn_with(path, KeyringCredentialStore, ReqwestForgeApi::new())
    }

    #[cfg(feature = "forge-fixture-override")]
    pub fn spawn(path: PathBuf) -> Result<Self, String> {
        Self::spawn_with(
            path,
            crate::credentials::InMemoryCredentialStore::default(),
            ReqwestForgeApi::new(),
        )
    }

    /// Constructs the worker's credential store and forge HTTP transport by dependency
    /// injection, so tests can supply in-memory/fake implementations at both external seams
    /// (the OS keychain and the forge HTTP transport) instead of a real keychain or a live
    /// GitHub/Bitbucket account. Production code only ever calls this through `spawn`, above.
    fn spawn_with<S, A>(path: PathBuf, credential_store: S, forge_api: A) -> Result<Self, String>
    where
        S: CredentialStore + Send + Sync + 'static,
        A: ForgeApi + Send + 'static,
    {
        Self::spawn_paused(path, credential_store, forge_api, None)
    }

    /// Test-only: a worker that stops after an armed command, before its undo `finish`, until
    /// the test lets it go (see `PauseControl`), to make cross-worker races deterministic.
    #[cfg(test)]
    pub(crate) fn spawn_with_pause_before_finish(path: PathBuf) -> (Self, PauseControl) {
        Self::spawn_with_pause(path, PausePoint::BeforeFinish)
    }

    /// Test-only: a worker that stops when an armed command first finds the state lock busy,
    /// before its wait starts, until the test lets it go (see `PauseControl`, #274). The test
    /// knows the worker is waiting, and the wait can't run out while the test works.
    #[cfg(test)]
    pub(crate) fn spawn_with_pause_on_lock_wait(path: PathBuf) -> (Self, PauseControl) {
        Self::spawn_with_pause(path, PausePoint::LockWait)
    }

    #[cfg(test)]
    fn spawn_with_pause(path: PathBuf, point: PausePoint) -> (Self, PauseControl) {
        let (hook, control) = PauseHook::new(point);
        // The credential store `spawn` uses; these tests never touch it.
        #[cfg(not(feature = "forge-fixture-override"))]
        let store = KeyringCredentialStore;
        #[cfg(feature = "forge-fixture-override")]
        let store = crate::credentials::InMemoryCredentialStore::default();
        let worker = Self::spawn_paused(path, store, ReqwestForgeApi::new(), Some(hook)).unwrap();
        (worker, control)
    }

    fn spawn_paused<S, A>(
        path: PathBuf,
        credential_store: S,
        forge_api: A,
        pause: Option<PauseHook>,
    ) -> Result<Self, String>
    where
        S: CredentialStore + Send + Sync + 'static,
        A: ForgeApi + Send + 'static,
    {
        let repo_path = path;
        let repo = git_core::repo::open(&repo_path).map_err(|error| error.to_string())?;
        let workdir = repo.workdir().map(std::path::Path::to_path_buf);
        let (tx, rx) = mpsc::channel::<Command>();
        let cancelled = CancelRegistry::default();
        let search_generation: SearchGeneration = Arc::new(AtomicU64::new(0));
        let worker_search_generation = Arc::clone(&search_generation);

        thread::spawn(move || {
            let mut worker = worker_loop::WorkerLoop::new(
                repo_path,
                repo,
                worker_search_generation,
                credential_store,
                forge_api,
                pause,
            );
            for command in rx {
                worker.step(command);
            }
        });

        Ok(Worker {
            tx,
            cancelled,
            search_generation,
            workdir,
        })
    }

    /// A cloneable handle to this worker, cheap enough to take out of a mutex guard.
    pub fn handle(&self) -> WorkerHandle {
        WorkerHandle {
            tx: self.tx.clone(),
            cancelled: self.cancelled.clone(),
            search_generation: Arc::clone(&self.search_generation),
            workdir: self.workdir.clone(),
        }
    }
}

/// Where a test-only `PauseHook` stops the worker.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PausePoint {
    /// Between a command and its undo `finish` (`Worker::spawn_with_pause_before_finish`).
    BeforeFinish,
    /// When a command first finds the state lock busy, before its wait starts
    /// (`Worker::spawn_with_pause_on_lock_wait`, #274).
    LockWait,
}

/// Test-only pause of the worker at one `PausePoint`. Uninhabited outside tests, so the loop's
/// checks compile away.
#[cfg(not(test))]
enum PauseHook {}

#[cfg(not(test))]
impl PauseHook {
    fn pause_here(&self, _point: PausePoint) {
        match *self {}
    }
}

#[cfg(test)]
struct PauseHook {
    point: PausePoint,
    armed: Arc<std::sync::atomic::AtomicBool>,
    reached: Sender<()>,
    go: std::sync::Mutex<mpsc::Receiver<()>>,
}

#[cfg(test)]
impl PauseHook {
    fn new(point: PausePoint) -> (Self, PauseControl) {
        let armed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (reached_tx, reached_rx) = mpsc::channel();
        let (go_tx, go_rx) = mpsc::channel();
        (
            Self {
                point,
                armed: Arc::clone(&armed),
                reached: reached_tx,
                go: std::sync::Mutex::new(go_rx),
            },
            PauseControl {
                armed,
                reached: reached_rx,
                go: go_tx,
            },
        )
    }

    fn pause_here(&self, point: PausePoint) {
        if point == self.point && self.armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
            let _ = self.reached.send(());
            let _ = self.go.lock().unwrap().recv();
        }
    }
}

/// Drives a `PauseHook` from a test: `arm` before sending the command to pause, then
/// `wait_reached` (the worker stopped at the hook's `PausePoint`) and `go`.
#[cfg(test)]
pub(crate) struct PauseControl {
    armed: Arc<std::sync::atomic::AtomicBool>,
    reached: mpsc::Receiver<()>,
    go: Sender<()>,
}

#[cfg(test)]
impl PauseControl {
    pub fn arm(&self) {
        self.armed.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn wait_reached(&self) {
        self.reached
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the paused command never reached its pause point");
    }

    pub fn go(&self) {
        self.go.send(()).unwrap();
    }
}

impl WorkerHandle {
    /// Sends the command `build` makes around a fresh reply channel and waits for its reply.
    /// Every request goes through here, so a stopped worker always answers the same
    /// `ErrorKind::WorkerStopped` errors (#527).
    pub(crate) fn request<T>(
        &self,
        build: impl FnOnce(Sender<Result<T, ErrorDto>>) -> Command,
    ) -> Result<T, ErrorDto> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(build(reply))
            .map_err(|_| ErrorDto::worker_stopped())?;
        rx.recv()
            .map_err(|_| ErrorDto::worker_stopped_before_replying())?
    }

    /// True if the worker thread behind this handle is still running.
    ///
    /// `std::sync::mpsc::Sender::send` only ever fails once the paired `Receiver` has been
    /// dropped, which is exactly what happens when a panic inside a `Command` match arm unwinds
    /// the dispatch loop's thread — the `Receiver` it owns (`for command in rx`) drops as the
    /// stack unwinds. Sending a `Ping` and checking the `send` result (not waiting for the
    /// reply) detects that disconnection immediately, without blocking on a worker that is alive
    /// but simply busy with a slow command ahead of this one in the queue.
    pub fn is_alive(&self) -> bool {
        let (reply, _reply_rx) = mpsc::channel();
        self.tx.send(Command::Ping { reply }).is_ok()
    }

    /// Asks the in-flight transfer identified by `operation_id` to stop.
    ///
    /// Writes straight into the shared `CancelRegistry` — **never** through `self.tx`. A
    /// `Command` would be queued behind the blocking transfer itself and only be read once that
    /// transfer had already finished, which is exactly what this must not do. The `git2`
    /// progress callbacks poll this set from inside the blocked call and abort the transfer.
    ///
    /// A cancel for an ID this worker has not handed out, or whose command it is done with (a
    /// late click after the operation finished), is dropped (#219). The registry also keeps a
    /// cancel for a command that is still queued, defensively: the UI only learns an ID once
    /// the operation has started.
    pub fn cancel_transfer(&self, operation_id: &str) {
        self.cancelled.cancel(operation_id);
    }

    /// Test-only: crashes the worker thread from inside its dispatch loop, the same failure mode
    /// a panicking `git-core` call inside a `Command` match arm would trigger. See
    /// `Command::PanicNow`.
    #[cfg(test)]
    pub(crate) fn crash_for_test(&self) {
        let _ = self.tx.send(Command::PanicNow);
    }
}

#[cfg(test)]
mod tests;

/// #172: the worker holds the state lock across commands that can end an operation.
#[cfg(test)]
mod state_lock_tests;

#[cfg(test)]
mod policy_tests;
