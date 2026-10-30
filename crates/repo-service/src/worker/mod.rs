use std::collections::{HashMap, HashSet};
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
mod remote;
mod search;
mod sequence_store;
mod stash;
mod status;
mod submodule;
mod tag;
mod tools;
mod undo;
mod undo_store;
mod worktree;

pub use hooks::HookEvent;
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
    SwitchBranch {
        name: String,
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
    StartMerge {
        branch_name: String,
        options: git_core::merge::MergeOptions,
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
    CherryPick {
        commit_id: String,
        mainline: Option<u32>,
        reply: Sender<Result<git_core::pick::PickOutcome, String>>,
    },
    RevertCommit {
        commit_id: String,
        mainline: Option<u32>,
        reply: Sender<Result<git_core::pick::PickOutcome, String>>,
    },
    CherryPickMany {
        commit_ids: Vec<String>,
        reply: Sender<Result<git_core::pick::SequenceOutcome, String>>,
    },
    RevertMany {
        commit_ids: Vec<String>,
        reply: Sender<Result<git_core::pick::SequenceOutcome, String>>,
    },
    ContinueSequence {
        reply: Sender<Result<git_core::pick::SequenceOutcome, String>>,
    },
    AbortSequence {
        reply: Sender<Result<git_core::pick::SequenceAbort, String>>,
    },
    ResetToCommit {
        commit_id: String,
        mode: git_core::reset::ResetMode,
        reply: Sender<Result<(), String>>,
    },
    CheckoutCommit {
        commit_id: String,
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
            for mut command in rx {
                undo_recorder.settle(&repo, marker(&repo, &rebase_memory).as_deref());
                let plan = undo::plan_for(&command);
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
                    } => status::commit(
                        &mut repo,
                        message,
                        no_verify,
                        trailers,
                        remote::TransferOp {
                            id: operation_id,
                            cancelled: &cancelled,
                        },
                        events,
                        reply,
                    ),
                    Command::ListBranches { reply } => branch::list(&repo, reply),
                    Command::CreateBranch {
                        name,
                        start_point,
                        reply,
                    } => branch::create(&repo, name, start_point, reply),
                    Command::SwitchBranch { name, reply } => branch::switch(&repo, name, reply),
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
                        reply,
                    } => merge::start(&repo, branch_name, options, reply),
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
                        // A stale UI's Abort during a sequence must not strand sequence.json.
                        if sequence_store.load(&repo).is_some() {
                            history::abort_sequence_for_merge(&repo, &sequence_store, reply)
                        } else {
                            merge::abort(&repo, reply)
                        }
                    }
                    Command::GetHeadInfo { reply } => history::head_info(&repo, &probe!(), reply),
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
                        reply,
                    } => history::cherry_pick(&repo, commit_id, mainline, reply),
                    Command::RevertCommit {
                        commit_id,
                        mainline,
                        reply,
                    } => history::revert(&repo, commit_id, mainline, reply),
                    Command::CherryPickMany { commit_ids, reply } => {
                        let kind = git_core::pick::SequenceKind::CherryPick;
                        let result = history::pick_many(&repo, &sequence_store, kind, commit_ids);
                        undo::relabel_sequence(&mut undo_recorder, kind, &result);
                        let _ = reply.send(result);
                    }
                    Command::RevertMany { commit_ids, reply } => {
                        let kind = git_core::pick::SequenceKind::Revert;
                        let result = history::pick_many(&repo, &sequence_store, kind, commit_ids);
                        undo::relabel_sequence(&mut undo_recorder, kind, &result);
                        let _ = reply.send(result);
                    }
                    Command::ContinueSequence { reply } => {
                        let (kind, result) = history::continue_sequence(&repo, &sequence_store);
                        if let Some(kind) = kind {
                            undo::relabel_sequence(&mut undo_recorder, kind, &result);
                        }
                        let _ = reply.send(result);
                    }
                    Command::AbortSequence { reply } => {
                        history::abort_sequence(&repo, &sequence_store, reply)
                    }
                    Command::ResetToCommit {
                        commit_id,
                        mode,
                        reply,
                    } => history::reset(&repo, commit_id, mode, reply),
                    Command::CheckoutCommit { commit_id, reply } => {
                        history::checkout(&repo, commit_id, reply)
                    }
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
                            undo_recorder.undo(&repo, marker(&repo, &rebase_memory).as_deref()),
                        );
                    }
                    Command::Redo { reply } => {
                        let _ = reply.send(
                            undo_recorder.redo(&repo, marker(&repo, &rebase_memory).as_deref()),
                        );
                    }
                }
                if track {
                    undo_recorder.finish(&repo, marker(&repo, &rebase_memory).as_deref(), aborts);
                }
                held.release();
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

/// Returns a live `WorkerHandle` for `path`, evicting and respawning first if `workers` already
/// holds an entry whose worker thread has since exited — most notably after a panic inside a
/// `Command` match arm (see AUD-2026-09-05-CONC-001). Without this check, a caller's naive
/// "already open, reuse it" fast path (a plain `contains_key`/`Entry::Vacant` test) would keep
/// handing back a dead handle forever, since the map itself never notices its `Worker` stopped
/// responding.
///
/// Takes the map's `Mutex` rather than an already-locked `&mut HashMap` so it can enforce the
/// same "drop the lock before doing any slow work" discipline `tauri-app`'s `AppState` and
/// `vscode-sidecar`'s `Repos` both rely on elsewhere: the lock here is held only for the cheap
/// lookup/evict and the final insert, never across `Worker::spawn` (which opens a `git2::
/// Repository` and can be slow), so a repo that's slow to open can't block a concurrent request
/// against a different, already-open repo.
///
/// Shared by `tauri-app`'s `AppState` and `vscode-sidecar`'s repo map so the eviction rule can't
/// drift between the two frontends.
pub fn ensure_worker(
    workers: &Mutex<HashMap<String, Worker>>,
    path: &str,
) -> Result<WorkerHandle, String> {
    {
        let mut guard = workers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(worker) = guard.get(path) {
            let handle = worker.handle();
            if handle.is_alive() {
                return Ok(handle);
            }
            // The worker thread already exited (most likely a panic inside a `Command` match
            // arm) — evict the stale entry so the fresh spawn below actually replaces it instead
            // of leaving this path permanently wedged behind a `contains_key`-style fast path.
            guard.remove(path);
        }
    }

    let worker = Worker::spawn(PathBuf::from(path))?;
    let handle = worker.handle();
    let mut guard = workers.lock().unwrap_or_else(|e| e.into_inner());
    // Another caller may have opened (or re-opened, after its own eviction) the same
    // never-before-open path while we were spawning, unguarded, above. Prefer whichever worker
    // is already sitting in the map: this loser's `Worker` (and its `Sender`) is simply dropped,
    // letting its otherwise-idle thread exit immediately — wasted work, not a correctness issue.
    match guard.entry(path.to_string()) {
        std::collections::hash_map::Entry::Occupied(entry) => Ok(entry.get().handle()),
        std::collections::hash_map::Entry::Vacant(entry) => {
            entry.insert(worker);
            Ok(handle)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::path::Path;
    use std::sync::atomic::Ordering;
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    use git2::Repository;
    use git_core::forge::ForgeProvider;
    use git_core::merge::MergeOutcome;
    use git_core::remote::{TransferErrorKind, TransferOperation};
    use tempfile::TempDir;

    #[cfg(unix)]
    use super::HookEvent;
    use super::{ensure_worker, Command, TransferEvent, UndoOutcome, UndoState, Worker};
    use crate::credentials::{CredentialKey, CredentialStore, CredentialStoreError};
    use crate::pull_requests::{ForgeApiError, ForgeHttpRequest, ForgeHttpResponse};

    /// In-memory `CredentialStore` fake for worker tests — the design doc's "in-memory trait
    /// implementations only at external seams such as the keychain" rule applies here just as
    /// it does to `credentials.rs`'s own tests; the real OS keychain is never touched.
    #[derive(Default)]
    struct FakeCredentialStore {
        tokens: RefCell<HashMap<CredentialKey, String>>,
    }

    impl CredentialStore for FakeCredentialStore {
        fn get(&self, key: &CredentialKey) -> Result<Option<String>, CredentialStoreError> {
            Ok(self.tokens.borrow().get(key).cloned())
        }

        fn set(&self, key: &CredentialKey, token: &str) -> Result<(), CredentialStoreError> {
            self.tokens
                .borrow_mut()
                .insert(key.clone(), token.to_owned());
            Ok(())
        }

        fn delete(&self, key: &CredentialKey) -> Result<(), CredentialStoreError> {
            self.tokens.borrow_mut().remove(key);
            Ok(())
        }
    }

    /// Canned-response `ForgeApi` fake — the other external seam the design doc calls out.
    /// Records every request it receives so tests can assert on headers (e.g. the injected
    /// `Authorization` token) without ever making a real HTTP call. `Arc<Mutex<..>>`-backed
    /// (rather than `RefCell`, like `pull_requests.rs`'s own same-thread fake) because this one
    /// crosses the worker's background-thread boundary: a clone kept in the test can still
    /// inspect requests recorded by the clone moved into `Worker::spawn_with`.
    #[derive(Clone, Default)]
    struct FakeForgeApi {
        responses: Arc<Mutex<VecDeque<Result<ForgeHttpResponse, ForgeApiError>>>>,
        requests: Arc<Mutex<Vec<ForgeHttpRequest>>>,
    }

    impl FakeForgeApi {
        fn queue(responses: Vec<Result<ForgeHttpResponse, ForgeApiError>>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses.into_iter().collect())),
                requests: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn requests(&self) -> Vec<ForgeHttpRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl crate::pull_requests::ForgeApi for FakeForgeApi {
        fn send(&self, request: ForgeHttpRequest) -> Result<ForgeHttpResponse, ForgeApiError> {
            self.requests.lock().unwrap().push(request);
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("test queued an unexpected extra request")
        }
    }

    fn ok(status: u16, body: &str) -> Result<ForgeHttpResponse, ForgeApiError> {
        Ok(ForgeHttpResponse {
            status,
            body: body.to_string(),
            headers: Vec::new(),
        })
    }

    const GITHUB_LIST_FIXTURE: &str = r#"[
        {
            "id": 101,
            "number": 7,
            "title": "Add pull request support",
            "html_url": "https://github.com/acme/widget/pull/7",
            "user": {"login": "rene"},
            "head": {"ref": "feature/pr"},
            "base": {"ref": "main"},
            "state": "open"
        }
    ]"#;

    fn init_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().expect("create temp dir");
        let repo = Repository::init(dir.path()).expect("init repo");
        {
            let mut config = repo.config().expect("repo config");
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
            config.set_bool("core.autocrlf", false).unwrap();
            config.set_str("core.eol", "lf").unwrap();
            config.set_bool("core.safecrlf", false).unwrap();
            config.set_bool("commit.gpgSign", false).unwrap();
            // Pin the hooks dir (git's default) so a global `core.hooksPath` never runs here.
            let hooks = repo.path().join("hooks");
            config
                .set_str("core.hooksPath", hooks.to_str().expect("utf-8 temp path"))
                .unwrap();
        }
        (dir, repo)
    }

    #[cfg(unix)]
    fn install_hook(repo: &Repository, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let dir = git_core::hooks::hooks_dir(repo).expect("hooks dir");
        std::fs::create_dir_all(&dir).expect("create hooks dir");
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write hook");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_pre_commit_replies_a_hook_failed_outcome_and_records_no_undo() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        install_hook(&repo, "pre-commit", "echo nope; exit 1");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();
        let undo_before = handle.get_undo_state().unwrap().undo;
        let (events_tx, events_rx) = mpsc::channel();

        let outcome = handle
            .commit("msg".into(), false, Vec::new(), events_tx)
            .unwrap();

        assert_eq!(
            outcome,
            git_core::commit::CommitOutcome::HookFailed {
                hook: "pre-commit".into(),
                output: "nope\n".into()
            }
        );
        let events: Vec<HookEvent> = events_rx.iter().collect();
        assert!(
            matches!(&events[..], [
                HookEvent::Started { operation_id: a, hook: h1 },
                HookEvent::Finished { operation_id: b, hook: h2 },
            ] if a == b && a.starts_with("commit-") && h1 == "pre-commit" && h2 == "pre-commit"),
            "{events:?}"
        );
        assert_eq!(handle.get_undo_state().unwrap().undo, undo_before);
        assert!(repo.head().is_err(), "nothing may be committed");
    }

    #[cfg(unix)]
    #[test]
    fn cancel_transfer_stops_a_running_pre_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        install_hook(&repo, "pre-commit", "sleep 30");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();
        let (events_tx, events_rx) = mpsc::channel();
        let canceller = handle.clone();
        let cancel_thread = std::thread::spawn(move || {
            if let Ok(HookEvent::Started { operation_id, .. }) = events_rx.recv() {
                canceller.cancel_transfer(&operation_id);
            }
        });
        let started = std::time::Instant::now();

        let result = handle.commit("msg".into(), false, Vec::new(), events_tx);

        cancel_thread.join().unwrap();
        assert_eq!(result.unwrap_err(), "The pre-commit hook was cancelled.");
        // Generous bound: only proves the hook's `sleep 30` was not waited out.
        assert!(started.elapsed() < Duration::from_secs(25));
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_pre_push_completes_with_hook_failed_and_its_output() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let local = Repository::open(local_dir.path()).unwrap();
        write_file(local_dir.path(), "new.txt", "new");
        commit_all(&local, "to push");
        let branch = local.head().unwrap().shorthand().unwrap().to_string();
        install_hook(&local, "pre-push", "echo 'tests failed'; exit 1");
        drop(local);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let (event_tx, event_rx) = mpsc::channel();
        handle
            .push_branch(
                "origin".into(),
                branch.clone(),
                "topic".into(),
                false,
                false,
                event_tx,
            )
            .unwrap();
        let events: Vec<_> = event_rx.iter().collect();
        assert!(
            events.iter().any(|event| matches!(
                event,
                TransferEvent::Progress(progress)
                    if progress.phase == git_core::remote::TransferPhase::RunningHook
            )),
            "{events:?}"
        );
        assert!(
            matches!(
                events.last(),
                Some(TransferEvent::Completed {
                    error: Some(TransferErrorKind::HookFailed),
                    hook_output: Some(output),
                    ..
                }) if output == "tests failed\n"
            ),
            "{events:?}"
        );

        let (event_tx, event_rx) = mpsc::channel();
        handle
            .push_branch(
                "origin".into(),
                branch,
                "topic".into(),
                false,
                true,
                event_tx,
            )
            .unwrap();
        let events: Vec<_> = event_rx.iter().collect();
        assert!(
            matches!(
                events.last(),
                Some(TransferEvent::Completed {
                    error: None,
                    hook_output: None,
                    ..
                })
            ),
            "{events:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn post_checkout_gets_old_and_new_head_after_switch_and_checkout_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "first");
        let first = repo.head().unwrap().target().unwrap();
        repo.branch("other", &repo.find_commit(first).unwrap(), false)
            .unwrap();
        write_file(dir.path(), "a.txt", "b");
        commit_all(&repo, "second");
        let second = repo.head().unwrap().target().unwrap();
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        let record = dir.path().join("post-checkout.txt");
        // Fails on purpose: a post-checkout failure must not fail the switch.
        install_hook(
            &repo,
            "post-checkout",
            &format!(r#"echo "$@" >> '{}'; exit 1"#, record.display()),
        );
        std::fs::write(
            dir.path().join(".gitignore"),
            "post-checkout.txt\n.gitignore\n",
        )
        .unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        handle.switch_branch("other".into()).unwrap();
        handle.checkout_commit(second.to_string()).unwrap();
        handle.switch_branch(main).unwrap();

        assert_eq!(
            std::fs::read_to_string(&record).unwrap(),
            format!("{second} {first} 1\n{first} {second} 1\n{second} {second} 1\n")
        );
    }

    #[cfg(unix)]
    #[test]
    fn post_merge_runs_after_a_merge_and_a_squash() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "first");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        let first = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &first, false).unwrap();
        repo.branch("other", &first, false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        write_file(dir.path(), "b.txt", "b");
        commit_all(&repo, "feature work");
        repo.set_head("refs/heads/other").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        write_file(dir.path(), "c.txt", "c");
        commit_all(&repo, "other work");
        repo.set_head(&format!("refs/heads/{main}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        let record = repo.path().join("post-merge.txt");
        install_hook(
            &repo,
            "post-merge",
            &format!(r#"echo "$@" >> '{}'"#, record.display()),
        );
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let merged = handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();
        assert_eq!(merged, MergeOutcome::FastForwarded);
        let squashed = handle
            .start_merge(
                "other".into(),
                git_core::merge::MergeOptions {
                    squash: true,
                    ..git_core::merge::MergeOptions::default()
                },
            )
            .unwrap();
        assert_eq!(squashed, MergeOutcome::Squashed);

        assert_eq!(std::fs::read_to_string(&record).unwrap(), "0\n1\n");
    }

    #[cfg(unix)]
    #[test]
    fn amend_with_no_verify_skips_a_failing_pre_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "first");
        install_hook(&repo, "pre-commit", "exit 1");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let failed = handle
            .amend_commit("amended".into(), false, None, Vec::new(), mpsc::channel().0)
            .unwrap();
        assert!(matches!(
            failed,
            git_core::commit::CommitOutcome::HookFailed { .. }
        ));
        let outcome = handle
            .amend_commit("amended".into(), true, None, Vec::new(), mpsc::channel().0)
            .unwrap();

        assert!(matches!(
            outcome,
            git_core::commit::CommitOutcome::Committed { .. }
        ));
    }

    fn co_author(value: &str) -> git_core::commit::Trailer {
        git_core::commit::Trailer {
            key: "Co-authored-by".into(),
            value: value.into(),
        }
    }

    fn head_commit_message(repo: &Repository) -> String {
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap()
            .to_string()
    }

    #[test]
    fn get_file_sides_returns_both_sides_of_an_unstaged_edit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "img.png", "foo");
        commit_all(&repo, "first");
        write_file(dir.path(), "img.png", "foobar");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let sides = worker
            .handle()
            .get_file_sides("img.png".into(), git_core::tools::DiffSource::Unstaged)
            .unwrap();
        assert_eq!(
            sides.old,
            git_core::diff::FileSide::Loaded {
                bytes: b"foo".to_vec()
            }
        );
        assert_eq!(
            sides.new,
            git_core::diff::FileSide::Loaded {
                bytes: b"foobar".to_vec()
            }
        );
        assert!(worker
            .handle()
            .get_file_sides("../x.png".into(), git_core::tools::DiffSource::Unstaged)
            .unwrap_err()
            .contains("path must be relative to the repository"));
    }

    #[test]
    fn commit_appends_trailers_and_refuses_an_invalid_one() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();

        let bad = git_core::commit::Trailer {
            key: "Bad key".into(),
            value: "x".into(),
        };
        assert_eq!(
            handle
                .commit("Add a".into(), false, vec![bad], mpsc::channel().0)
                .unwrap_err(),
            "Invalid trailer \"Bad key: x\"."
        );
        assert!(repo.head().is_err(), "nothing was committed");

        let outcome = handle
            .commit(
                "Add a".into(),
                false,
                vec![co_author("Ada <ada@x.org>")],
                mpsc::channel().0,
            )
            .unwrap();
        assert!(matches!(
            outcome,
            git_core::commit::CommitOutcome::Committed { .. }
        ));
        assert_eq!(
            head_commit_message(&repo),
            "Add a\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn an_empty_message_with_trailers_is_refused_for_commit_and_amend() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();

        assert_eq!(
            handle
                .commit(
                    " \n\n".into(),
                    false,
                    vec![co_author("Ada <ada@x.org>")],
                    mpsc::channel().0,
                )
                .unwrap_err(),
            "Aborting commit due to empty commit message."
        );
        assert!(repo.head().is_err(), "nothing was committed");

        handle
            .commit("Add a".into(), false, vec![], mpsc::channel().0)
            .unwrap();
        let before = repo.head().unwrap().target();
        assert_eq!(
            handle
                .amend_commit(
                    "".into(),
                    false,
                    None,
                    vec![co_author("Ada <ada@x.org>")],
                    mpsc::channel().0,
                )
                .unwrap_err(),
            "Aborting commit due to empty commit message."
        );
        assert_eq!(
            repo.head().unwrap().target(),
            before,
            "HEAD was not amended"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_commit_msg_hook_sees_the_trailers() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        let seen = dir.path().join("seen.txt");
        install_hook(
            &repo,
            "commit-msg",
            &format!("cp \"$1\" '{}'", seen.display()),
        );
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();
        handle
            .commit(
                "Add a".into(),
                false,
                vec![co_author("Ada <ada@x.org>")],
                mpsc::channel().0,
            )
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&seen).unwrap(),
            "Add a\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn amend_commit_overrides_the_author_and_appends_trailers() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "first");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .amend_commit(
                "first".into(),
                false,
                Some(git_core::commit::SignatureInput {
                    name: "Ada".into(),
                    email: "ada@x.org".into(),
                }),
                vec![co_author("Bob <bob@x.org>")],
                mpsc::channel().0,
            )
            .unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.author().name().ok(), Some("Ada"));
        assert_eq!(head.author().email().ok(), Some("ada@x.org"));
        assert_eq!(head.committer().name().ok(), Some("Test User"));
        assert_eq!(
            head.message().ok(),
            Some("first\n\nCo-authored-by: Bob <bob@x.org>\n")
        );
        drop(head);

        let before = repo.head().unwrap().target();
        assert_eq!(
            handle
                .amend_commit(
                    "first".into(),
                    false,
                    Some(git_core::commit::SignatureInput {
                        name: "Ada".into(),
                        email: "".into(),
                    }),
                    Vec::new(),
                    mpsc::channel().0,
                )
                .unwrap_err(),
            "Invalid author. Use a name and an email address."
        );
        assert_eq!(repo.head().unwrap().target(), before);
    }

    #[test]
    fn list_recent_authors_excludes_the_current_user() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "mine");
        write_file(dir.path(), "b.txt", "b");
        {
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("b.txt")).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let ada = git2::Signature::now("Ada", "ada@x.org").unwrap();
            let parent = repo.head().unwrap().peel_to_commit().unwrap();
            repo.commit(Some("HEAD"), &ada, &ada, "hers", &tree, &[&parent])
                .unwrap();
        }
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        assert_eq!(
            worker.handle().list_recent_authors().unwrap(),
            vec![git_core::commit::SignatureInput {
                name: "Ada".into(),
                email: "ada@x.org".into()
            }]
        );
    }

    fn write_file(dir: &Path, relative_path: &str, contents: &str) {
        std::fs::write(dir.join(relative_path), contents).expect("write file");
    }

    /// Stages everything in the worktree and commits it on `HEAD`, creating the first commit
    /// when there is none yet. Mirrors `crates/git-core/tests/common/mod.rs::commit_all`.
    fn commit_all(repo: &Repository, message: &str) {
        let mut index = repo.index().expect("open index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("stage all");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let signature = repo.signature().expect("signature");

        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .expect("commit");
    }

    fn local_and_bare_remote() -> (TempDir, TempDir, TempDir) {
        let (source_dir, source) = init_repo();
        write_file(source_dir.path(), "README.md", "initial commit\n");
        commit_all(&source, "initial commit");

        let remote_dir = TempDir::new().expect("create bare remote");
        let remote = Repository::init_bare(remote_dir.path()).expect("init bare remote");
        let branch = source
            .head()
            .expect("source head")
            .shorthand()
            .expect("source branch")
            .to_string();
        let branch_ref = format!("refs/heads/{branch}");
        source
            .remote("origin", remote_dir.path().to_str().expect("remote path"))
            .expect("add source remote");
        source
            .find_remote("origin")
            .expect("source remote")
            .push(&[format!("{branch_ref}:{branch_ref}")], None)
            .expect("push source commit");
        drop(remote);

        let (local_dir, local) = init_repo();
        local
            .remote("origin", remote_dir.path().to_str().expect("remote path"))
            .expect("add local remote");

        (source_dir, remote_dir, local_dir)
    }

    #[test]
    fn get_status_reflects_an_untracked_file() {
        let (dir, _repo) = init_repo();
        write_file(dir.path(), "untracked.txt", "hello");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let entries = worker.handle().get_status().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "untracked.txt");
    }

    #[test]
    fn spawn_fails_on_a_non_repository_path() {
        let dir = TempDir::new().unwrap();

        let result = Worker::spawn(dir.path().to_path_buf());

        assert!(result.is_err());
    }

    #[test]
    fn get_commit_graph_reflects_a_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "hello");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let commits = worker.handle().get_commit_graph(10, None).unwrap();

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].summary, "initial commit");
    }

    #[test]
    fn stage_then_commit_round_trips_through_the_worker() {
        let (dir, _repo) = init_repo();
        write_file(dir.path(), "new.txt", "hello");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("new.txt".into()).unwrap();
        let result = handle.commit("message".into(), false, Vec::new(), mpsc::channel().0);

        assert!(result.is_ok());
        assert!(handle.get_status().unwrap().is_empty());
    }

    #[test]
    fn get_commit_signature_reports_unsigned_for_a_plain_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "plain");
        let id = repo.head().unwrap().target().unwrap().to_string();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let signature = worker.handle().get_commit_signature(id).unwrap();

        assert_eq!(
            signature.status,
            git_core::signing::SignatureStatus::Unsigned
        );
    }

    #[test]
    fn workdir_is_the_work_tree_root_even_when_opened_from_a_subdirectory() {
        let (dir, repo) = init_repo();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        write_file(dir.path(), "sub/a.txt", "a");
        commit_all(&repo, "initial");
        let worker = Worker::spawn(dir.path().join("sub")).unwrap();
        assert_eq!(
            worker.handle().workdir().unwrap().canonicalize().unwrap(),
            dir.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn external_tool_commands_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "old");
        commit_all(&repo, "initial");
        write_file(dir.path(), "a.txt", "new");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        assert_eq!(
            handle
                .read_file_version("a.txt".into(), git_core::tools::FileVersion::Index)
                .unwrap(),
            Some(b"old".to_vec())
        );
        // Hermetic error paths: both are checked before any git config is read.
        assert_eq!(
            handle
                .prepare_diff_tool("../a.txt".into(), git_core::tools::DiffSource::Unstaged)
                .unwrap_err(),
            "Invalid path."
        );
        assert_eq!(
            handle.prepare_merge_tool("a.txt".into()).unwrap_err(),
            "no conflict found for path 'a.txt'"
        );
        assert!(handle.get_git_tools().is_ok());
        assert_eq!(
            handle.check_merge_target("a.txt".into()).unwrap_err(),
            "no conflict found for path 'a.txt'"
        );
    }

    #[test]
    fn mark_resolved_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "base\n");
        commit_all(&repo, "base");
        let main = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();
        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "a.txt", "theirs\n");
        commit_all(&repo, "feature");
        git_core::branch::switch_branch(&repo, &main).unwrap();
        write_file(dir.path(), "a.txt", "ours\n");
        commit_all(&repo, "main");
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.check_merge_target("a.txt".into()).unwrap();
        assert_eq!(
            handle.mark_resolved("a.txt".into(), false).unwrap(),
            git_core::merge::MarkResolvedOutcome::MarkersRemain
        );
        assert_eq!(
            handle.mark_resolved("a.txt".into(), true).unwrap(),
            git_core::merge::MarkResolvedOutcome::Resolved
        );
    }

    /// base: a.txt="base\n". feature: "f one" (+b.txt), "f two" (a.txt="feature\n"),
    /// "f three" (+c.txt). The base branch (checked out): a.txt="main\n". Returns the feature
    /// ids oldest-first.
    fn side_commits_with_a_conflict() -> (TempDir, Repository, Vec<String>) {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "base\n");
        commit_all(&repo, "base");
        let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();
        {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("feature", &head, false).unwrap();
        }
        repo.set_head("refs/heads/feature").unwrap();
        let mut ids = Vec::new();
        for (path, contents, message) in [
            ("b.txt", "b\n", "f one"),
            ("a.txt", "feature\n", "f two"),
            ("c.txt", "c\n", "f three"),
        ] {
            write_file(dir.path(), path, contents);
            commit_all(&repo, message);
            ids.push(repo.head().unwrap().target().unwrap().to_string());
        }
        repo.set_head(&format!("refs/heads/{base_branch}")).unwrap();
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new()
                .force()
                .remove_untracked(true),
        ))
        .unwrap();
        write_file(dir.path(), "a.txt", "main\n");
        commit_all(&repo, "main change");
        (dir, repo, ids)
    }

    fn sequence_file(repo: &Repository) -> std::path::PathBuf {
        repo.path().join("browsitory").join("sequence.json")
    }

    #[test]
    fn a_conflicted_sequence_finished_by_continue_is_one_undo_entry() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        let original = repo.head().unwrap().target().unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let outcome = handle.cherry_pick_many(ids.clone()).unwrap();
        assert!(matches!(
            outcome,
            git_core::pick::SequenceOutcome::Conflicted { .. }
        ));
        assert!(sequence_file(&repo).exists());
        let progress = handle.get_head_info().unwrap().sequence.unwrap();
        assert_eq!((progress.current_step, progress.total_steps), (2, 3));
        assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
        let busy = "Finish or abort the current merge/cherry-pick/revert/rebase first.";
        assert_eq!(handle.cherry_pick(ids[0].clone(), None).unwrap_err(), busy);
        assert_eq!(handle.revert_many(ids.clone()).unwrap_err(), busy);
        assert_eq!(
            handle
                .start_merge("feature".into(), git_core::merge::MergeOptions::default())
                .unwrap_err(),
            busy
        );

        handle
            .resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        let outcome = handle.continue_sequence().unwrap();
        assert_eq!(
            outcome,
            git_core::pick::SequenceOutcome::Completed {
                applied: 3,
                skipped: vec![]
            }
        );
        assert_eq!(handle.get_head_info().unwrap().sequence, None);
        assert!(!sequence_file(&repo).exists());
        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Cherry-pick 3 commits".into())
        );
        assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
        assert_eq!(repo.head().unwrap().target().unwrap(), original);
    }

    #[test]
    fn a_stored_sequence_refuses_everything_that_moves_head_or_the_index() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.cherry_pick_many(ids.clone()).unwrap();
        let base = repo.head().unwrap().target().unwrap().to_string();
        let busy = "Finish or abort the current merge/cherry-pick/revert/rebase first.";

        assert_eq!(
            handle
                .commit("x".into(), false, Vec::new(), mpsc::channel().0)
                .unwrap_err(),
            busy
        );
        assert_eq!(
            handle
                .reset_to_commit(base.clone(), git_core::reset::ResetMode::Hard)
                .unwrap_err(),
            busy
        );
        assert_eq!(handle.checkout_commit(base.clone()).unwrap_err(), busy);
        assert_eq!(handle.switch_branch("feature".into()).unwrap_err(), busy);
        assert_eq!(
            handle.pull_current_upstream(mpsc::channel().0).unwrap_err(),
            busy
        );
        assert_eq!(handle.apply_stash(0, "s".into(), false).unwrap_err(), busy);
        assert_eq!(handle.pop_stash(0, "s".into(), false).unwrap_err(), busy);
        assert_eq!(
            handle
                .restore_reflog_entry("HEAD".into(), base.clone())
                .unwrap_err(),
            busy
        );
        // Read-only and resolving commands still work.
        assert!(handle.get_status().is_ok());
        handle
            .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
            .unwrap();
        assert!(handle.continue_sequence().is_ok());
    }

    #[test]
    fn the_undo_label_counts_the_commits_that_were_applied() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        // "f one" is already on the branch: it is skipped, so two commits are applied.
        git_core::pick::cherry_pick(&repo, &ids[0], None).unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.cherry_pick_many(ids.clone()).unwrap();
        handle
            .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
            .unwrap();
        assert_eq!(
            handle.continue_sequence().unwrap(),
            git_core::pick::SequenceOutcome::Completed {
                applied: 2,
                skipped: vec![ids[0].clone()],
            }
        );
        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Cherry-pick 2 commits".into())
        );
    }

    #[test]
    fn an_aborted_sequence_records_nothing() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        let original = repo.head().unwrap().target().unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert_eq!(
            handle.revert_many(vec![]).unwrap_err(),
            "Select at least one commit."
        );
        assert!(!sequence_file(&repo).exists());
        handle.cherry_pick_many(ids).unwrap();
        assert_eq!(
            handle.abort_sequence().unwrap(),
            git_core::pick::SequenceAbort { rewound: true }
        );
        assert_eq!(repo.head().unwrap().target().unwrap(), original);
        assert_eq!(handle.get_head_info().unwrap().sequence, None);
        assert!(!sequence_file(&repo).exists());
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState {
                undo: None,
                redo: None
            }
        );
        let none = "No cherry-pick or revert is in progress.";
        assert_eq!(handle.continue_sequence().unwrap_err(), none);
        assert_eq!(handle.abort_sequence().unwrap_err(), none);
    }

    #[test]
    fn abort_merge_during_a_sequence_clears_it() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        let original = repo.head().unwrap().target().unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.cherry_pick_many(ids).unwrap();
        handle.abort_merge().unwrap();
        assert_eq!(handle.get_head_info().unwrap().sequence, None);
        assert!(!sequence_file(&repo).exists());
        assert_eq!(repo.state(), git2::RepositoryState::Clean);
        assert_eq!(repo.head().unwrap().target().unwrap(), original);
    }

    /// A cherry-pick sequence of `side_commits_with_a_conflict` stopped on its conflict (#171).
    /// Returns the branch it runs on (short name).
    fn paused_sequence() -> (TempDir, Repository, Worker, super::WorkerHandle, String) {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert!(matches!(
            handle.cherry_pick_many(ids).unwrap(),
            git_core::pick::SequenceOutcome::Conflicted { .. }
        ));
        (dir, repo, worker, handle, branch)
    }

    fn in_progress() -> String {
        git_core::pick::PickError::OperationInProgress.to_string()
    }

    #[test]
    fn renaming_the_sequence_branch_is_refused_while_a_sequence_is_paused() {
        let (_dir, repo, _worker, handle, branch) = paused_sequence();
        assert_eq!(
            handle.rename_branch(branch.clone(), "other".into()),
            Err(in_progress())
        );
        assert!(repo.find_branch(&branch, git2::BranchType::Local).is_ok());
        assert_eq!(
            handle.abort_sequence().unwrap(),
            git_core::pick::SequenceAbort { rewound: true }
        );
    }

    #[test]
    fn deleting_the_sequence_branch_is_refused_while_a_sequence_is_paused() {
        let (_dir, repo, _worker, handle, branch) = paused_sequence();
        // Move HEAD off the branch (as `git switch` in a terminal), so libgit2's own refusal to
        // delete the current branch doesn't hide the gate.
        repo.cleanup_state().unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        assert_eq!(
            handle.delete_branch(branch.clone(), true),
            Err(in_progress())
        );
        assert!(repo.find_branch(&branch, git2::BranchType::Local).is_ok());
    }

    #[test]
    fn adding_a_worktree_on_the_sequence_branch_is_refused() {
        let (dir, _repo, _worker, handle, branch) = paused_sequence();
        let path = dir.path().with_extension("wt");
        assert_eq!(
            handle.create_worktree("wt".into(), path.clone(), branch, None),
            Err(in_progress())
        );
        assert!(!path.exists());
    }

    #[test]
    fn stash_save_is_refused_while_a_sequence_is_paused() {
        let (dir, repo, _worker, handle, _branch) = paused_sequence();
        write_file(dir.path(), "b.txt", "edited\n");
        assert_eq!(
            handle.save_stash(git_core::stash::StashOptions::default()),
            Err(in_progress())
        );
        assert!(repo.find_reference("refs/stash").is_err());
        assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
    }

    #[test]
    fn other_branches_stay_manageable_during_a_paused_sequence() {
        let (_dir, repo, _worker, handle, _branch) = paused_sequence();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("spare", &head, false).unwrap();
        handle
            .rename_branch("spare".into(), "spare2".into())
            .unwrap();
        handle.delete_branch("spare2".into(), true).unwrap();
        assert!(repo.find_branch("spare2", git2::BranchType::Local).is_err());
    }

    #[test]
    fn a_stopped_sequence_survives_a_worker_restart() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        {
            let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
            worker.handle().cherry_pick_many(ids).unwrap();
        }
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let progress = handle.get_head_info().unwrap().sequence.unwrap();
        assert_eq!(progress.current_step, 2);
        handle
            .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
            .unwrap();
        assert_eq!(
            handle.continue_sequence().unwrap(),
            git_core::pick::SequenceOutcome::Completed {
                applied: 3,
                skipped: vec![]
            }
        );
        assert!(!sequence_file(&repo).exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "feature\n"
        );
    }

    #[test]
    fn a_sequence_stopped_by_an_error_blocks_undo_and_offers_continue() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        // c.txt untracked in the way of step three.
        write_file(dir.path(), "c.txt", "mine\n");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        // Step two conflicts first; take theirs and continue into step three's error.
        handle.cherry_pick_many(ids).unwrap();
        handle
            .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
            .unwrap();
        assert!(handle.continue_sequence().is_err());
        assert_eq!(repo.state(), git2::RepositoryState::Clean);
        let progress = handle.get_head_info().unwrap().sequence.unwrap();
        assert_eq!(progress.current_step, 3);
        assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
        std::fs::remove_file(dir.path().join("c.txt")).unwrap();
        assert!(matches!(
            handle.continue_sequence().unwrap(),
            git_core::pick::SequenceOutcome::Completed { applied: 3, .. }
        ));
        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Cherry-pick 3 commits".into())
        );
    }

    #[test]
    fn a_corrupt_sequence_file_never_blocks_the_repository() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        std::fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        std::fs::write(sequence_file(&repo), "{not json").unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert_eq!(handle.get_head_info().unwrap().sequence, None);
        assert!(matches!(
            handle.cherry_pick(ids[0].clone(), None).unwrap(),
            git_core::pick::PickOutcome::Committed { .. }
        ));
        assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
    }

    #[test]
    fn whole_file_resolution_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "base\n");
        commit_all(&repo, "base");
        let main = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();
        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "a.txt", "theirs\n");
        commit_all(&repo, "feature");
        git_core::branch::switch_branch(&repo, &main).unwrap();
        write_file(dir.path(), "a.txt", "ours\n");
        commit_all(&repo, "main");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();
        handle
            .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "theirs\n"
        );
        assert!(!repo.index().unwrap().has_conflicts());
    }

    fn two_commit_repo() -> (TempDir, Repository) {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "alpha commit");
        write_file(dir.path(), "b.txt", "b");
        commit_all(&repo, "beta commit");
        (dir, repo)
    }

    #[test]
    fn search_commits_round_trips_through_the_worker() {
        let (dir, _repo) = two_commit_repo();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

        let page = worker
            .handle()
            .search_commits("alpha".into(), None, None)
            .unwrap();

        assert_eq!(page.commits.len(), 1);
        assert_eq!(page.commits[0].summary, "alpha commit");
        assert_eq!(page.next_cursor, None);
        assert_eq!(page.scanned, 2);
    }

    #[test]
    fn an_invalid_search_query_replies_its_message() {
        let (dir, _repo) = two_commit_repo();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

        assert_eq!(
            worker
                .handle()
                .search_commits("author:".into(), None, None)
                .unwrap_err(),
            "author: needs a value."
        );
    }

    #[test]
    fn a_search_superseded_before_it_runs_replies_cancelled() {
        let (dir, _repo) = two_commit_repo();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        // Claim a generation the way `search_commits` does, then let a newer search/cancel
        // supersede it before the command is even queued.
        let stale = handle.search_generation.fetch_add(1, Ordering::SeqCst) + 1;
        handle.cancel_search();
        let (tx, rx) = mpsc::channel();
        handle
            .tx
            .send(Command::SearchCommits {
                query: "alpha".into(),
                refs: None,
                cursor: None,
                generation: stale,
                reply: tx,
            })
            .unwrap();

        assert_eq!(rx.recv().unwrap().unwrap_err(), "Search cancelled.");
        // A fresh search after the cancel runs normally.
        assert_eq!(
            handle
                .search_commits("alpha".into(), None, None)
                .unwrap()
                .commits
                .len(),
            1
        );
    }

    #[test]
    fn failed_signing_replies_err_and_records_no_undo_entry() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "base");
        {
            let mut config = repo.config().unwrap();
            config.set_bool("commit.gpgSign", true).unwrap();
            config
                .set_str("gpg.program", "browsitory-no-such-gpg")
                .unwrap();
        }
        write_file(dir.path(), "b.txt", "b");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("b.txt".into()).unwrap();
        let undo_before = handle.get_undo_state().unwrap().undo;
        let result = handle.commit("signed?".into(), false, Vec::new(), mpsc::channel().0);

        assert_eq!(
            result.unwrap_err(),
            "Signing program 'browsitory-no-such-gpg' was not found."
        );
        assert_eq!(handle.get_undo_state().unwrap().undo, undo_before);
    }

    #[test]
    fn stage_hunk_then_commit_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "tracked.txt", "line one\nline two\n");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let hunks = handle
            .get_working_diff(
                "tracked.txt".into(),
                false,
                git_core::diff::DiffOptions::default(),
                None,
            )
            .unwrap();
        assert_eq!(hunks.len(), 1);

        handle
            .stage_hunk(
                "tracked.txt".into(),
                hunks[0].old_start,
                hunks[0].new_start,
                3,
            )
            .unwrap();

        let staged = handle.get_status().unwrap();
        assert!(staged.iter().any(|e| e.path == "tracked.txt" && e.staged));
    }

    #[test]
    fn unstage_hunk_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "tracked.txt", "line one\nline two\n");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let hunks = handle
            .get_working_diff(
                "tracked.txt".into(),
                false,
                git_core::diff::DiffOptions::default(),
                None,
            )
            .unwrap();
        handle
            .stage_hunk(
                "tracked.txt".into(),
                hunks[0].old_start,
                hunks[0].new_start,
                3,
            )
            .unwrap();

        handle
            .unstage_hunk(
                "tracked.txt".into(),
                hunks[0].old_start,
                hunks[0].new_start,
                3,
            )
            .unwrap();

        let status = handle.get_status().unwrap();
        assert!(!status.iter().any(|e| e.path == "tracked.txt" && e.staged));
    }

    #[test]
    fn discard_hunk_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "tracked.txt", "line one\nline two\n");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let hunks = handle
            .get_working_diff(
                "tracked.txt".into(),
                false,
                git_core::diff::DiffOptions::default(),
                None,
            )
            .unwrap();

        handle
            .discard_hunk(
                "tracked.txt".into(),
                hunks[0].old_start,
                hunks[0].new_start,
                3,
            )
            .unwrap();

        let on_disk = std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap();
        assert_eq!(on_disk.replace("\r\n", "\n"), "line one\nline two\n");
    }

    #[test]
    fn a_staged_rename_diffs_old_against_new() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "old.txt", "one\ntwo\nthree\nfour\n");
        commit_all(&repo, "base");
        std::fs::remove_file(dir.path().join("old.txt")).unwrap();
        write_file(dir.path(), "new.txt", "one\nTWO\nthree\nfour\n");
        let mut index = repo.index().unwrap();
        index.remove_path(std::path::Path::new("old.txt")).unwrap();
        index.add_path(std::path::Path::new("new.txt")).unwrap();
        index.write().unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let opts = git_core::diff::DiffOptions::default();
        let hunks = handle
            .get_working_diff("new.txt".into(), true, opts, Some("old.txt".into()))
            .unwrap();
        let edits: Vec<_> = hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .filter(|l| l.origin != git_core::diff::DiffLineOrigin::Context)
            .map(|l| l.content.clone())
            .collect();
        assert_eq!(edits, vec!["two\n".to_string(), "TWO\n".to_string()]);

        // Without old_path a staged rename is a whole-file add, as before.
        let added = handle
            .get_working_diff("new.txt".into(), true, opts, None)
            .unwrap();
        assert_eq!(added[0].lines.len(), 4);
    }

    #[test]
    fn line_and_file_commands_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "f.txt", "a\nb\nc\n");
        commit_all(&repo, "base");
        write_file(dir.path(), "f.txt", "a\nB\nc\nd\n");
        write_file(dir.path(), "junk.log", "x\n");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let opts = git_core::diff::DiffOptions::default();
        let hunk = handle
            .get_working_diff("f.txt".into(), false, opts, None)
            .unwrap()
            .remove(0);

        let shown = |h: &git_core::diff::DiffHunk| -> Vec<String> {
            h.lines.iter().map(|l| l.content.clone()).collect()
        };
        // A hunk shown with different content (stale view) is refused, nothing changes.
        let stale = handle.stage_lines(
            "f.txt".into(),
            hunk.old_start,
            hunk.new_start,
            3,
            vec![4],
            vec!["not what is on disk\n".into()],
        );
        assert!(stale.unwrap_err().contains("hunk not found"));
        assert!(handle
            .get_working_diff("f.txt".into(), true, opts, None)
            .unwrap()
            .is_empty());

        handle
            .stage_lines(
                "f.txt".into(),
                hunk.old_start,
                hunk.new_start,
                3,
                vec![4],
                shown(&hunk),
            )
            .unwrap();
        let staged = handle
            .get_working_diff("f.txt".into(), true, opts, None)
            .unwrap()
            .remove(0);
        handle
            .unstage_lines(
                "f.txt".into(),
                staged.old_start,
                staged.new_start,
                3,
                vec![staged.lines.len() as u32 - 1],
                shown(&staged),
            )
            .unwrap();
        assert!(handle
            .get_working_diff("f.txt".into(), true, opts, None)
            .unwrap()
            .is_empty());

        let hunk = handle
            .get_working_diff("f.txt".into(), false, opts, None)
            .unwrap()
            .remove(0);
        handle
            .discard_lines(
                "f.txt".into(),
                hunk.old_start,
                hunk.new_start,
                3,
                vec![4],
                shown(&hunk),
            )
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "a\nB\nc\n"
        );

        handle.ignore_path("*.log".into()).unwrap();
        handle.discard_file("f.txt".into()).unwrap();
        write_file(dir.path(), "u.tmp", "u\n");
        write_file(dir.path(), "late.tmp", "late\n");
        handle.discard_all_unstaged(vec!["u.tmp".into()]).unwrap();
        assert!(!dir.path().join("u.tmp").exists());
        assert!(dir.path().join("late.tmp").exists(), "only listed paths");
        std::fs::remove_file(dir.path().join("late.tmp")).unwrap();
        let status = handle.get_status().unwrap();
        assert!(
            status
                .iter()
                .all(|e| e.path == ".gitignore" || e.path == "junk.log"),
            "{status:?}"
        );

        assert!(handle
            .stage_lines("f.txt".into(), 99, 99, 3, vec![0], vec![])
            .is_err());
        assert!(
            handle.get_status().is_ok(),
            "worker survives a failed command"
        );
    }

    #[test]
    fn list_branches_reflects_the_initial_branch_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let branches = worker.handle().list_branches().unwrap();

        assert_eq!(branches.len(), 1);
        assert!(branches[0].is_current);
    }

    #[test]
    fn create_then_remove_worktree_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        let linked = dir.path().join("feature-tree");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert_eq!(handle.list_worktrees().unwrap().len(), 1);
        commit_all(&repo, "initial commit");
        handle
            .create_worktree(
                "feature-tree".into(),
                linked.clone(),
                "feature".into(),
                Some("HEAD".into()),
            )
            .unwrap();

        assert!(handle
            .list_worktrees()
            .unwrap()
            .iter()
            .any(|worktree| worktree.name == "feature-tree"));

        handle.remove_worktree("feature-tree".into()).unwrap();

        assert!(!linked.exists());
        assert_eq!(handle.list_worktrees().unwrap().len(), 1);
    }

    #[test]
    fn initializes_and_updates_a_submodule_through_the_worker() {
        let dir = TempDir::new().expect("create parent directory");
        let child_path = dir.path().join("child-source");
        let child = Repository::init(&child_path).expect("init child repository");
        {
            let mut config = child.config().expect("child config");
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        write_file(&child_path, "child.txt", "initial child commit");
        commit_all(&child, "child commit");

        let parent_path = dir.path().join("parent-source");
        let parent = Repository::init(&parent_path).expect("init parent repository");
        {
            let mut config = parent.config().expect("parent config");
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        let mut submodule = parent
            .submodule(
                child_path.to_str().expect("child path"),
                Path::new("deps/child"),
                true,
            )
            .expect("configure submodule");
        submodule.clone(None).expect("clone submodule");
        submodule.add_to_index(true).expect("stage submodule");
        submodule.add_finalize().expect("finalize submodule");
        drop(submodule);
        commit_all(&parent, "add child submodule");

        let checkout_path = dir.path().join("checkout");
        Repository::clone(parent_path.to_str().expect("parent path"), &checkout_path)
            .expect("clone parent checkout");
        let worker = Worker::spawn(checkout_path).expect("start worker");
        let handle = worker.handle();

        let before = handle.list_submodules().expect("list submodules");
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].path, "deps/child");
        assert!(!before[0].initialized);

        handle
            .init_submodule("deps/child".into())
            .expect("initialize submodule");
        handle
            .update_submodule("deps/child".into(), false)
            .expect("update submodule");

        let after = handle
            .list_submodules()
            .expect("list initialized submodule");
        assert!(after[0].initialized);
        assert!(after[0].head_id.is_some());
    }

    #[test]
    fn lists_and_restores_head_reflog_entries_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "first commit");
        write_file(dir.path(), "file.txt", "v2");
        commit_all(&repo, "second commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).expect("start worker");
        let handle = worker.handle();

        assert!(handle
            .list_reflog_refs()
            .expect("list reflog references")
            .iter()
            .any(|reference| reference == "HEAD"));
        let entries = handle.get_reflog("HEAD".into()).expect("read HEAD reflog");
        assert_eq!(entries[0].summary.as_deref(), Some("second commit"));
        let first_commit = entries[1].new_id.clone();

        handle
            .restore_reflog_entry("HEAD".into(), first_commit.clone())
            .expect("restore prior HEAD reflog entry");

        assert_eq!(
            repo.head()
                .expect("read HEAD")
                .target()
                .expect("HEAD target")
                .to_string(),
            first_commit
        );
    }

    #[test]
    fn refuses_to_remove_the_worktree_open_in_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        let linked = dir.path().join("feature-tree");
        git_core::worktree::create_worktree(&repo, "feature-tree", &linked, "feature", None)
            .unwrap();
        let worker = Worker::spawn(linked.clone()).unwrap();

        let result = worker.handle().remove_worktree("feature-tree".into());

        assert!(result.is_err());
        assert!(linked.exists());
        assert!(worker.handle().get_status().is_ok());
    }

    #[test]
    fn remove_worktree_ignores_the_synthetic_main_row_when_names_collide() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        let linked = dir.path().join("linked-called-main");
        git_core::worktree::create_worktree(&repo, "main", &linked, "feature", None).unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

        let result = worker.handle().remove_worktree("main".into());

        assert!(result.is_ok());
        assert!(!linked.exists());
    }

    #[test]
    fn create_then_switch_branch_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .create_branch("feature".into(), "HEAD".into())
            .unwrap();

        let branches = handle.list_branches().unwrap();
        let feature = branches.iter().find(|b| b.name == "feature").unwrap();
        assert!(feature.is_current);

        let initial_branch_name = branches
            .iter()
            .find(|b| b.name != "feature")
            .unwrap()
            .name
            .clone();
        handle.switch_branch(initial_branch_name.clone()).unwrap();

        let branches_after = handle.list_branches().unwrap();
        assert!(
            branches_after
                .iter()
                .find(|b| b.name == initial_branch_name)
                .unwrap()
                .is_current
        );
    }

    #[test]
    fn rename_branch_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let initial_branch_name = handle.list_branches().unwrap()[0].name.clone();

        handle
            .rename_branch(initial_branch_name, "renamed".into())
            .unwrap();

        let branches = handle.list_branches().unwrap();
        assert_eq!(branches[0].name, "renamed");
    }

    #[test]
    fn delete_branch_with_force_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .create_branch("feature".into(), "HEAD".into())
            .unwrap();
        let initial_branch_name = handle
            .list_branches()
            .unwrap()
            .into_iter()
            .find(|b| b.name != "feature")
            .unwrap()
            .name;
        handle.switch_branch(initial_branch_name).unwrap();

        handle.delete_branch("feature".into(), true).unwrap();

        assert!(!handle
            .list_branches()
            .unwrap()
            .iter()
            .any(|b| b.name == "feature"));
    }

    #[test]
    fn save_then_list_stash_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "file.txt", "v2");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .save_stash(git_core::stash::StashOptions::default())
            .unwrap();

        let stashes = handle.list_stashes().unwrap();
        assert_eq!(stashes.len(), 1);
    }

    #[test]
    fn remote_and_current_upstream_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        repo.remote("origin", "https://example.com/owner/repo.git")
            .unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("topic", &head, false).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("branch.topic.remote", "origin").unwrap();
        config
            .set_str("branch.topic.merge", "refs/heads/topic")
            .unwrap();
        // Repo-local, so a developer's global `fetch.prune` cannot leak into `prune` below.
        config.set_bool("fetch.prune", false).unwrap();
        // Makes `push_branch` independent of whether the init default branch is main or master.
        config.set_str("push.default", "upstream").unwrap();
        drop(config);

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .set_current_upstream("origin".into(), "main".into())
            .unwrap();

        assert_eq!(
            handle.list_remotes().unwrap(),
            vec![git_core::remote::RemoteInfo {
                name: "origin".into(),
                fetch_url: "https://example.com/owner/repo.git".into(),
                push_url: None,
                prune: false,
            }]
        );
        assert_eq!(
            handle.get_current_upstream().unwrap(),
            Some(git_core::remote::UpstreamInfo {
                local_branch: repo.head().unwrap().shorthand().unwrap().into(),
                remote_name: "origin".into(),
                remote_branch: "main".into(),
                ahead: None,
                behind: None,
                push_branch: Some("main".into()),
            })
        );

        let mut affected_branches: Vec<_> = handle
            .get_remote_upstreams("origin".into())
            .unwrap()
            .into_iter()
            .map(|upstream| upstream.local_branch)
            .collect();
        affected_branches.sort();
        let mut expected_branches = vec![
            repo.head().unwrap().shorthand().unwrap().to_string(),
            "topic".to_string(),
        ];
        expected_branches.sort();
        assert_eq!(affected_branches, expected_branches);

        handle.remove_remote("origin".into(), true).unwrap();
        assert!(handle.list_remotes().unwrap().is_empty());
        assert_eq!(handle.get_current_upstream().unwrap(), None);
        assert!(handle
            .get_remote_upstreams("origin".into())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn transfer_fetch_streams_ordered_owned_events() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        let operation_id = worker
            .handle()
            .fetch_remote("origin".into(), None, event_tx)
            .expect("start fetch");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(matches!(
            events.first(),
            Some(TransferEvent::Started {
                operation_id: id,
                operation: TransferOperation::Fetch,
            }) if id == &operation_id
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            TransferEvent::Progress(progress) if progress.operation_id == operation_id
        )));
        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed {
                operation_id: id,
                operation: TransferOperation::Fetch,
                error: None,
                hook_output: None,
            }) if id == &operation_id
        ));
    }

    #[test]
    fn transfer_fetch_failure_streams_only_a_sanitized_terminal_error() {
        let (local_dir, _repo) = init_repo();
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        let operation_id = worker
            .handle()
            .fetch_remote("missing-remote".into(), None, event_tx)
            .expect("start fetch");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed {
                operation_id: id,
                operation: TransferOperation::Fetch,
                error: Some(TransferErrorKind::TransferFailed),
                hook_output: None,
            }) if id == &operation_id
        ));
    }

    #[test]
    fn cancelling_a_transfer_by_operation_id_surfaces_a_cancelled_completion_event() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let handle = worker.handle();
        let (event_tx, event_rx) = mpsc::channel();
        let (reply_tx, reply_rx) = mpsc::channel();

        // The operation is marked cancelled *before* the worker dequeues it. A fetch from a
        // local bare remote finishes in microseconds, so cancelling after `fetch_remote` returned
        // would race the transfer and make this test flaky; pre-marking is well-defined precisely
        // because `cancel_transfer` writes to the shared registry rather than sending a command,
        // and the registry is what the `git2` progress callbacks poll.
        handle.cancel_transfer("fetch-cancel-test");
        handle
            .tx
            .send(Command::FetchRemote {
                remote_name: "origin".into(),
                prune: None,
                operation_id: "fetch-cancel-test".into(),
                events: event_tx,
                reply: reply_tx,
            })
            .expect("queue fetch");
        let _ = reply_rx.recv();
        let events: Vec<_> = event_rx.iter().collect();

        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed {
                operation_id: id,
                operation: TransferOperation::Fetch,
                error: Some(TransferErrorKind::Cancelled),
                hook_output: None,
            }) if id == "fetch-cancel-test"
        ));
        // The cancelled transfer never created the tracking ref it would have on success.
        assert!(Repository::open(local_dir.path())
            .expect("open local repo")
            .find_reference("refs/remotes/origin/main")
            .is_err());
        // Completion clears the registry entry, so a long-lived worker's cancel set cannot grow
        // without bound.
        assert!(!handle
            .cancelled
            .lock()
            .unwrap()
            .contains("fetch-cancel-test"));
    }

    #[test]
    fn cancel_transfer_reaches_a_worker_that_is_not_reading_its_command_channel() {
        // The whole point of the cancel registry: while a transfer is blocked inside `git2`'s
        // network I/O the worker thread is not reading `rx`, so a `Command`-based cancel would
        // queue behind the very operation it is meant to interrupt. A dead worker is the
        // strongest form of "not reading commands" a test can create — every `tx.send` fails,
        // yet the cancel must still land.
        let (dir, _repo) = init_repo();
        let worker = Worker::spawn(dir.path().to_path_buf()).expect("spawn worker");
        let handle = worker.handle();
        handle.crash_for_test();
        while handle.is_alive() {
            thread::sleep(Duration::from_millis(5));
        }

        handle.cancel_transfer("fetch-7");

        assert!(handle.cancelled.lock().unwrap().contains("fetch-7"));
    }

    #[test]
    fn lists_remote_tracking_branches_after_a_fetch() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let handle = worker.handle();
        let (event_tx, event_rx) = mpsc::channel();
        handle
            .fetch_remote("origin".into(), None, event_tx)
            .expect("start fetch");
        let _events: Vec<_> = event_rx.iter().collect();

        let branches = handle
            .list_remote_branches("origin".into())
            .expect("list remote branches");

        assert_eq!(branches.len(), 1);
    }

    #[test]
    fn non_fast_forward_branch_push_streams_a_safe_terminal_error_kind() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let local = Repository::open(local_dir.path()).expect("open local repo");
        write_file(local_dir.path(), "README.md", "unrelated local history\n");
        commit_all(&local, "unrelated local commit");
        drop(local);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        let operation_id = worker
            .handle()
            .push_current_branch("origin".into(), false, event_tx)
            .expect("start branch push");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed {
                operation_id: id,
                operation: TransferOperation::PushBranch,
                error: Some(TransferErrorKind::NonFastForward),
                hook_output: None,
            }) if id == &operation_id
        ));
    }

    fn completed(events: &[TransferEvent]) -> (TransferOperation, Option<TransferErrorKind>) {
        match events.last() {
            Some(TransferEvent::Completed {
                operation, error, ..
            }) => (*operation, *error),
            other => panic!("expected a Completed event last, got {other:?}"),
        }
    }

    #[test]
    fn fetch_all_remotes_reports_each_unreachable_remote_and_completes() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let missing = TempDir::new().expect("missing dir");
        Repository::open(local_dir.path())
            .expect("open local")
            .remote(
                "broken",
                missing.path().join("nope").to_str().expect("path"),
            )
            .expect("add broken remote");
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        let operation_id = worker
            .handle()
            .fetch_all_remotes(None, event_tx)
            .expect("start fetch all");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(operation_id.starts_with("fetch-all-"));
        assert!(matches!(
            events.first(),
            Some(TransferEvent::Started {
                operation: TransferOperation::FetchAll,
                ..
            })
        ));
        let failed: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                TransferEvent::RemoteFailed {
                    operation_id: id,
                    remote_name,
                    error,
                } => {
                    assert_eq!(id, &operation_id);
                    Some((remote_name.clone(), *error))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            failed,
            vec![("broken".to_string(), TransferErrorKind::TransferFailed)]
        );
        assert_eq!(completed(&events), (TransferOperation::FetchAll, None));
        let local = Repository::open(local_dir.path()).expect("reopen local");
        assert!(
            local
                .branches(Some(git2::BranchType::Remote))
                .expect("branches")
                .count()
                > 0
        );
    }

    #[test]
    fn fetch_all_remotes_fails_when_every_remote_fails() {
        let (local_dir, repo) = init_repo();
        let missing = TempDir::new().expect("missing dir");
        repo.remote(
            "broken",
            missing.path().join("nope").to_str().expect("path"),
        )
        .expect("add broken remote");
        drop(repo);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        worker
            .handle()
            .fetch_all_remotes(Some(true), event_tx)
            .expect("start fetch all");
        let events: Vec<_> = event_rx.iter().collect();

        assert_eq!(
            completed(&events),
            (
                TransferOperation::FetchAll,
                Some(TransferErrorKind::TransferFailed)
            )
        );
    }

    #[test]
    fn fetch_all_remotes_without_remotes_completes() {
        let (local_dir, _repo) = init_repo();
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        worker
            .handle()
            .fetch_all_remotes(None, event_tx)
            .expect("start fetch all");
        let events: Vec<_> = event_rx.iter().collect();

        assert_eq!(completed(&events), (TransferOperation::FetchAll, None));
    }

    #[test]
    fn push_branch_publishes_with_upstream_and_delete_remote_branch_removes_it() {
        let (_source_dir, remote_dir, local_dir) = local_and_bare_remote();
        let local = Repository::open(local_dir.path()).expect("open local");
        write_file(local_dir.path(), "local.txt", "local\n");
        commit_all(&local, "local root");
        let head = local
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("commit");
        local.branch("topic", &head, false).expect("create topic");
        drop(head);
        drop(local);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let handle = worker.handle();
        let bare_has = |name: &str| {
            Repository::open_bare(remote_dir.path())
                .expect("open bare")
                .find_reference(&format!("refs/heads/{name}"))
                .is_ok()
        };
        assert!(bare_has("main") || bare_has("master"));

        let (event_tx, event_rx) = mpsc::channel();
        let push_id = handle
            .push_branch(
                "origin".into(),
                "topic".into(),
                "topic".into(),
                true,
                false,
                event_tx,
            )
            .expect("start push");
        let events: Vec<_> = event_rx.iter().collect();
        assert!(push_id.starts_with("push-"));
        assert_eq!(completed(&events), (TransferOperation::PushBranch, None));
        assert!(bare_has("topic"));
        let topic = handle
            .list_branches()
            .expect("list branches")
            .into_iter()
            .find(|branch| branch.name == "topic")
            .expect("topic listed");
        assert_eq!(topic.upstream.as_deref(), Some("origin/topic"));

        let (event_tx, event_rx) = mpsc::channel();
        let delete_id = handle
            .delete_remote_branch("origin".into(), "topic".into(), false, event_tx)
            .expect("start delete");
        let events: Vec<_> = event_rx.iter().collect();
        assert!(delete_id.starts_with("delete-remote-"));
        assert_eq!(
            completed(&events),
            (TransferOperation::DeleteRemoteBranch, None)
        );
        assert!(!bare_has("topic"));
        // Only that branch: the remote's default branch is untouched.
        assert!(bare_has("main") || bare_has("master"));
        assert!(handle.get_undo_state().expect("undo state").undo.is_none());
    }

    #[test]
    fn push_and_force_push_refuse_up_front_when_push_default_needs_a_choice() {
        let (_source_dir, remote_dir, local_dir) = local_and_bare_remote();
        let local = Repository::open(local_dir.path()).expect("open local");
        write_file(local_dir.path(), "local.txt", "local\n");
        commit_all(&local, "local root");
        let mut config = local.config().expect("config");
        config
            .set_str("push.default", "simple")
            .expect("push.default");
        config
            .set_str("branch.feat.remote", "origin")
            .expect("remote");
        config
            .set_str("branch.feat.merge", "refs/heads/main")
            .expect("merge");
        drop(config);
        let head = local
            .head()
            .expect("head")
            .peel_to_commit()
            .expect("commit");
        local.branch("feat", &head, false).expect("create feat");
        local.set_head("refs/heads/feat").expect("checkout feat");
        drop(head);
        drop(local);
        let bare_main = || {
            Repository::open_bare(remote_dir.path())
                .expect("open bare")
                .find_reference("refs/heads/main")
                .ok()
                .and_then(|reference| reference.target())
        };
        let before = bare_main();
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let handle = worker.handle();

        let (event_tx, event_rx) = mpsc::channel();
        let pushed = handle.push_current_branch("origin".into(), false, event_tx);
        assert!(event_rx.iter().next().is_none());
        let (event_tx, event_rx) = mpsc::channel();
        let forced = handle.force_push_current_branch("origin".into(), false, event_tx);
        assert!(event_rx.iter().next().is_none());

        for result in [pushed, forced] {
            let message = result.expect_err("must not start a transfer");
            assert!(message.contains("Push to…"), "{message}");
        }
        assert_eq!(bare_main(), before);
        let upstream = handle
            .get_current_upstream()
            .expect("upstream")
            .expect("set");
        assert_eq!(upstream.push_branch, None);
    }

    #[test]
    fn tags_and_branch_push_round_trip_through_the_worker() {
        let (local_dir, repo) = init_repo();
        write_file(local_dir.path(), "README.md", "initial commit\n");
        commit_all(&repo, "initial commit");

        let remote_dir = TempDir::new().expect("create bare remote directory");
        Repository::init_bare(remote_dir.path()).expect("init bare remote");
        repo.remote("origin", remote_dir.path().to_str().expect("remote path"))
            .expect("add origin");
        drop(repo);

        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let handle = worker.handle();

        handle
            .create_tag("v1.0.0".into(), Some("first release".into()))
            .expect("create tag");
        let tags = handle.list_tags().expect("list tags");
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, "v1.0.0");
        assert_eq!(
            tags[0].target_id,
            handle.get_commit_graph(1, None).unwrap()[0].id
        );
        assert_eq!(
            tags[0].commit_id.as_deref(),
            Some(handle.get_commit_graph(1, None).unwrap()[0].id.as_str())
        );
        assert!(tags[0].annotated);
        assert_eq!(tags[0].message.as_deref(), Some("first release"));
        assert_eq!(tags[0].tagger_name.as_deref(), Some("Test User"));
        assert!(tags[0].timestamp.is_some());

        let (event_tx, event_rx) = mpsc::channel();
        let operation_id = handle
            .push_current_branch("origin".into(), false, event_tx)
            .expect("start branch push");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(matches!(
            events.first(),
            Some(TransferEvent::Started {
                operation_id: id,
                operation: TransferOperation::PushBranch,
            }) if id == &operation_id
        ));
        assert!(events.iter().all(|event| match event {
            TransferEvent::Started {
                operation_id: id, ..
            }
            | TransferEvent::RemoteFailed {
                operation_id: id, ..
            }
            | TransferEvent::Completed {
                operation_id: id, ..
            } => id == &operation_id,
            TransferEvent::Progress(progress) => progress.operation_id == operation_id,
        }));
        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed { error: None, .. })
        ));
        assert!(Repository::open_bare(remote_dir.path())
            .expect("open bare remote")
            .head()
            .is_ok());
    }

    #[test]
    fn pull_fetch_failure_returns_only_a_sanitized_error() {
        let (local_dir, repo) = init_repo();
        write_file(local_dir.path(), "README.md", "initial commit\n");
        commit_all(&repo, "initial commit");
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        let secret_path = local_dir.path().join("alice-secret").join("missing.git");
        repo.remote("origin", secret_path.to_str().unwrap())
            .unwrap();
        let mut config = repo.config().unwrap();
        config
            .set_str(&format!("branch.{branch}.remote"), "origin")
            .unwrap();
        config
            .set_str(&format!("branch.{branch}.merge"), "refs/heads/main")
            .unwrap();
        drop(config);
        drop(repo);

        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, _event_rx) = mpsc::channel();

        let error = worker
            .handle()
            .pull_current_upstream(event_tx)
            .expect_err("pull should fail while fetching");

        assert_eq!(error, "pull failed");
        assert!(!error.contains("alice-secret"));
    }

    #[test]
    fn pull_with_untracked_file_reports_dirty_worktree() {
        let (local_dir, repo) = init_repo();
        write_file(local_dir.path(), "README.md", "initial commit\n");
        commit_all(&repo, "initial commit");
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.remote("origin", local_dir.path().to_str().unwrap())
            .unwrap();
        let mut config = repo.config().unwrap();
        config
            .set_str(&format!("branch.{branch}.remote"), "origin")
            .unwrap();
        config
            .set_str(
                &format!("branch.{branch}.merge"),
                &format!("refs/heads/{branch}"),
            )
            .unwrap();
        drop(config);
        drop(repo);
        write_file(local_dir.path(), "untracked.txt", "x");

        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, _event_rx) = mpsc::channel();

        let error = worker
            .handle()
            .pull_current_upstream(event_tx)
            .expect_err("pull should refuse a dirty worktree");

        assert_eq!(error, "cannot pull with a dirty worktree");
    }

    #[test]
    fn apply_then_drop_stash_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "file.txt", "v2");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .save_stash(git_core::stash::StashOptions::default())
            .unwrap();
        let id = handle.list_stashes().unwrap()[0].commit_id.clone();

        assert_eq!(
            handle.apply_stash(0, id.clone(), false).unwrap(),
            git_core::stash::StashApplyOutcome::Applied
        );
        let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
        assert_eq!(contents, "v2");

        handle.drop_stash(0, id).unwrap();
        assert!(handle.list_stashes().unwrap().is_empty());
    }

    #[test]
    fn stash_commands_refuse_a_stale_stash_id_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "file.txt", "v2");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .save_stash(git_core::stash::StashOptions::default())
            .unwrap();
        let stale = handle.list_stashes().unwrap()[0].commit_id.clone();
        write_file(dir.path(), "file.txt", "v3");
        handle
            .save_stash(git_core::stash::StashOptions::default())
            .unwrap();

        let expected = "The stash list changed; refresh and try again.".to_string();
        assert_eq!(
            handle.apply_stash(0, stale.clone(), true),
            Err(expected.clone())
        );
        assert_eq!(
            handle.pop_stash(0, stale.clone(), true),
            Err(expected.clone())
        );
        assert_eq!(handle.drop_stash(0, stale), Err(expected));
        assert_eq!(handle.list_stashes().unwrap().len(), 2);
    }

    fn diverged_feature_repo() -> (tempfile::TempDir, git2::Repository) {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "v1\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();
        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "feature.txt", "new\n");
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "main.txt", "new\n");
        commit_all(&repo, "main commit");
        (dir, repo)
    }

    #[test]
    fn squash_merge_round_trips_and_is_undoable_and_labelled() {
        let (dir, _repo) = diverged_feature_repo();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let outcome = handle
            .start_merge(
                "feature".into(),
                git_core::merge::MergeOptions {
                    fast_forward: git_core::merge::FastForward::Allow,
                    squash: true,
                },
            )
            .unwrap();

        assert_eq!(outcome, git_core::merge::MergeOutcome::Squashed);
        assert!(handle.get_head_info().unwrap().squash_message.is_some());
        assert_eq!(
            handle.get_undo_state().unwrap().undo.as_deref(),
            Some("Squash merge feature")
        );
        handle.undo().unwrap();
        assert!(handle.get_status().unwrap().is_empty());
        assert!(!dir.path().join("feature.txt").exists());
        assert!(handle.get_head_info().unwrap().squash_message.is_none());
    }

    #[test]
    fn no_ff_merge_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "v1\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();
        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "feature.txt", "new\n");
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let outcome = handle
            .start_merge(
                "feature".into(),
                git_core::merge::MergeOptions {
                    fast_forward: git_core::merge::FastForward::Never,
                    squash: false,
                },
            )
            .unwrap();

        assert_eq!(outcome, git_core::merge::MergeOutcome::Merged);
        assert!(handle.get_merge_message().unwrap().is_some());
    }

    #[test]
    fn stash_options_pop_and_files_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "v1");
        write_file(dir.path(), "b.txt", "v1");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "a.txt", "mine");
        write_file(dir.path(), "b.txt", "other");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        handle
            .save_stash(git_core::stash::StashOptions {
                message: Some("only a".into()),
                paths: vec!["a.txt".into()],
                ..Default::default()
            })
            .unwrap();

        let stashes = handle.list_stashes().unwrap();
        assert!(stashes[0].message.ends_with(": only a"));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
            "other"
        );
        let files = handle
            .get_stash_files(stashes[0].commit_id.clone())
            .unwrap();
        assert_eq!(files.tracked, vec!["a.txt".to_string()]);
        assert_eq!(files.untracked_commit_id, None);
        assert_eq!(
            handle
                .pop_stash(0, stashes[0].commit_id.clone(), true)
                .unwrap(),
            git_core::stash::StashApplyOutcome::Applied
        );
        assert!(handle.list_stashes().unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "mine"
        );
    }

    #[test]
    fn merge_defaults_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        // Set in the repo's own config so a global `merge.ff` cannot leak in (issue #111).
        repo.config().unwrap().set_str("merge.ff", "only").unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

        let defaults = worker.handle().merge_defaults().unwrap();

        assert_eq!(defaults.fast_forward, git_core::merge::FastForward::Only);
        assert!(!defaults.squash);
    }

    #[test]
    fn get_blame_reflects_a_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "hello\n");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let lines = worker
            .handle()
            .get_blame("HEAD".into(), "file.txt".into())
            .unwrap();

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].content, "hello");
    }

    #[test]
    fn get_file_history_and_rename_diff_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "l1\nl2\nl3\nl4\nl5\n");
        commit_all(&repo, "add file");
        write_file(dir.path(), "other.txt", "x\n");
        commit_all(&repo, "unrelated");
        write_file(dir.path(), "file.txt", "l1\nl2\nl3\nl4\nCHANGED\n");
        commit_all(&repo, "edit file");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let history = handle.get_file_history("file.txt".into(), 10).unwrap();

        let summaries: Vec<&str> = history.iter().map(|e| e.commit.summary.as_str()).collect();
        assert_eq!(summaries, vec!["edit file", "add file"]);

        // No rename in this repo: diffing the same path as both sides behaves like commit_diff.
        let hunks = handle
            .get_commit_rename_diff(
                history[0].commit.id.clone(),
                "file.txt".into(),
                "file.txt".into(),
                git_core::diff::DiffOptions::default(),
            )
            .unwrap();
        assert_eq!(hunks.len(), 1);
    }

    #[test]
    fn range_and_working_tree_compares_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "first");
        let first = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        write_file(dir.path(), "a.txt", "two\n");
        commit_all(&repo, "second");
        let second = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        write_file(dir.path(), "a.txt", "three\n");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let a = git_core::diff::ChangedFile {
            path: "a.txt".into(),
            old_path: None,
        };
        let opts = git_core::diff::DiffOptions::default();

        assert_eq!(
            handle
                .get_range_files(first.clone(), second.clone())
                .unwrap(),
            vec![a.clone()]
        );
        let hunks = handle
            .get_range_diff(first.clone(), second, "a.txt".into(), None, opts)
            .unwrap();
        assert!(hunks[0].lines.iter().any(|l| l.content == "two\n"));

        assert_eq!(
            handle
                .get_working_tree_compare_files(first.clone())
                .unwrap(),
            vec![a]
        );
        let hunks = handle
            .get_working_tree_compare_diff(first, "a.txt".into(), None, opts)
            .unwrap();
        assert!(hunks[0].lines.iter().any(|l| l.content == "three\n"));
    }

    #[test]
    fn branch_compare_range_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "base");
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        // A name no `init.defaultBranch` would pick, so HEAD is never this branch.
        repo.branch("compare-worker-target", &base, false).unwrap();
        write_file(dir.path(), "a.txt", "two\n");
        commit_all(&repo, "main only");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let range = handle
            .get_branch_compare_range("refs/heads/compare-worker-target".into())
            .unwrap();
        assert_eq!(range.from_id, base.id().to_string());
        assert_eq!(range.to_id, base.id().to_string());
        assert_eq!(
            handle
                .get_branch_compare_range("refs/heads/nope".into())
                .unwrap_err(),
            "No branch named \"nope\"."
        );
    }

    #[test]
    fn start_merge_and_get_merge_message_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "shared.txt", "line one\nline two\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();

        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "shared.txt", "line one\nmain two\n");
        commit_all(&repo, "main commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let outcome = handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();

        assert!(matches!(
            outcome,
            git_core::merge::MergeOutcome::Conflicted { .. }
        ));
        assert!(handle
            .get_merge_message()
            .unwrap()
            .unwrap()
            .contains("feature"));
    }

    #[test]
    fn get_conflict_hunks_then_resolve_conflict_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "shared.txt", "line one\nline two\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();

        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "shared.txt", "line one\nmain two\n");
        commit_all(&repo, "main commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();

        let segments = handle.get_conflict_hunks("shared.txt".into()).unwrap();
        assert!(!segments.is_empty());

        handle
            .resolve_conflict("shared.txt".into(), "line one\nresolved\n".into())
            .unwrap();

        // Resolving and staging a conflict does not empty the status list — the file now
        // differs from HEAD, so it shows as one staged entry, exactly like any other staged
        // change. It only leaves `status()` once actually committed.
        let status = handle.get_status().unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].path, "shared.txt");
        assert!(status[0].staged);
        assert_eq!(status[0].kind, git_core::status::StatusKind::Modified);
    }

    #[test]
    fn abort_merge_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "shared.txt", "line one\nline two\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();

        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "shared.txt", "line one\nmain two\n");
        commit_all(&repo, "main commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();

        handle.abort_merge().unwrap();

        assert!(handle.get_merge_message().unwrap().is_none());
        assert!(handle.get_status().unwrap().is_empty());
    }

    #[test]
    fn resolve_add_delete_conflict_round_trips_through_the_worker() {
        let (dir, mut repo) = init_repo();
        write_file(dir.path(), "shared.txt", "v1\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();

        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "shared.txt", "v2\n");
        commit_all(&repo, "feature commit modifies");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
        git_core::stage::stage_file(&repo, "shared.txt").unwrap();
        git_core::commit::commit(
            &mut repo,
            "main commit deletes",
            git_core::hooks::HookOptions::default(),
        )
        .unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();

        handle
            .resolve_add_delete_conflict(
                "shared.txt".into(),
                git_core::merge::FileConflictChoice::Theirs,
            )
            .unwrap();

        let status = handle.get_status().unwrap();
        assert_eq!(status.len(), 1);
        assert!(status[0].staged);
    }

    #[test]
    fn commits_since_and_start_rebase_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "v1\n");
        commit_all(&repo, "base commit");
        let onto = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "add a");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let commits = handle.commits_since(onto.clone()).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].summary, "add a");

        let plan = vec![git_core::rebase::RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: git_core::rebase::RebaseAction::Pick,
            combined_message: None,
        }];
        let result = handle.start_rebase(onto, plan).unwrap();

        assert_eq!(result, git_core::rebase::RebaseStepResult::Done);
        assert_eq!(rebase_progress(&handle), None);
    }

    #[test]
    fn rebase_continue_and_abort_rebase_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "v1\n");
        commit_all(&repo, "base commit");
        let onto = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "add a");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let commits = handle.commits_since(onto.clone()).unwrap();
        let plan = vec![git_core::rebase::RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: git_core::rebase::RebaseAction::Edit,
            combined_message: None,
        }];
        let result = handle.start_rebase(onto, plan).unwrap();
        assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
        // 1-indexed: the first (and here only) paused step reports "Step 1 of 1", not "0 of 1".
        assert_eq!(rebase_progress(&handle), Some((1, 1)));

        handle.abort_rebase().unwrap();

        assert_eq!(rebase_progress(&handle), None);
        assert!(handle.get_status().unwrap().is_empty());
    }

    #[test]
    fn detecting_an_ambiguous_remote_declines_rather_than_erroring_or_requesting() {
        let (dir, repo) = init_repo();
        // Three path segments (owner/name/extra) is ambiguous per git-core's forge parsing.
        repo.remote("origin", "https://github.com/acme/widget/extra.git")
            .unwrap();

        // An empty response queue: any attempted HTTP request panics the test.
        let worker = Worker::spawn_with(
            dir.path().to_path_buf(),
            FakeCredentialStore::default(),
            FakeForgeApi::queue(vec![]),
        )
        .unwrap();
        let handle = worker.handle();

        assert_eq!(handle.detect_forge_repository().unwrap(), vec![]);

        let error = handle
            .list_pull_requests("origin".to_string(), "rene".to_string())
            .expect_err("an ambiguous remote must not be treated as a supported repository");
        assert_eq!(error, "this remote is not a supported forge repository");
    }

    #[test]
    fn saved_forge_token_is_looked_up_inside_rust_and_never_exposed_in_the_result() {
        let (dir, repo) = init_repo();
        repo.remote("origin", "https://github.com/acme/widget.git")
            .unwrap();
        let token = "gh-token-should-never-leak";

        let api = FakeForgeApi::queue(vec![ok(200, GITHUB_LIST_FIXTURE)]);
        let worker = Worker::spawn_with(
            dir.path().to_path_buf(),
            FakeCredentialStore::default(),
            api.clone(),
        )
        .unwrap();
        let handle = worker.handle();

        assert_eq!(
            handle
                .detect_forge_repository()
                .unwrap()
                .into_iter()
                .map(|repository| repository.remote_name)
                .collect::<Vec<_>>(),
            vec!["origin".to_string()]
        );

        // Listing before any token is saved must be rejected without ever calling the
        // transport — `Worker::list_pull_requests`'s signature has no token parameter at all,
        // so the only way this can succeed is a lookup happening inside the worker.
        let before = handle
            .list_pull_requests("origin".to_string(), "rene".to_string())
            .expect_err("a missing token must be rejected before any request");
        assert_eq!(before, "a provider token is required for this action");
        assert!(api.requests().is_empty());

        handle
            .save_forge_token(ForgeProvider::GitHub, "rene".to_string(), token.to_string())
            .unwrap();

        let pull_requests = handle
            .list_pull_requests("origin".to_string(), "rene".to_string())
            .unwrap();

        assert_eq!(pull_requests.pull_requests.len(), 1);
        assert_eq!(pull_requests.pull_requests[0].number, 7);
        assert_eq!(
            pull_requests.pull_requests[0].title,
            "Add pull request support"
        );
        assert!(!pull_requests.truncated);
        // The lookup happened inside the worker: the request the fake transport actually
        // received carries the saved token in its Authorization header.
        let requests = api.requests();
        assert_eq!(requests.len(), 1);
        assert!(requests[0]
            .headers
            .contains(&("Authorization".to_string(), format!("Bearer {token}"))));
        // Round-tripped result never carries the token.
        assert!(!format!("{pull_requests:?}").contains(token));

        // Forgetting the token makes subsequent listing fail again the same way.
        handle
            .forget_forge_token(ForgeProvider::GitHub, "rene".to_string())
            .unwrap();
        let after_forget = handle
            .list_pull_requests("origin".to_string(), "rene".to_string())
            .expect_err("a forgotten token must not still be usable");
        assert_eq!(after_forget, "a provider token is required for this action");
    }

    #[test]
    fn create_pull_request_round_trips_through_the_worker_without_exposing_the_token() {
        let (dir, repo) = init_repo();
        repo.remote("origin", "https://github.com/acme/widget.git")
            .unwrap();
        let token = "gh-token-should-never-leak";
        const GITHUB_CREATE_FIXTURE: &str = r#"{
            "id": 202,
            "number": 8,
            "title": "Add feature",
            "html_url": "https://github.com/acme/widget/pull/8",
            "user": {"login": "rene"},
            "head": {"ref": "feature/pr"},
            "base": {"ref": "main"},
            "state": "open"
        }"#;

        let worker = Worker::spawn_with(
            dir.path().to_path_buf(),
            FakeCredentialStore::default(),
            FakeForgeApi::queue(vec![ok(201, GITHUB_CREATE_FIXTURE)]),
        )
        .unwrap();
        let handle = worker.handle();
        handle
            .save_forge_token(ForgeProvider::GitHub, "rene".to_string(), token.to_string())
            .unwrap();

        let created = handle
            .create_pull_request(
                "origin".to_string(),
                "rene".to_string(),
                crate::pull_requests::CreatePullRequest {
                    title: "Add feature".to_string(),
                    description: Some("Implements the thing".to_string()),
                    source_branch: "feature/pr".to_string(),
                    target_branch: "main".to_string(),
                },
            )
            .unwrap();

        assert_eq!(created.number, 8);
        assert!(!format!("{created:?}").contains(token));
    }

    #[test]
    fn ensure_worker_recovers_a_repo_after_its_worker_thread_panics() {
        let (dir, _repo) = init_repo();
        let path = dir.path().to_string_lossy().into_owned();

        let workers: Mutex<HashMap<String, Worker>> = Mutex::new(HashMap::new());
        let handle = ensure_worker(&workers, &path).expect("spawn the first worker");
        assert!(handle.get_status().is_ok());

        // Crash the worker thread from inside its dispatch loop — the same failure mode a
        // panicking git-core call inside a `Command` match arm would trigger.
        handle.crash_for_test();

        // The channel-disconnect signal std::sync::mpsc gives us is immediate, so this call
        // shouldn't hang — bound it with a timeout anyway so a regression here fails the test
        // instead of wedging it.
        let (done_tx, done_rx) = mpsc::channel();
        let crashed_handle = handle.clone();
        thread::spawn(move || {
            let _ = done_tx.send(crashed_handle.get_status());
        });
        let result = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("worker did not report an error after crashing — call hung");
        let error = result.expect_err("a crashed worker must return an error, not a status list");
        assert!(
            error.contains("worker thread stopped"),
            "unexpected error message: {error}"
        );

        // The map still holds the stale (dead) entry at this point. `ensure_worker` — the same
        // helper `open_repo` uses in both tauri-app and vscode-sidecar — must detect it's dead
        // and respawn rather than handing back the same broken handle
        // (AUD-2026-09-05-CONC-001), which is what `open_repo`'s old `contains_key` fast path
        // did.
        assert!(!handle.is_alive());
        let fresh_handle =
            ensure_worker(&workers, &path).expect("respawn a fresh worker for the same path");
        assert!(fresh_handle.get_status().is_ok());
    }

    #[test]
    fn history_commands_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "base\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();
        let base = repo.head().unwrap().target().unwrap().to_string();
        git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
        git_core::branch::switch_branch(&repo, "side").unwrap();
        write_file(dir.path(), "side.txt", "side\n");
        commit_all(&repo, "side commit");
        let side = repo.head().unwrap().target().unwrap().to_string();
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        assert_eq!(handle.get_head_info().unwrap().head_id, Some(base.clone()));

        let outcome = handle.cherry_pick(side.clone(), None).unwrap();
        assert!(matches!(
            outcome,
            git_core::pick::PickOutcome::Committed { .. }
        ));

        handle
            .amend_commit(
                "side, amended".into(),
                false,
                None,
                Vec::new(),
                mpsc::channel().0,
            )
            .unwrap();
        let picked = handle.get_head_info().unwrap().head_id.unwrap();

        let reverted = handle.revert_commit(picked, None).unwrap();
        assert!(matches!(
            reverted,
            git_core::pick::PickOutcome::Committed { .. }
        ));

        handle
            .reset_to_commit(base.clone(), git_core::reset::ResetMode::Hard)
            .unwrap();
        assert_eq!(handle.get_head_info().unwrap().head_id, Some(base.clone()));

        handle.checkout_commit(side).unwrap();
        assert!(handle.get_head_info().unwrap().detached_at.is_some());

        assert!(handle.cherry_pick("not-a-sha".into(), None).is_err());
        assert!(
            handle.get_head_info().is_ok(),
            "worker survives a failed command"
        );
    }

    #[test]
    fn force_push_streams_a_completed_force_push_event() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let local = Repository::open(local_dir.path()).expect("open local repo");
        // Record the remote tip as the lease's expected value.
        local
            .find_remote("origin")
            .expect("origin")
            .fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)
            .expect("fetch remote tip");
        write_file(local_dir.path(), "README.md", "rewritten local history\n");
        commit_all(&local, "local commit");
        drop(local);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        let operation_id = worker
            .handle()
            .force_push_current_branch("origin".into(), false, event_tx)
            .expect("start force push");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(operation_id.starts_with("force-push-"));
        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed {
                operation_id: id,
                operation: TransferOperation::ForcePushBranch,
                error: None,
                hook_output: None,
            }) if id == &operation_id
        ));
    }

    #[test]
    fn undo_and_redo_a_discard_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        write_file(dir.path(), "a.txt", "precious\n");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        handle.discard_file("a.txt".into()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one\n"
        );
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState {
                undo: Some("Discard changes to a.txt".into()),
                redo: None
            }
        );

        assert_eq!(
            handle.undo().unwrap(),
            UndoOutcome::Done {
                label: "Discard changes to a.txt".into()
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "precious\n"
        );
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState {
                undo: None,
                redo: Some("Discard changes to a.txt".into())
            }
        );

        assert_eq!(
            handle.redo().unwrap(),
            UndoOutcome::Done {
                label: "Discard changes to a.txt".into()
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one\n"
        );
    }

    #[test]
    fn failed_and_no_op_commands_record_nothing() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert!(handle.delete_branch("missing".into(), false).is_err());
        handle.stage_file("a.txt".into()).unwrap(); // already clean: no change
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState {
                undo: None,
                redo: None
            }
        );
        assert_eq!(handle.undo().unwrap(), UndoOutcome::Nothing);
    }

    #[test]
    fn a_new_command_clears_redo() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        write_file(dir.path(), "a.txt", "two\n");
        write_file(dir.path(), "b.txt", "b\n");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();
        handle.undo().unwrap();
        handle.stage_file("b.txt".into()).unwrap();
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState {
                undo: Some("Stage b.txt".into()),
                redo: None
            }
        );
    }

    #[test]
    fn external_change_refuses_undo_and_keeps_the_entry() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        write_file(dir.path(), "a.txt", "precious\n");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.discard_file("a.txt".into()).unwrap();
        write_file(dir.path(), "a.txt", "typed later\n");
        assert_eq!(
            handle.undo().unwrap(),
            UndoOutcome::RepositoryChanged {
                label: "Discard changes to a.txt".into()
            }
        );
        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Discard changes to a.txt".into())
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "typed later\n"
        );
    }

    /// `base` → `feature` and `main` both rewrite `a.txt`, so merging `feature` conflicts.
    fn conflicting_branches() -> (TempDir, Repository) {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "base\n");
        commit_all(&repo, "base");
        let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();
        {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("feature", &head, false).unwrap();
        }
        repo.set_head("refs/heads/feature").unwrap();
        write_file(dir.path(), "a.txt", "feature\n");
        commit_all(&repo, "feature change");
        repo.set_head(&format!("refs/heads/{base_branch}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        write_file(dir.path(), "a.txt", "main\n");
        commit_all(&repo, "main change");
        (dir, repo)
    }

    #[test]
    fn stage_during_a_conflicted_merge_is_not_recorded_and_abort_drops_pending() {
        let (dir, _repo) = conflicting_branches();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let outcome = handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();
        assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
        // A recordable command while the merge is paused must not become its own entry.
        write_file(dir.path(), "b.txt", "unrelated\n");
        handle.stage_file("b.txt".into()).unwrap();
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState {
                undo: None,
                redo: None
            }
        );
        assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
        assert_eq!(handle.redo().unwrap(), UndoOutcome::OperationInProgress);
        // An untracked file written mid-merge survives the abort, so the repo afterwards differs
        // from before the merge: only an explicit drop — not a "nothing changed" check — keeps
        // the aborted merge off the stack.
        write_file(dir.path(), "notes.txt", "kept\n");
        handle.abort_merge().unwrap();
        assert!(dir.path().join("notes.txt").exists());
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState {
                undo: None,
                redo: None
            }
        );
        assert_eq!(handle.undo().unwrap(), UndoOutcome::Nothing);
        // And the recorder is idle again: the next command records normally.
        handle.create_branch("after".into(), "HEAD".into()).unwrap();
        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Create branch after".into())
        );
    }

    #[test]
    fn a_conflicted_merge_finished_by_commit_is_one_entry() {
        let (dir, repo) = conflicting_branches();
        let main_tip = repo.head().unwrap().target().unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();
        handle
            .resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        handle
            .commit("Merge feature".into(), false, Vec::new(), mpsc::channel().0)
            .unwrap();
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState {
                undo: Some("Merge feature".into()),
                redo: None
            }
        );
        assert_eq!(
            handle.undo().unwrap(),
            UndoOutcome::Done {
                label: "Merge feature".into()
            }
        );
        let reopened = git2::Repository::open(dir.path()).unwrap();
        assert_eq!(reopened.head().unwrap().target().unwrap(), main_tip);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "main\n"
        );
    }

    #[test]
    fn history_survives_a_worker_restart_and_is_capped() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        {
            let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
            let handle = worker.handle();
            for n in 0..55 {
                handle
                    .create_branch(format!("b{n}"), "HEAD".into())
                    .unwrap();
            }
        }
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Create branch b54".into())
        );
        let mut undone = 0;
        while let UndoOutcome::Done { .. } = handle.undo().unwrap() {
            undone += 1;
        }
        assert_eq!(undone, 50);
    }

    // --- Restart resilience (#108, #134) ----------------------------------------------------

    /// An app restart: drop the worker and every handle (held replies mean everything was
    /// persisted before the last reply), then start a fresh worker on the same path.
    fn restart(
        worker: Worker,
        handle: super::WorkerHandle,
        path: &Path,
    ) -> (Worker, super::WorkerHandle) {
        drop(handle);
        drop(worker);
        let worker = Worker::spawn(path.to_path_buf()).unwrap();
        let handle = worker.handle();
        (worker, handle)
    }

    /// "Step N of M" of the stored rebase, as `HeadInfo.rebase` reports it.
    fn rebase_progress(handle: &super::WorkerHandle) -> Option<(usize, usize)> {
        handle
            .get_head_info()
            .unwrap()
            .rebase
            .map(|rebase| (rebase.current_step, rebase.total_steps))
    }

    fn rebase_file(repo: &Repository) -> std::path::PathBuf {
        repo.path().join("browsitory").join("rebase.json")
    }

    /// `base` (onto; f.txt = "base\n") → "a" (a.txt) → "b" (f.txt = "from b\n") on the
    /// checked-out branch. Returns `(onto, plan [Edit a, Pick b], branch ref, branch tip)`.
    fn edit_then_pick(
        dir: &Path,
        repo: &Repository,
    ) -> (
        String,
        Vec<git_core::rebase::RebasePlanEntry>,
        String,
        git2::Oid,
    ) {
        write_file(dir, "f.txt", "base\n");
        commit_all(repo, "base");
        let onto = repo.head().unwrap().target().unwrap().to_string();
        write_file(dir, "a.txt", "a\n");
        commit_all(repo, "a");
        let a = repo.head().unwrap().target().unwrap().to_string();
        write_file(dir, "f.txt", "from b\n");
        commit_all(repo, "b");
        let head = repo.head().unwrap();
        let tip = head.target().unwrap();
        let entry = |id: String, action| git_core::rebase::RebasePlanEntry {
            commit_id: id,
            action,
            combined_message: None,
        };
        (
            onto,
            vec![
                entry(a, git_core::rebase::RebaseAction::Edit),
                entry(tip.to_string(), git_core::rebase::RebaseAction::Pick),
            ],
            head.name().unwrap().to_string(),
            tip,
        )
    }

    fn paused_for_edit(
        dir: &Path,
        repo: &Repository,
    ) -> (Worker, super::WorkerHandle, String, git2::Oid) {
        let (onto, plan, branch, tip) = edit_then_pick(dir, repo);
        let worker = Worker::spawn(dir.to_path_buf()).unwrap();
        let handle = worker.handle();
        assert_eq!(
            handle.start_rebase(onto, plan).unwrap(),
            git_core::rebase::RebaseStepResult::PausedForEdit
        );
        (worker, handle, branch, tip)
    }

    fn branch_target(path: &Path, branch: &str) -> git2::Oid {
        Repository::open(path)
            .unwrap()
            .find_reference(branch)
            .unwrap()
            .target()
            .unwrap()
    }

    /// `edit_then_pick`'s repository with `f.txt` modified on disk (#170).
    fn dirty_before_rebase(
        dir: &Path,
        repo: &Repository,
    ) -> (
        Worker,
        super::WorkerHandle,
        String,
        Vec<git_core::rebase::RebasePlanEntry>,
    ) {
        let (onto, plan, _, _) = edit_then_pick(dir, repo);
        write_file(dir, "f.txt", "DIRTY\n");
        let worker = Worker::spawn(dir.to_path_buf()).unwrap();
        let handle = worker.handle();
        (worker, handle, onto, plan)
    }

    #[test]
    fn a_refused_rebase_start_records_no_undo_entry_and_keeps_the_edit() {
        let (dir, repo) = init_repo();
        let (_worker, handle, onto, plan) = dirty_before_rebase(dir.path(), &repo);
        let before = handle.get_undo_state().unwrap();

        let error = handle.start_rebase(onto, plan).unwrap_err();
        assert_eq!(
            error,
            "You have uncommitted changes to f.txt. Commit or stash them before starting a rebase."
        );
        assert_eq!(handle.get_undo_state().unwrap(), before);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
            "DIRTY\n"
        );
    }

    #[test]
    fn a_refused_rebase_start_leaves_no_rebase_stored() {
        let (dir, repo) = init_repo();
        let (_worker, handle, onto, plan) = dirty_before_rebase(dir.path(), &repo);
        handle.start_rebase(onto, plan).unwrap_err();
        assert_eq!(rebase_progress(&handle), None);
        assert!(!rebase_file(&repo).exists());
    }

    #[test]
    fn a_paused_rebase_survives_a_worker_restart() {
        let (dir, repo) = init_repo();
        let (worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
        assert!(rebase_file(&repo).exists());

        let (_worker, handle) = restart(worker, handle, dir.path());

        assert_eq!(rebase_progress(&handle), Some((1, 2)));
    }

    #[test]
    fn a_restarted_worker_continues_the_rebase_to_completion() {
        let (dir, repo) = init_repo();
        let (worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
        let (_worker, handle) = restart(worker, handle, dir.path());

        assert_eq!(
            handle.rebase_continue().unwrap(),
            git_core::rebase::RebaseStepResult::Done
        );

        // Replayed onto its own base, b may get its old id back; check the result instead.
        let _ = tip;
        let reopened = Repository::open(dir.path()).unwrap();
        assert_eq!(reopened.head().unwrap().name().unwrap(), branch);
        let head = reopened.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.summary().unwrap(), Some("b"));
        assert_eq!(head.parent(0).unwrap().summary().unwrap(), Some("a"));
        assert_eq!(reopened.state(), git2::RepositoryState::Clean);
        assert!(!rebase_file(&repo).exists());
        assert_eq!(rebase_progress(&handle), None);
    }

    #[test]
    fn a_restarted_worker_aborts_the_rebase() {
        let (dir, repo) = init_repo();
        let (worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
        let (_worker, handle) = restart(worker, handle, dir.path());

        handle.abort_rebase().unwrap();

        assert_eq!(branch_target(dir.path(), &branch), tip);
        let reopened = Repository::open(dir.path()).unwrap();
        assert_eq!(reopened.head().unwrap().name().unwrap(), branch);
        assert!(!rebase_file(&repo).exists());
        assert_eq!(rebase_progress(&handle), None);
    }

    #[test]
    fn a_crashed_worker_keeps_the_paused_rebase() {
        let (dir, repo) = init_repo();
        let (onto, plan, _, _) = edit_then_pick(dir.path(), &repo);
        let path = dir.path().to_string_lossy().into_owned();
        let workers: Mutex<HashMap<String, Worker>> = Mutex::new(HashMap::new());
        let handle = ensure_worker(&workers, &path).unwrap();
        handle.start_rebase(onto, plan).unwrap();

        handle.crash_for_test();
        while handle.get_status().is_ok() {
            thread::sleep(Duration::from_millis(10));
        }
        let fresh = ensure_worker(&workers, &path).unwrap();

        assert_eq!(rebase_progress(&fresh), Some((1, 2)));
    }

    #[test]
    fn a_rebase_error_stop_is_persisted_with_its_cursor() {
        let (dir, repo) = init_repo();
        let (worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
        // A local edit in the way of b: a lands, b is refused.
        write_file(dir.path(), "f.txt", "local\n");
        assert!(handle.rebase_continue().is_err());
        let (_worker, handle) = restart(worker, handle, dir.path());
        assert_eq!(rebase_progress(&handle), Some((2, 2)));

        write_file(dir.path(), "f.txt", "base\n");
        assert_eq!(
            handle.rebase_continue().unwrap(),
            git_core::rebase::RebaseStepResult::Done
        );

        let reopened = Repository::open(dir.path()).unwrap();
        let head = reopened.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.summary().unwrap(), Some("b"));
        assert_ne!(head.tree_id(), head.parent(0).unwrap().tree_id());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
            "from b\n"
        );
    }

    /// The forced checkout of Abort can't create a file in a read-only folder, so Abort fails.
    /// The rebase must stay stored and HEAD where the rebase left it, so Abort can be retried.
    #[test]
    #[cfg(unix)]
    fn a_failed_abort_keeps_the_stored_rebase() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, repo) = init_repo();
        std::fs::create_dir(dir.path().join("d")).unwrap();
        write_file(dir.path(), "d/g.txt", "base\n");
        commit_all(&repo, "base");
        let onto = repo.head().unwrap().target().unwrap().to_string();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "a");
        let a = repo.head().unwrap().target().unwrap().to_string();
        // The branch tip has d/new.txt, which going back to the branch must create.
        write_file(dir.path(), "d/new.txt", "new\n");
        commit_all(&repo, "b");
        let branch = repo.head().unwrap().name().unwrap().to_string();
        let tip = repo.head().unwrap().target().unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let plan = vec![git_core::rebase::RebasePlanEntry {
            commit_id: a,
            action: git_core::rebase::RebaseAction::Edit,
            combined_message: None,
        }];
        assert_eq!(
            handle.start_rebase(onto, plan).unwrap(),
            git_core::rebase::RebaseStepResult::PausedForEdit
        );
        assert!(!dir.path().join("d/new.txt").exists());
        let folder = dir.path().join("d");
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::write(folder.join("probe"), "").is_ok() {
            // Root ignores folder permissions: no reliable failure to provoke here.
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }

        let failed = handle.abort_rebase();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(failed.is_err());
        assert!(rebase_file(&repo).exists());
        assert_eq!(rebase_progress(&handle), Some((1, 1)));
        assert!(Repository::open(dir.path())
            .unwrap()
            .head_detached()
            .unwrap());
        handle.abort_rebase().unwrap();
        let reopened = Repository::open(dir.path()).unwrap();
        assert_eq!(reopened.head().unwrap().name().unwrap(), branch);
        assert_eq!(reopened.head().unwrap().target().unwrap(), tip);
        assert_eq!(
            std::fs::read_to_string(folder.join("new.txt")).unwrap(),
            "new\n"
        );
        assert!(!rebase_file(&repo).exists());
    }

    #[test]
    fn a_failed_save_keeps_the_rebase_for_the_session() {
        let (dir, repo) = init_repo();
        let (onto, plan, branch, _) = edit_then_pick(dir.path(), &repo);
        std::fs::create_dir_all(rebase_file(&repo)).unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let error = handle.start_rebase(onto, plan).unwrap_err();

        assert!(error.contains("could not be saved"), "{error}");
        let short = branch.trim_start_matches("refs/heads/");
        assert!(error.contains(&format!("git switch {short}")), "{error}");
        assert_eq!(rebase_progress(&handle), Some((1, 2)));
        assert_eq!(
            handle.rebase_continue().unwrap(),
            git_core::rebase::RebaseStepResult::Done
        );
        assert_eq!(rebase_progress(&handle), None);
    }

    /// Every command that would move HEAD or rewrite the index under a stored rebase replies
    /// with the operation-in-progress text, and HEAD stays put.
    fn assert_moves_are_refused(handle: &super::WorkerHandle, path: &Path) {
        let busy = "Finish or abort the current merge/cherry-pick/revert/rebase first.";
        let repo = Repository::open(path).unwrap();
        let head = repo.head().unwrap().target().unwrap();
        let id = head.to_string();
        let branch = repo
            .branches(Some(git2::BranchType::Local))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .0
            .name()
            .unwrap()
            .unwrap()
            .to_string();
        assert_eq!(handle.switch_branch(branch.clone()).unwrap_err(), busy);
        assert_eq!(handle.cherry_pick(id.clone(), None).unwrap_err(), busy);
        assert_eq!(handle.cherry_pick_many(vec![id.clone()]).unwrap_err(), busy);
        assert_eq!(
            handle
                .commit("x".into(), false, Vec::new(), mpsc::channel().0)
                .unwrap_err(),
            busy
        );
        assert_eq!(
            handle
                .amend_commit("x".into(), false, None, Vec::new(), mpsc::channel().0)
                .unwrap_err(),
            busy
        );
        assert_eq!(
            handle
                .start_merge(branch, git_core::merge::MergeOptions::default())
                .unwrap_err(),
            busy
        );
        assert_eq!(handle.start_rebase(id.clone(), vec![]).unwrap_err(), busy);
        assert_eq!(
            handle
                .reset_to_commit(id.clone(), git_core::reset::ResetMode::Hard)
                .unwrap_err(),
            busy
        );
        assert_eq!(handle.checkout_commit(id.clone()).unwrap_err(), busy);
        assert_eq!(handle.pop_stash(0, id, false).unwrap_err(), busy);
        assert_eq!(repo.head().unwrap().target().unwrap(), head);
    }

    #[test]
    fn commands_are_refused_while_a_rebase_is_stored() {
        let (dir, repo) = init_repo();
        let (_worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
        assert_moves_are_refused(&handle, dir.path());
    }

    #[test]
    fn commands_are_refused_after_a_rebase_error_stop() {
        let (dir, repo) = init_repo();
        let (_worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
        write_file(dir.path(), "f.txt", "local\n");
        assert!(handle.rebase_continue().is_err());
        // To git nothing is going on now; only the stored rebase says otherwise.
        assert_eq!(repo.state(), git2::RepositoryState::Clean);
        assert_moves_are_refused(&handle, dir.path());
    }

    #[test]
    fn staging_and_conflict_resolution_stay_available_during_a_rebase() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "base\n");
        commit_all(&repo, "base");
        let onto = repo.head().unwrap().target().unwrap().to_string();
        write_file(dir.path(), "a.txt", "on top\n");
        commit_all(&repo, "on top");
        write_file(dir.path(), "a.txt", "again\n");
        commit_all(&repo, "again");
        let picked = repo.head().unwrap().target().unwrap().to_string();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let plan = vec![git_core::rebase::RebasePlanEntry {
            commit_id: picked,
            action: git_core::rebase::RebaseAction::Pick,
            combined_message: None,
        }];
        assert!(matches!(
            handle.start_rebase(onto, plan).unwrap(),
            git_core::rebase::RebaseStepResult::Conflicted { .. }
        ));

        handle
            .resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        write_file(dir.path(), "notes.txt", "notes\n");
        handle.stage_file("notes.txt".into()).unwrap();
        handle.unstage_file("notes.txt".into()).unwrap();

        assert_eq!(
            handle.rebase_continue().unwrap(),
            git_core::rebase::RebaseStepResult::Done
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "resolved\n"
        );
    }

    fn health(info: &git_core::repo::HeadInfo) -> git_core::rebase::RebaseHealth {
        info.rebase.as_ref().expect("a stored rebase").health
    }

    #[test]
    fn get_head_info_reports_a_paused_rebase_after_restart() {
        let (dir, repo) = init_repo();
        let (worker, handle, branch, _) = paused_for_edit(dir.path(), &repo);
        // An edit pause leaves HEAD on `onto`: the step is applied to the index only.
        let onto = Repository::open(dir.path())
            .unwrap()
            .head()
            .unwrap()
            .target()
            .unwrap();
        let (_worker, handle) = restart(worker, handle, dir.path());

        let info = handle.get_head_info().unwrap();

        let rebase = info.rebase.unwrap();
        assert_eq!(rebase.branch, branch.trim_start_matches("refs/heads/"));
        assert_eq!(rebase.onto, onto.to_string());
        assert_eq!((rebase.current_step, rebase.total_steps), (1, 2));
        assert_eq!(rebase.health, git_core::rebase::RebaseHealth::Paused);
    }

    #[test]
    fn get_head_info_reports_a_diverged_rebase_when_head_moved_outside() {
        let (dir, repo) = init_repo();
        let (worker, handle, branch, _) = paused_for_edit(dir.path(), &repo);
        drop((worker, handle));
        // `git switch <branch>` in a terminal while the app is closed.
        repo.set_head(&branch).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let info = handle.get_head_info().unwrap();

        assert_eq!(
            health(&info),
            git_core::rebase::RebaseHealth::Diverged(git_core::rebase::RebaseDivergence::HeadMoved)
        );
        assert_eq!(
            handle.checkout_commit(info.head_id.unwrap()).unwrap_err(),
            "Finish or abort the current merge/cherry-pick/revert/rebase first."
        );
        assert_eq!(
            handle.abort_rebase().unwrap(),
            git_core::rebase::RebaseAbort { restored: false }
        );
        assert_eq!(handle.get_head_info().unwrap().rebase, None);
        assert!(!rebase_file(&repo).exists());
    }

    #[test]
    fn get_head_info_reports_no_rebase_after_it_finishes() {
        let (dir, repo) = init_repo();
        let (_worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
        assert!(handle.get_head_info().unwrap().rebase.is_some());
        handle.rebase_continue().unwrap();
        assert_eq!(handle.get_head_info().unwrap().rebase, None);
    }

    fn no_undo() -> UndoState {
        UndoState {
            undo: None,
            redo: None,
        }
    }

    fn undo_file(repo: &Repository) -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(repo.path().join("browsitory/undo.json")).unwrap())
            .unwrap()
    }

    #[test]
    fn a_sequence_finished_after_a_restart_is_one_undo_entry() {
        let (dir, repo, ids) = side_commits_with_a_conflict();
        let original = repo.head().unwrap().target().unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert!(matches!(
            handle.cherry_pick_many(ids).unwrap(),
            git_core::pick::SequenceOutcome::Conflicted { .. }
        ));
        let (_worker, handle) = restart(worker, handle, dir.path());

        handle
            .resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        assert!(matches!(
            handle.continue_sequence().unwrap(),
            git_core::pick::SequenceOutcome::Completed { .. }
        ));

        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Cherry-pick 3 commits".into())
        );
        assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
        assert_eq!(repo.head().unwrap().target().unwrap(), original);
    }

    #[test]
    fn a_rebase_finished_after_a_restart_is_one_undo_entry() {
        let (dir, repo) = init_repo();
        let (worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
        let (_worker, handle) = restart(worker, handle, dir.path());
        write_file(dir.path(), "a.txt", "amended\n");
        handle.stage_file("a.txt".into()).unwrap();

        assert_eq!(
            handle.rebase_continue().unwrap(),
            git_core::rebase::RebaseStepResult::Done
        );

        let label = handle.get_undo_state().unwrap().undo.unwrap();
        assert!(label.starts_with("Rebase onto "), "{label}");
        assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
        assert_eq!(branch_target(dir.path(), &branch), tip);
    }

    #[test]
    fn a_merge_finished_after_a_restart_is_one_undo_entry() {
        let (dir, repo) = conflicting_branches();
        let main_tip = repo.head().unwrap().target().unwrap();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();
        let (_worker, handle) = restart(worker, handle, dir.path());

        handle
            .resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        handle
            .commit("Merge feature".into(), false, Vec::new(), mpsc::channel().0)
            .unwrap();

        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Merge feature".into())
        );
        assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
        let reopened = Repository::open(dir.path()).unwrap();
        assert_eq!(reopened.head().unwrap().target().unwrap(), main_tip);
    }

    #[test]
    fn a_rebase_aborted_after_a_restart_records_nothing() {
        let (dir, repo) = init_repo();
        let (worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
        let (_worker, handle) = restart(worker, handle, dir.path());
        handle.abort_rebase().unwrap();
        assert_eq!(handle.get_undo_state().unwrap(), no_undo());
        assert!(undo_file(&repo).get("pending").is_none());
    }

    /// `git merge --abort` in a terminal: forget the merge and reset to HEAD.
    fn abort_outside(repo: &Repository) {
        repo.cleanup_state().unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.reset(head.as_object(), git2::ResetType::Hard, None)
            .unwrap();
    }

    #[test]
    fn an_operation_aborted_outside_while_closed_drops_the_pending_entry() {
        let (dir, repo) = conflicting_branches();
        {
            let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
            worker
                .handle()
                .start_merge("feature".into(), git_core::merge::MergeOptions::default())
                .unwrap();
        }
        assert_eq!(undo_file(&repo)["pending"]["label"], "Merge feature");
        abort_outside(&repo);

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        assert_eq!(handle.get_undo_state().unwrap(), no_undo());
        assert!(undo_file(&repo).get("pending").is_none());
        handle.create_branch("after".into(), "HEAD".into()).unwrap();
        assert_eq!(
            handle.get_undo_state().unwrap().undo,
            Some("Create branch after".into())
        );
    }

    #[test]
    fn a_different_operation_started_outside_does_not_inherit_the_pending_entry() {
        let (dir, repo) = conflicting_branches();
        {
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            let base = head.parent(0).unwrap();
            repo.branch("other", &base, false).unwrap();
        }
        repo.set_head("refs/heads/other").unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        write_file(dir.path(), "a.txt", "other\n");
        commit_all(&repo, "other change");
        let main = repo
            .branches(Some(git2::BranchType::Local))
            .unwrap()
            .map(|branch| branch.unwrap().0.name().unwrap().unwrap().to_string())
            .find(|name| name != "other" && name != "feature")
            .unwrap();
        repo.set_head(&format!("refs/heads/{main}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        {
            let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
            worker
                .handle()
                .start_merge("feature".into(), git_core::merge::MergeOptions::default())
                .unwrap();
        }
        abort_outside(&repo);
        // `git merge other` in a terminal, which conflicts too.
        let other = repo
            .find_annotated_commit(repo.refname_to_id("refs/heads/other").unwrap())
            .unwrap();
        repo.merge(&[&other], None, None).unwrap();
        assert!(repo.index().unwrap().has_conflicts());

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        handle
            .commit("Merge other".into(), false, Vec::new(), mpsc::channel().0)
            .unwrap();

        // Not "Merge feature"; and "Merge other" can't be recorded: its "before" is unknown.
        assert_eq!(handle.get_undo_state().unwrap(), no_undo());
    }

    #[test]
    fn undo_refuses_while_a_restored_pending_entry_exists() {
        let (dir, repo) = conflicting_branches();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .create_branch("before".into(), "HEAD".into())
            .unwrap();
        handle
            .start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();
        let (_worker, handle) = restart(worker, handle, dir.path());
        assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
        assert_eq!(handle.redo().unwrap(), UndoOutcome::OperationInProgress);
        assert!(Repository::open(dir.path())
            .unwrap()
            .find_branch("before", git2::BranchType::Local)
            .is_ok());
        let _ = repo;
    }

    #[test]
    fn branch_and_stash_commands_are_refused_during_a_rebase() {
        let (dir, repo) = init_repo();
        let (_worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
        let busy = "Finish or abort the current merge/cherry-pick/revert/rebase first.";
        let short = branch.trim_start_matches("refs/heads/").to_string();
        assert_eq!(handle.delete_branch(short.clone(), true).unwrap_err(), busy);
        assert_eq!(
            handle.rename_branch(short, "renamed".into()).unwrap_err(),
            busy
        );
        assert_eq!(
            handle
                .save_stash(git_core::stash::StashOptions::default())
                .unwrap_err(),
            busy
        );
        let parent = TempDir::new().unwrap();
        assert_eq!(
            handle
                .create_worktree(
                    "rebased".into(),
                    parent.path().join("rebased"),
                    branch.trim_start_matches("refs/heads/").into(),
                    None,
                )
                .unwrap_err(),
            busy
        );
        assert_eq!(branch_target(dir.path(), &branch), tip);

        // Creating a branch checks it out (`git checkout -b`), which would move HEAD.
        assert_eq!(
            handle
                .create_branch("new".into(), tip.to_string())
                .unwrap_err(),
            busy
        );
        // Other branches stay manageable.
        repo.branch("other", &repo.find_commit(tip).unwrap(), false)
            .unwrap();
        handle
            .rename_branch("other".into(), "renamed".into())
            .unwrap();
        handle.delete_branch("renamed".into(), true).unwrap();
    }

    /// A `rebase.json` that can't be deleted (read-only folder) must not keep gating the
    /// repository after the rebase ended.
    #[test]
    #[cfg(unix)]
    fn a_rebase_file_that_cannot_be_removed_is_still_forgotten() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, repo) = init_repo();
        let (_worker, handle, branch, _) = paused_for_edit(dir.path(), &repo);
        let folder = repo.path().join("browsitory");
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::write(folder.join("probe"), "").is_ok() {
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
            return; // root: no reliable failure to provoke
        }

        let aborted = handle.abort_rebase();
        let rebase = handle.get_head_info().map(|info| info.rebase);
        let switched = handle.switch_branch(branch.trim_start_matches("refs/heads/").into());
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            aborted.unwrap(),
            git_core::rebase::RebaseAbort { restored: true }
        );
        assert!(rebase_file(&repo).exists());
        assert_eq!(rebase.unwrap(), None);
        assert!(switched.is_ok(), "{switched:?}");
    }

    #[test]
    fn a_pending_entry_recorded_by_another_worker_is_not_recorded_twice() {
        let (dir, _repo) = conflicting_branches();
        let first = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let second = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let (a, b) = (first.handle(), second.handle());
        a.start_merge("feature".into(), git_core::merge::MergeOptions::default())
            .unwrap();
        assert_eq!(b.get_undo_state().unwrap(), no_undo()); // b now knows the pending entry
        a.resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        a.commit("Merge feature".into(), false, Vec::new(), mpsc::channel().0)
            .unwrap();

        b.create_branch("after".into(), "HEAD".into()).unwrap();

        assert_eq!(
            b.undo().unwrap(),
            UndoOutcome::Done {
                label: "Create branch after".into()
            }
        );
        assert_eq!(
            b.undo().unwrap(),
            UndoOutcome::Done {
                label: "Merge feature".into()
            }
        );
        assert_eq!(b.undo().unwrap(), UndoOutcome::Nothing);
    }

    /// Another writer (the other app) moves HEAD while this worker is landing a step, so the
    /// step's commit loses HEAD's compare-and-swap. This worker's copy is then stale and must not
    /// be saved over the stored rebase. The race is provoked with a fake signing program that
    /// moves HEAD on its second call (the second step's commit).
    #[test]
    #[cfg(unix)]
    fn a_continue_that_loses_head_to_another_writer_saves_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, repo) = init_repo();
        let (_worker, handle, _, tip) = paused_for_edit(dir.path(), &repo);
        let tools = TempDir::new().unwrap();
        let mark = tools.path().join("called-once");
        let script = tools.path().join("fake-gpg");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\ncat > /dev/null\nif [ -e '{mark}' ]; then git -C '{workdir}' update-ref --no-deref HEAD {tip}; fi\ntouch '{mark}'\nprintf -- '-----BEGIN PGP SIGNATURE-----\\nfake\\n-----END PGP SIGNATURE-----\\n'\necho '[GNUPG:] SIG_CREATED D 1 8 00 0 X' >&2\n",
                mark = mark.display(),
                workdir = dir.path().display(),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_bool("commit.gpgSign", true).unwrap();
            config
                .set_str("gpg.program", script.to_str().unwrap())
                .unwrap();
        }
        let stored = std::fs::read_to_string(rebase_file(&repo)).unwrap();

        assert!(handle.rebase_continue().is_err());

        assert!(mark.exists(), "the fake signer never ran");
        assert_eq!(
            Repository::open(dir.path())
                .unwrap()
                .head()
                .unwrap()
                .target(),
            Some(tip)
        );
        assert_eq!(std::fs::read_to_string(rebase_file(&repo)).unwrap(), stored);
    }
}
