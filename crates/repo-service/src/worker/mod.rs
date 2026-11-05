use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use git_core::blame::BlameLine;
use git_core::branch::BranchInfo;
use git_core::diff::DiffHunk;
use git_core::forge::{ForgeProvider, ForgeRepository};
use git_core::graph::GraphCommit;
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

#[cfg(not(feature = "forge-fixture-override"))]
use crate::credentials::KeyringCredentialStore;
use crate::credentials::{CredentialService, CredentialStore};
use crate::pull_requests::{
    CreatePullRequest, ForgeApi, PullRequest, PullRequestList, PullRequestService, ReqwestForgeApi,
};

mod atomic_file;
mod branch;
mod compare;
mod forge;
mod gate;
mod history;
mod hooks;
mod merge;
mod rebase;
mod rebase_store;
mod reflog;
mod registry;
mod remote;
mod search;
mod sequence_store;
mod stash;
mod state_lock;
mod status;
mod submodule;
mod tag;
mod tools;
mod undo;
mod undo_store;
mod worktree;

pub use hooks::{HookEvent, HookSource};
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
    /// loop below only echoes back on `reply`. Used by callers that hold a repo-path -> `Worker`
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
        reply: Sender<Result<Vec<StatusEntry>, String>>,
    },
    GetCommitGraph {
        limit: usize,
        selected_branches: Option<Vec<String>>,
        reply: Sender<Result<Vec<GraphCommit>, String>>,
    },
    GetWorkingDiff {
        path: String,
        staged: bool,
        opts: git_core::diff::DiffOptions,
        old_path: Option<String>,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetCommitDiff {
        commit_id: String,
        path: String,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetCommitFiles {
        commit_id: String,
        reply: Sender<Result<Vec<String>, String>>,
    },
    GetCommitMessage {
        commit_id: String,
        reply: Sender<Result<String, String>>,
    },
    GetCommitSignature {
        commit_id: String,
        reply: Sender<Result<git_core::signing::CommitSignature, String>>,
    },
    GetBlame {
        commit_id: String,
        path: String,
        reply: Sender<Result<Vec<BlameLine>, String>>,
    },
    GetFileHistory {
        path: String,
        limit: usize,
        reply: Sender<Result<Vec<git_core::history::FileHistoryEntry>, String>>,
    },
    /// One page of a commit search. `generation` is the `SearchGeneration` value claimed when
    /// the command was queued; once a newer search or `cancel_search` bumps the counter, this
    /// one stops and replies `Search cancelled.`.
    SearchCommits {
        query: String,
        refs: Option<Vec<String>>,
        cursor: Option<String>,
        generation: u64,
        reply: Sender<Result<git_core::search::SearchPage, String>>,
    },
    GetCommitRenameDiff {
        commit_id: String,
        old_path: String,
        new_path: String,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetRangeFiles {
        from_id: String,
        to_id: String,
        reply: Sender<Result<Vec<git_core::diff::ChangedFile>, String>>,
    },
    GetRangeDiff {
        from_id: String,
        to_id: String,
        path: String,
        old_path: Option<String>,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetWorkingTreeCompareFiles {
        commit_id: String,
        reply: Sender<Result<Vec<git_core::diff::ChangedFile>, String>>,
    },
    GetWorkingTreeCompareDiff {
        commit_id: String,
        path: String,
        old_path: Option<String>,
        opts: git_core::diff::DiffOptions,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetBranchCompareRange {
        ref_name: String,
        reply: Sender<Result<git_core::diff::CompareRange, String>>,
    },
    StageFile {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    UnstageFile {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    StageHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        reply: Sender<Result<(), String>>,
    },
    UnstageHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        reply: Sender<Result<(), String>>,
    },
    DiscardHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        reply: Sender<Result<(), String>>,
    },
    StageLines {
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
        reply: Sender<Result<(), String>>,
    },
    UnstageLines {
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
        reply: Sender<Result<(), String>>,
    },
    DiscardLines {
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
        reply: Sender<Result<(), String>>,
    },
    DiscardFile {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    DiscardAllUnstaged {
        paths: Vec<String>,
        reply: Sender<Result<(), String>>,
    },
    IgnorePath {
        pattern: String,
        reply: Sender<Result<(), String>>,
    },
    Commit {
        message: String,
        no_verify: bool,
        /// Appended to `message` before the hooks run (`Co-authored-by` from the commit box).
        trailers: Vec<git_core::commit::Trailer>,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::commit::CommitOutcome, String>>,
    },
    ListBranches {
        reply: Sender<Result<Vec<BranchInfo>, String>>,
    },
    CreateBranch {
        name: String,
        start_point: String,
        reply: Sender<Result<(), String>>,
    },
    /// `operation_id` (`switch-<n>`) is what `cancel_transfer` stops; `events` reports
    /// `post-checkout` (#185).
    SwitchBranch {
        name: String,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<(), String>>,
    },
    DeleteBranch {
        name: String,
        force: bool,
        reply: Sender<Result<(), String>>,
    },
    RenameBranch {
        old_name: String,
        new_name: String,
        reply: Sender<Result<(), String>>,
    },
    ListWorktrees {
        reply: Sender<Result<Vec<WorktreeInfo>, String>>,
    },
    CreateWorktree {
        name: String,
        path: PathBuf,
        branch: String,
        start_point: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    RemoveWorktree {
        name: String,
        reply: Sender<Result<(), String>>,
    },
    PruneWorktrees {
        reply: Sender<Result<(), String>>,
    },
    ListSubmodules {
        reply: Sender<Result<Vec<SubmoduleInfo>, String>>,
    },
    InitSubmodule {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    UpdateSubmodule {
        path: String,
        recursive: bool,
        reply: Sender<Result<(), String>>,
    },
    ListReflogRefs {
        reply: Sender<Result<Vec<String>, String>>,
    },
    GetReflog {
        reference: String,
        reply: Sender<Result<Vec<ReflogEntry>, String>>,
    },
    RestoreReflogEntry {
        reference: String,
        new_id: String,
        reply: Sender<Result<(), String>>,
    },
    ListRemotes {
        reply: Sender<Result<Vec<RemoteInfo>, String>>,
    },
    ListRemoteBranches {
        remote_name: String,
        reply: Sender<Result<Vec<String>, String>>,
    },
    GetCurrentUpstream {
        reply: Sender<Result<Option<UpstreamInfo>, String>>,
    },
    GetRemoteUpstreams {
        name: String,
        reply: Sender<Result<Vec<UpstreamInfo>, String>>,
    },
    GetRemoteAuthMode {
        name: String,
        reply: Sender<Result<Option<git_core::remote::RemoteAuthMode>, String>>,
    },
    SaveHttpsCredential {
        remote_name: String,
        username: String,
        token: String,
        reply: Sender<Result<(), String>>,
    },
    ForgetHttpsCredential {
        remote_name: String,
        reply: Sender<Result<(), String>>,
    },
    SetRemoteAuthMode {
        remote_name: String,
        mode: git_core::remote::RemoteAuthMode,
        reply: Sender<Result<(), String>>,
    },
    AddRemote {
        name: String,
        fetch_url: String,
        push_url: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    RenameRemote {
        old_name: String,
        new_name: String,
        reply: Sender<Result<(), String>>,
    },
    UpdateRemoteUrls {
        name: String,
        fetch_url: String,
        push_url: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    RemoveRemote {
        name: String,
        clear_upstreams: bool,
        reply: Sender<Result<(), String>>,
    },
    SetCurrentUpstream {
        remote_name: String,
        remote_branch: String,
        reply: Sender<Result<(), String>>,
    },
    ClearCurrentUpstream {
        reply: Sender<Result<(), String>>,
    },
    ListTags {
        reply: Sender<Result<Vec<TagInfo>, String>>,
    },
    CreateTag {
        name: String,
        message: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    DeleteTag {
        name: String,
        reply: Sender<Result<(), String>>,
    },
    ListStashes {
        reply: Sender<Result<Vec<StashEntry>, String>>,
    },
    SaveStash {
        options: git_core::stash::StashOptions,
        reply: Sender<Result<(), String>>,
    },
    ApplyStash {
        index: usize,
        commit_id: String,
        restore_index: bool,
        reply: Sender<Result<git_core::stash::StashApplyOutcome, String>>,
    },
    PopStash {
        index: usize,
        commit_id: String,
        restore_index: bool,
        reply: Sender<Result<git_core::stash::StashApplyOutcome, String>>,
    },
    GetStashFiles {
        commit_id: String,
        reply: Sender<Result<git_core::stash::StashFiles, String>>,
    },
    DropStash {
        index: usize,
        commit_id: String,
        reply: Sender<Result<(), String>>,
    },
    /// `operation_id` (`merge-<n>`) and `events`: a fast-forward's or squash's `post-merge`
    /// (#185).
    StartMerge {
        branch_name: String,
        options: git_core::merge::MergeOptions,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<MergeOutcome, String>>,
    },
    GetMergeDefaults {
        reply: Sender<Result<git_core::merge::MergeOptions, String>>,
    },
    #[allow(dead_code)]
    GetConflictHunks {
        path: String,
        reply: Sender<Result<Vec<ConflictSegment>, String>>,
    },
    #[allow(dead_code)]
    ResolveConflict {
        path: String,
        resolved_content: String,
        reply: Sender<Result<(), String>>,
    },
    #[allow(dead_code)]
    AbortMerge {
        reply: Sender<Result<(), String>>,
    },
    #[allow(dead_code)]
    GetMergeMessage {
        reply: Sender<Result<Option<String>, String>>,
    },
    ResolveAddDeleteConflict {
        path: String,
        choice: FileConflictChoice,
        reply: Sender<Result<(), String>>,
    },
    MarkResolved {
        path: String,
        force: bool,
        reply: Sender<Result<git_core::merge::MarkResolvedOutcome, String>>,
    },
    ResolveWholeFile {
        path: String,
        side: git_core::merge::ConflictSide,
        reply: Sender<Result<(), String>>,
    },
    PrepareDiffTool {
        path: String,
        source: git_core::tools::DiffSource,
        reply: Sender<Result<git_core::tools::PreparedDiffTool, String>>,
    },
    PrepareMergeTool {
        path: String,
        reply: Sender<Result<git_core::tools::PreparedMergeTool, String>>,
    },
    GetGitTools {
        reply: Sender<Result<git_core::tools::GitTools, String>>,
    },
    /// Both sides of `path` in `source`, for the image preview (`MAX_PREVIEW_BYTES` each).
    GetFileSides {
        path: String,
        source: git_core::tools::DiffSource,
        reply: Sender<Result<git_core::diff::FileSides, String>>,
    },
    ListRecentAuthors {
        reply: Sender<Result<Vec<git_core::commit::SignatureInput>, String>>,
    },
    CheckMergeTarget {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    ReadFileVersion {
        path: String,
        version: git_core::tools::FileVersion,
        reply: Sender<Result<Option<Vec<u8>>, String>>,
    },
    #[allow(dead_code)]
    CommitsSince {
        onto: String,
        reply: Sender<Result<Vec<RebasePlanCommit>, String>>,
    },
    #[allow(dead_code)]
    StartRebase {
        onto: String,
        plan: Vec<RebasePlanEntry>,
        reply: Sender<Result<RebaseStepResult, String>>,
    },
    #[allow(dead_code)]
    RebaseContinue {
        reply: Sender<Result<RebaseStepResult, String>>,
    },
    #[allow(dead_code)]
    AbortRebase {
        reply: Sender<Result<git_core::rebase::RebaseAbort, String>>,
    },
    FetchRemote {
        remote_name: String,
        prune: Option<bool>,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    PullCurrentUpstream {
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<PullOutcome, String>>,
    },
    GetHeadInfo {
        reply: Sender<Result<git_core::repo::HeadInfo, String>>,
    },
    AmendCommit {
        message: String,
        no_verify: bool,
        /// Replaces the author's name and email (the author time is kept).
        author: Option<git_core::commit::SignatureInput>,
        trailers: Vec<git_core::commit::Trailer>,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::commit::CommitOutcome, String>>,
    },
    /// The pick commands' `operation_id` (`pick-<n>`) and `events`: each clean step's
    /// `prepare-commit-msg`, signing and `post-commit` (#183).
    CherryPick {
        commit_id: String,
        mainline: Option<u32>,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::PickOutcome, String>>,
    },
    RevertCommit {
        commit_id: String,
        mainline: Option<u32>,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::PickOutcome, String>>,
    },
    CherryPickMany {
        commit_ids: Vec<String>,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::SequenceOutcome, String>>,
    },
    RevertMany {
        commit_ids: Vec<String>,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::SequenceOutcome, String>>,
    },
    /// Commits the resolved step through the commit hooks (#133): `no_verify` skips the
    /// skippable ones, `operation_id` (`continue-<n>`) is what `cancel_transfer` stops, and
    /// `events` reports each hook.
    ContinueSequence {
        no_verify: bool,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::pick::SequenceOutcome, String>>,
    },
    AbortSequence {
        /// For a terminal sequence: what the user confirmed (#281 review); a stored sequence
        /// ignores it.
        confirmed: Option<git_core::pick::ConfirmedAbort>,
        reply: Sender<Result<git_core::pick::SequenceAbort, String>>,
    },
    ResetToCommit {
        commit_id: String,
        mode: git_core::reset::ResetMode,
        reply: Sender<Result<(), String>>,
    },
    /// `operation_id` (`checkout-<n>`) and `events`: `post-checkout` (#185).
    CheckoutCommit {
        commit_id: String,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<(), String>>,
    },
    ForcePushCurrentBranch {
        remote_name: String,
        no_verify: bool,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    PushCurrentBranch {
        remote_name: String,
        no_verify: bool,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    PushTags {
        remote_name: String,
        names: Vec<String>,
        no_verify: bool,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    FetchAllRemotes {
        prune: Option<bool>,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    PushBranch {
        remote_name: String,
        local_branch: String,
        remote_branch: String,
        set_upstream: bool,
        no_verify: bool,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    DeleteRemoteBranch {
        remote_name: String,
        branch_name: String,
        no_verify: bool,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
    DetectForgeRepository {
        reply: Sender<Result<Vec<ForgeRepository>, String>>,
    },
    SaveForgeToken {
        provider: ForgeProvider,
        account: String,
        token: String,
        reply: Sender<Result<(), String>>,
    },
    ForgetForgeToken {
        provider: ForgeProvider,
        account: String,
        reply: Sender<Result<(), String>>,
    },
    ListPullRequests {
        remote_name: String,
        account: String,
        reply: Sender<Result<PullRequestList, String>>,
    },
    CreatePullRequest {
        remote_name: String,
        account: String,
        create: CreatePullRequest,
        reply: Sender<Result<PullRequest, String>>,
    },
    GetUndoState {
        reply: Sender<Result<UndoState, String>>,
    },
    Undo {
        reply: Sender<Result<UndoOutcome, String>>,
    },
    Redo {
        reply: Sender<Result<UndoOutcome, String>>,
    },
}

/// Operation IDs whose transfer the user asked to stop.
///
/// This is deliberately a *side channel* rather than another `Command`: the worker thread
/// processes one command at a time, and while a fetch/pull/push is blocked inside `git2`'s
/// network I/O it is not reading `rx` at all — a cancel sent through the command channel would
/// queue behind the very transfer it is meant to interrupt. `WorkerHandle::cancel_transfer`
/// therefore writes straight into this set, and `git-core`'s progress callbacks poll it on every
/// tick from inside the blocked call (see `worker/remote.rs`'s `cancel_check`).
pub type CancelRegistry = Arc<Mutex<HashSet<String>>>;

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
        S: CredentialStore + Send + 'static,
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
        S: CredentialStore + Send + 'static,
        A: ForgeApi + Send + 'static,
    {
        let repo_path = path;
        let repo = git_core::repo::open(&repo_path).map_err(|e| e.to_string())?;
        let workdir = repo.workdir().map(std::path::Path::to_path_buf);
        let (tx, rx) = mpsc::channel::<Command>();
        let cancelled: CancelRegistry = Arc::new(Mutex::new(HashSet::new()));
        let worker_cancelled = Arc::clone(&cancelled);
        let search_generation: SearchGeneration = Arc::new(AtomicU64::new(0));
        let worker_search_generation = Arc::clone(&search_generation);

        thread::spawn(move || {
            let cancelled = worker_cancelled;
            let search_generation = worker_search_generation;
            let mut repo = repo;
            let credential_service = CredentialService::new(credential_store);
            let pull_request_service = PullRequestService::new(forge_api);
            let mut undo_recorder = undo::UndoRecorder::open(&repo);
            // A stopped multi-commit cherry-pick/revert, persisted so it survives a restart.
            let sequence_store = sequence_store::SequenceStore::open(&repo);
            // A paused rebase, persisted likewise (`rebase.json`), plus this session's copy when
            // saving it failed. To git a paused rebase is a cherry-pick on a detached HEAD, or
            // nothing at all after an error stop, so only the store says a rebase is going on;
            // a sequence stopped by an error is clean too.
            let rebase_store = rebase_store::RebaseStore::open(&repo);
            let mut rebase_memory = gate::RebaseMemory::default();
            // The clean merge this worker staged, so its commit runs `git merge`'s hooks
            // (#124). Memory only: see `merge::CleanMerge` for the fallback (D2-A).
            let mut clean_merge: Option<merge::CleanMerge> = None;
            macro_rules! probe {
                () => {
                    gate::OperationProbe {
                        rebase: &rebase_store,
                        memory: &rebase_memory,
                        sequence: &sequence_store,
                    }
                };
            }
            // Names the operation in progress (`None`: nothing is), for the undo recorder.
            let marker = |repo: &git2::Repository, rebase_memory: &gate::RebaseMemory| {
                gate::OperationProbe {
                    rebase: &rebase_store,
                    memory: rebase_memory,
                    sequence: &sequence_store,
                }
                .marker(repo)
            };
            macro_rules! rebase_slot {
                () => {
                    rebase::RebaseSlot {
                        store: &rebase_store,
                        memory: &mut rebase_memory,
                    }
                };
            }
            // Undo bookkeeping is shared with other processes on this repository (#172).
            let state_lock = state_lock::StateLock::open(&repo);
            let (mut logged_busy, mut logged_unavailable) = (false, false);
            // After a command waited `LOCK_WAIT` in vain, later ones wait only `CONTENDED_WAIT`
            // until the lock is taken again, and while it keeps being found busy a run of them
            // waits that once, not once each; those that would refuse still wait (`Contention`). A long holder in the other window
            // (a pull, a hook, a pinentry dialog) costs one long wait, not one per click (the
            // queue, reads included, is behind it), while a brief hold is still waited out
            // (#175, #197).
            let mut contention = state_lock::Contention::default();
            for mut command in rx {
                // A command that can record an entry or end a paused operation holds the lock
                // from before `settle` until its `finish`, so no other holder ever sees "the
                // operation ended, but its entry isn't recorded yet". Reads never wait.
                let mut unrecorded = false;
                let guard = if undo::locks_state(&command) {
                    match state_lock.acquire_with(
                        contention
                            .wait(std::time::Instant::now(), undo::refuses_when_busy(&command)),
                        || {
                            if let Some(pause) = &pause {
                                pause.pause_here(PausePoint::LockWait);
                            }
                        },
                    ) {
                        state_lock::Acquire::Held(guard) => {
                            contention.held();
                            Some(guard)
                        }
                        state_lock::Acquire::Busy => {
                            contention.busy(std::time::Instant::now());
                            if undo::reply_busy(&mut command) {
                                continue;
                            }
                            if !logged_busy {
                                logged_busy = true;
                                eprintln!(
                                    "browsitory: undo history is locked by another window; \
                                     running the command without recording it for undo"
                                );
                            }
                            unrecorded = true;
                            None
                        }
                        state_lock::Acquire::Unavailable(error) => {
                            if !logged_unavailable {
                                logged_unavailable = true;
                                eprintln!("browsitory: undo history lock unavailable: {error}");
                            }
                            None
                        }
                    }
                } else {
                    None
                };
                undo_recorder.settle(&repo, guard.as_ref(), || marker(&repo, &rebase_memory));
                let plan = undo::plan_for(&command).filter(|_| !unrecorded);
                let track = plan.is_some() || undo_recorder.has_pending();
                let aborts = matches!(
                    command,
                    Command::AbortMerge { .. }
                        | Command::AbortRebase { .. }
                        | Command::AbortSequence { .. }
                );
                let mut held = undo::HeldReplies::default();
                if track {
                    undo::hold_reply(&mut command, &mut held);
                }
                if let Some(plan) = plan {
                    undo_recorder.begin(&repo, plan, marker(&repo, &rebase_memory).as_deref());
                }
                // While a cherry-pick/revert sequence or a rebase is stored, commands that would
                // move HEAD or rewrite the index under it are refused before git-core runs.
                if let Some(scope) = probe!().gate_scope(&repo) {
                    command = gate::gate_during_operation(command, &scope);
                }
                match command {
                    Command::Ping { reply } => {
                        let _ = reply.send(());
                    }
                    #[cfg(test)]
                    Command::PanicNow => {
                        panic!("intentional panic for worker-crash-recovery test")
                    }
                    Command::GetStatus { reply } => status::get_status(&repo, reply),
                    Command::GetCommitGraph {
                        limit,
                        selected_branches,
                        reply,
                    } => status::get_commit_graph(&repo, limit, selected_branches, reply),
                    Command::GetWorkingDiff {
                        path,
                        staged,
                        opts,
                        old_path,
                        reply,
                    } => status::get_working_diff(&repo, path, staged, opts, old_path, reply),
                    Command::GetCommitDiff {
                        commit_id,
                        path,
                        opts,
                        reply,
                    } => status::get_commit_diff(&repo, commit_id, path, opts, reply),
                    Command::GetCommitFiles { commit_id, reply } => {
                        status::get_commit_files(&repo, commit_id, reply)
                    }
                    Command::GetCommitMessage { commit_id, reply } => {
                        status::get_commit_message(&repo, commit_id, reply)
                    }
                    Command::GetCommitSignature { commit_id, reply } => {
                        status::get_commit_signature(&repo, commit_id, reply)
                    }
                    Command::GetBlame {
                        commit_id,
                        path,
                        reply,
                    } => status::get_blame(&repo, commit_id, path, reply),
                    Command::GetFileHistory { path, limit, reply } => {
                        status::get_file_history(&repo, path, limit, reply)
                    }
                    Command::SearchCommits {
                        query,
                        refs,
                        cursor,
                        generation,
                        reply,
                    } => search::search_commits(
                        &repo,
                        &search_generation,
                        generation,
                        query,
                        refs,
                        cursor,
                        reply,
                    ),
                    Command::GetCommitRenameDiff {
                        commit_id,
                        old_path,
                        new_path,
                        opts,
                        reply,
                    } => status::get_commit_rename_diff(
                        &repo, commit_id, old_path, new_path, opts, reply,
                    ),
                    Command::GetRangeFiles {
                        from_id,
                        to_id,
                        reply,
                    } => compare::get_range_files(&repo, from_id, to_id, reply),
                    Command::GetRangeDiff {
                        from_id,
                        to_id,
                        path,
                        old_path,
                        opts,
                        reply,
                    } => {
                        compare::get_range_diff(&repo, from_id, to_id, path, old_path, opts, reply)
                    }
                    Command::GetWorkingTreeCompareFiles { commit_id, reply } => {
                        compare::get_working_tree_compare_files(&repo, commit_id, reply)
                    }
                    Command::GetWorkingTreeCompareDiff {
                        commit_id,
                        path,
                        old_path,
                        opts,
                        reply,
                    } => compare::get_working_tree_compare_diff(
                        &repo, commit_id, path, old_path, opts, reply,
                    ),
                    Command::GetBranchCompareRange { ref_name, reply } => {
                        compare::get_branch_compare_range(&repo, ref_name, reply)
                    }
                    Command::StageFile { path, reply } => status::stage_file(&repo, path, reply),
                    Command::UnstageFile { path, reply } => {
                        status::unstage_file(&repo, path, reply)
                    }
                    Command::StageHunk {
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        reply,
                    } => {
                        status::stage_hunk(&repo, path, old_start, new_start, context_lines, reply)
                    }
                    Command::UnstageHunk {
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        reply,
                    } => status::unstage_hunk(
                        &repo,
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        reply,
                    ),
                    Command::DiscardHunk {
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        reply,
                    } => status::discard_hunk(
                        &repo,
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        reply,
                    ),
                    Command::StageLines {
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        line_indices,
                        expected_lines,
                        reply,
                    } => status::stage_lines(
                        &repo,
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        line_indices,
                        expected_lines,
                        reply,
                    ),
                    Command::UnstageLines {
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        line_indices,
                        expected_lines,
                        reply,
                    } => status::unstage_lines(
                        &repo,
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        line_indices,
                        expected_lines,
                        reply,
                    ),
                    Command::DiscardLines {
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        line_indices,
                        expected_lines,
                        reply,
                    } => status::discard_lines(
                        &repo,
                        path,
                        old_start,
                        new_start,
                        context_lines,
                        line_indices,
                        expected_lines,
                        reply,
                    ),
                    Command::DiscardFile { path, reply } => {
                        status::discard_file(&repo, path, reply)
                    }
                    Command::DiscardAllUnstaged { paths, reply } => {
                        status::discard_all_unstaged(&repo, paths, reply)
                    }
                    Command::IgnorePath { pattern, reply } => {
                        status::ignore_path(&repo, pattern, reply)
                    }
                    Command::Commit {
                        message,
                        no_verify,
                        trailers,
                        operation_id,
                        events,
                        reply,
                    } => {
                        let set = match &clean_merge {
                            Some(merge) if merge.matches(&mut repo) => merge.hook_set(),
                            _ => git_core::commit::CommitHookSet::Commit,
                        };
                        let committed = status::commit(
                            &mut repo,
                            message,
                            no_verify,
                            trailers,
                            set,
                            remote::TransferOp {
                                id: operation_id,
                                cancelled: &cancelled,
                            },
                            events,
                            reply,
                        );
                        // A failed or cancelled commit keeps it: "Commit without hooks" still
                        // concludes the clean merge (`git merge --no-verify`).
                        if committed {
                            clean_merge = None;
                        }
                    }
                    Command::ListBranches { reply } => branch::list(&repo, reply),
                    Command::CreateBranch {
                        name,
                        start_point,
                        reply,
                    } => branch::create(&repo, name, start_point, reply),
                    Command::SwitchBranch {
                        name,
                        operation_id,
                        events,
                        reply,
                    } => branch::switch(
                        &repo,
                        name,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::DeleteBranch { name, force, reply } => {
                        branch::delete(&repo, name, force, reply)
                    }
                    Command::RenameBranch {
                        old_name,
                        new_name,
                        reply,
                    } => branch::rename(&repo, old_name, new_name, reply),
                    Command::ListWorktrees { reply } => worktree::list(&repo, reply),
                    Command::CreateWorktree {
                        name,
                        path,
                        branch,
                        start_point,
                        reply,
                    } => worktree::create(
                        &repo_path,
                        &mut repo,
                        name,
                        path,
                        branch,
                        start_point,
                        reply,
                    ),
                    Command::RemoveWorktree { name, reply } => {
                        worktree::remove(&repo_path, &mut repo, name, reply)
                    }
                    Command::PruneWorktrees { reply } => {
                        worktree::prune(&repo_path, &mut repo, reply)
                    }
                    Command::ListSubmodules { reply } => submodule::list(&repo, reply),
                    Command::InitSubmodule { path, reply } => submodule::init(&repo, path, reply),
                    Command::UpdateSubmodule {
                        path,
                        recursive,
                        reply,
                    } => submodule::update(&repo, path, recursive, reply),
                    Command::ListReflogRefs { reply } => reflog::list_refs(&repo, reply),
                    Command::GetReflog { reference, reply } => reflog::get(&repo, reference, reply),
                    Command::RestoreReflogEntry {
                        reference,
                        new_id,
                        reply,
                    } => reflog::restore(&repo, reference, new_id, reply),
                    Command::ListRemotes { reply } => remote::list(&repo, reply),
                    Command::ListRemoteBranches { remote_name, reply } => {
                        remote::list_branches(&repo, remote_name, reply)
                    }
                    Command::GetCurrentUpstream { reply } => remote::current_upstream(&repo, reply),
                    Command::AddRemote {
                        name,
                        fetch_url,
                        push_url,
                        reply,
                    } => remote::add(&repo, name, fetch_url, push_url, reply),
                    Command::RenameRemote {
                        old_name,
                        new_name,
                        reply,
                    } => remote::rename(&repo, old_name, new_name, reply),
                    Command::UpdateRemoteUrls {
                        name,
                        fetch_url,
                        push_url,
                        reply,
                    } => remote::update_urls(&repo, name, fetch_url, push_url, reply),
                    Command::GetRemoteUpstreams { name, reply } => {
                        remote::upstreams(&repo, name, reply)
                    }
                    Command::GetRemoteAuthMode { name, reply } => {
                        remote::auth_mode(&repo, name, reply)
                    }
                    Command::SaveHttpsCredential {
                        remote_name,
                        username,
                        token,
                        reply,
                    } => remote::save_https(
                        &repo,
                        &credential_service,
                        remote_name,
                        username,
                        token,
                        reply,
                    ),
                    Command::ForgetHttpsCredential { remote_name, reply } => {
                        remote::forget_https(&repo, &credential_service, remote_name, reply)
                    }
                    Command::SetRemoteAuthMode {
                        remote_name,
                        mode,
                        reply,
                    } => remote::set_auth_mode(&repo, remote_name, mode, reply),
                    Command::RemoveRemote {
                        name,
                        clear_upstreams,
                        reply,
                    } => remote::remove(&repo, name, clear_upstreams, reply),
                    Command::SetCurrentUpstream {
                        remote_name,
                        remote_branch,
                        reply,
                    } => remote::set_current_upstream(&repo, remote_name, remote_branch, reply),
                    Command::ClearCurrentUpstream { reply } => {
                        remote::clear_current_upstream(&repo, reply)
                    }
                    Command::ListTags { reply } => tag::list(&repo, reply),
                    Command::CreateTag {
                        name,
                        message,
                        reply,
                    } => tag::create(&repo, name, message, reply),
                    Command::DeleteTag { name, reply } => tag::delete(&repo, name, reply),
                    Command::ListStashes { reply } => stash::list(&mut repo, reply),
                    Command::SaveStash { options, reply } => stash::save(&mut repo, options, reply),
                    Command::ApplyStash {
                        index,
                        commit_id,
                        restore_index,
                        reply,
                    } => stash::apply(&mut repo, index, &commit_id, restore_index, reply),
                    Command::PopStash {
                        index,
                        commit_id,
                        restore_index,
                        reply,
                    } => stash::pop(&mut repo, index, &commit_id, restore_index, reply),
                    Command::GetStashFiles { commit_id, reply } => {
                        stash::files(&mut repo, commit_id, reply)
                    }
                    Command::DropStash {
                        index,
                        commit_id,
                        reply,
                    } => stash::drop(&mut repo, index, &commit_id, reply),
                    Command::StartMerge {
                        branch_name,
                        options,
                        operation_id,
                        events,
                        reply,
                    } => {
                        let result = merge::start(
                            &repo,
                            branch_name,
                            options,
                            remote::TransferOp {
                                id: operation_id,
                                cancelled: &cancelled,
                            },
                            events,
                            reply,
                        );
                        match result {
                            Ok(MergeOutcome::Merged) => {
                                clean_merge = merge::CleanMerge::capture(&mut repo)
                            }
                            Ok(_) => clean_merge = None,
                            // Refused (e.g. "already merging"): nothing changed, so a staged
                            // clean merge is still the one remembered.
                            Err(_) => {}
                        }
                    }
                    Command::GetMergeDefaults { reply } => merge::defaults(&repo, reply),
                    Command::GetConflictHunks { path, reply } => {
                        merge::conflict_hunks(&repo, path, reply)
                    }
                    Command::ResolveConflict {
                        path,
                        resolved_content,
                        reply,
                    } => merge::resolve(&repo, path, resolved_content, reply),
                    Command::AbortMerge { reply } => {
                        clean_merge = None;
                        // A stale UI's Abort during a sequence must not strand sequence.json.
                        if sequence_store.load(&repo).is_some() {
                            history::abort_sequence_for_merge(&repo, &sequence_store, reply)
                        } else if let Some(refusal) = history::abort_merge_refusal(&repo) {
                            // A terminal sequence: only AbortSequence ends it (#253).
                            let _ = reply.send(Err(refusal));
                        } else {
                            merge::abort(&repo, reply)
                        }
                    }
                    Command::GetHeadInfo { reply } => {
                        let clean = clean_merge.as_ref().is_some_and(|m| m.matches(&mut repo));
                        history::head_info(&repo, &probe!(), clean, reply)
                    }
                    Command::AmendCommit {
                        message,
                        no_verify,
                        author,
                        trailers,
                        operation_id,
                        events,
                        reply,
                    } => history::amend(
                        &repo,
                        message,
                        no_verify,
                        author,
                        trailers,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::CherryPick {
                        commit_id,
                        mainline,
                        operation_id,
                        events,
                        reply,
                    } => history::pick_one(
                        &repo,
                        git_core::pick::SequenceKind::CherryPick,
                        commit_id,
                        mainline,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::RevertCommit {
                        commit_id,
                        mainline,
                        operation_id,
                        events,
                        reply,
                    } => history::pick_one(
                        &repo,
                        git_core::pick::SequenceKind::Revert,
                        commit_id,
                        mainline,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::CherryPickMany {
                        commit_ids,
                        operation_id,
                        events,
                        reply,
                    } => {
                        let kind = git_core::pick::SequenceKind::CherryPick;
                        let result = history::pick_many(
                            &repo,
                            &sequence_store,
                            kind,
                            commit_ids,
                            remote::TransferOp {
                                id: operation_id,
                                cancelled: &cancelled,
                            },
                            events,
                        );
                        undo::relabel_sequence(&mut undo_recorder, kind, &result);
                        let _ = reply.send(result);
                    }
                    Command::RevertMany {
                        commit_ids,
                        operation_id,
                        events,
                        reply,
                    } => {
                        let kind = git_core::pick::SequenceKind::Revert;
                        let result = history::pick_many(
                            &repo,
                            &sequence_store,
                            kind,
                            commit_ids,
                            remote::TransferOp {
                                id: operation_id,
                                cancelled: &cancelled,
                            },
                            events,
                        );
                        undo::relabel_sequence(&mut undo_recorder, kind, &result);
                        let _ = reply.send(result);
                    }
                    Command::ContinueSequence {
                        no_verify,
                        operation_id,
                        events,
                        reply,
                    } => {
                        let (kind, result) = history::continue_sequence(
                            &repo,
                            &sequence_store,
                            no_verify,
                            remote::TransferOp {
                                id: operation_id,
                                cancelled: &cancelled,
                            },
                            events,
                        );
                        if let Some(kind) = kind {
                            undo::relabel_sequence(&mut undo_recorder, kind, &result);
                        }
                        let _ = reply.send(result);
                    }
                    Command::AbortSequence { confirmed, reply } => {
                        history::abort_sequence(&repo, &sequence_store, confirmed, reply)
                    }
                    Command::ResetToCommit {
                        commit_id,
                        mode,
                        reply,
                    } => history::reset(&repo, commit_id, mode, reply),
                    Command::CheckoutCommit {
                        commit_id,
                        operation_id,
                        events,
                        reply,
                    } => history::checkout(
                        &repo,
                        commit_id,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::ForcePushCurrentBranch {
                        remote_name,
                        no_verify,
                        operation_id,
                        events,
                        reply,
                    } => remote::force_push_branch(
                        &repo,
                        &credential_service,
                        remote_name,
                        no_verify,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::GetMergeMessage { reply } => merge::message(&repo, reply),
                    Command::ResolveAddDeleteConflict {
                        path,
                        choice,
                        reply,
                    } => merge::resolve_add_delete(&repo, path, choice, reply),
                    Command::MarkResolved { path, force, reply } => {
                        merge::mark_resolved(&repo, path, force, reply)
                    }
                    Command::ResolveWholeFile { path, side, reply } => {
                        merge::resolve_whole_file(&repo, path, side, reply)
                    }
                    Command::PrepareDiffTool {
                        path,
                        source,
                        reply,
                    } => tools::prepare_diff_tool(&repo, path, source, reply),
                    Command::PrepareMergeTool { path, reply } => {
                        tools::prepare_merge_tool(&repo, path, reply)
                    }
                    Command::GetGitTools { reply } => tools::git_tools(&repo, reply),
                    Command::GetFileSides {
                        path,
                        source,
                        reply,
                    } => tools::file_sides(&repo, path, source, reply),
                    Command::ListRecentAuthors { reply } => history::recent_authors(&repo, reply),
                    Command::CheckMergeTarget { path, reply } => {
                        tools::check_merge_target(&repo, path, reply)
                    }
                    Command::ReadFileVersion {
                        path,
                        version,
                        reply,
                    } => tools::read_file_version(&repo, path, version, reply),
                    Command::CommitsSince { onto, reply } => {
                        rebase::commits_since(&repo, onto, reply)
                    }
                    Command::StartRebase { onto, plan, reply } => {
                        let result = rebase::start(&repo, &onto, plan, rebase_slot!());
                        undo::relabel_rebase_start(&mut undo_recorder, &repo, &onto, &result);
                        let _ = reply.send(result);
                    }
                    Command::RebaseContinue { reply } => {
                        rebase::continue_rebase(&repo, rebase_slot!(), reply)
                    }
                    Command::AbortRebase { reply } => rebase::abort(&repo, rebase_slot!(), reply),
                    Command::FetchRemote {
                        remote_name,
                        prune,
                        operation_id,
                        events,
                        reply,
                    } => remote::fetch(
                        &repo,
                        &credential_service,
                        remote_name,
                        prune,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::PullCurrentUpstream {
                        operation_id,
                        events,
                        reply,
                    } => remote::pull(
                        &repo,
                        &credential_service,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::PushCurrentBranch {
                        remote_name,
                        no_verify,
                        operation_id,
                        events,
                        reply,
                    } => remote::push_current(
                        &repo,
                        &credential_service,
                        remote_name,
                        no_verify,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::PushTags {
                        remote_name,
                        names,
                        no_verify,
                        operation_id,
                        events,
                        reply,
                    } => remote::push_tags(
                        &repo,
                        &credential_service,
                        remote_name,
                        names,
                        no_verify,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::FetchAllRemotes {
                        prune,
                        operation_id,
                        events,
                        reply,
                    } => remote::fetch_all(
                        &repo,
                        &credential_service,
                        prune,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::PushBranch {
                        remote_name,
                        local_branch,
                        remote_branch,
                        set_upstream,
                        no_verify,
                        operation_id,
                        events,
                        reply,
                    } => remote::push_branch(
                        &repo,
                        &credential_service,
                        remote::PushTarget {
                            remote_name,
                            local_branch,
                            remote_branch,
                            set_upstream,
                            no_verify,
                        },
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::DeleteRemoteBranch {
                        remote_name,
                        branch_name,
                        no_verify,
                        operation_id,
                        events,
                        reply,
                    } => remote::delete_remote_branch(
                        &repo,
                        &credential_service,
                        remote_name,
                        branch_name,
                        no_verify,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::DetectForgeRepository { reply } => forge::detect(&repo, reply),
                    Command::SaveForgeToken {
                        provider,
                        account,
                        token,
                        reply,
                    } => forge::save_token(&credential_service, provider, account, token, reply),
                    Command::ForgetForgeToken {
                        provider,
                        account,
                        reply,
                    } => forge::forget_token(&credential_service, provider, account, reply),
                    Command::ListPullRequests {
                        remote_name,
                        account,
                        reply,
                    } => forge::list_pull_requests(
                        &repo,
                        &credential_service,
                        &pull_request_service,
                        remote_name,
                        account,
                        reply,
                    ),
                    Command::CreatePullRequest {
                        remote_name,
                        account,
                        create,
                        reply,
                    } => forge::create_pull_request(
                        &repo,
                        &credential_service,
                        &pull_request_service,
                        remote_name,
                        account,
                        create,
                        reply,
                    ),
                    Command::GetUndoState { reply } => undo::get_state(&mut undo_recorder, reply),
                    Command::Undo { reply } => {
                        let _ = reply.send(
                            undo_recorder
                                .undo(&repo, guard.as_ref(), || marker(&repo, &rebase_memory)),
                        );
                    }
                    Command::Redo { reply } => {
                        let _ = reply.send(
                            undo_recorder
                                .redo(&repo, guard.as_ref(), || marker(&repo, &rebase_memory)),
                        );
                    }
                }
                if let Some(pause) = &pause {
                    pause.pause_here(PausePoint::BeforeFinish);
                }
                if track {
                    undo_recorder.finish(
                        &repo,
                        guard.as_ref(),
                        || marker(&repo, &rebase_memory),
                        aborts,
                    );
                }
                drop(guard);
                held.release();
                if unrecorded {
                    contention.finished(std::time::Instant::now());
                }
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
            cancelled: Arc::clone(&self.cancelled),
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
    go: Mutex<mpsc::Receiver<()>>,
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
                go: Mutex::new(go_rx),
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
    /// Marking an operation that is not (or no longer) running is harmless: the entry is
    /// removed when a transfer with that ID completes, and an ID that never runs only occupies
    /// one string until the worker is dropped.
    pub fn cancel_transfer(&self, operation_id: &str) {
        self.cancelled
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(operation_id.to_string());
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
