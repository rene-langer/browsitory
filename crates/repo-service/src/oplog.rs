//! The operation log (#887): what the app ran in each repository, newest first, for the log
//! panel and its Run again.
//!
//! The worker notes every command that changes something (or talks to a remote) in its
//! repository's [`Journal`]: a `Running` entry when the command starts, then its outcome, how
//! long it took, its error, and the programs it ran (hooks, signing programs) with their output.
//! Reads are not noted.
//!
//! Memory only (owner decision on #887): each repository keeps its last [`MAX_ENTRIES`] entries
//! for as long as the process runs, in a process-wide map keyed by the repository key
//! (`worker::repo_key`), so closing and reopening a repository keeps its log and a restart
//! clears it. Nothing is written to disk.
//!
//! Nothing secret is kept: URLs lose their user information and `Authorization` headers their
//! value ([`redact`]) in every parameter, message, argument and output, and commands that carry
//! a token (saving a credential) note no token at all.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use git_core::process::{ProgramRun, ProgramRunStatus};
use git_core::remote::TransferErrorKind;

use crate::wire::ErrorKind;

/// The most entries a repository's log keeps; the oldest goes first.
pub const MAX_ENTRIES: usize = 500;
/// The most program runs an entry keeps (a long sequence runs hooks for every step).
pub const MAX_RUNS: usize = 32;
/// The most output an entry keeps per program run: its end, where a failing program says why.
pub const MAX_RUN_OUTPUT: usize = 16 * 1024;
/// What replaces the start of a longer output.
pub const OUTPUT_OMITTED: &str = "[… earlier output omitted …]\n";
/// What replaces user information in a URL and the value of an `Authorization` header.
pub const REDACTED: &str = "***";

/// One operation the app ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationEntry {
    /// Unique in the process and increasing: a later operation has a larger id.
    pub id: u64,
    /// What ran: "Commit", "Push", "Switch branch", ...
    pub name: String,
    /// What it ran on, when it has one target: a branch, a remote, a commit, a file.
    pub target: Option<String>,
    /// The rest of what it was given, for the details view.
    pub params: Vec<OperationParam>,
    /// Milliseconds since the Unix epoch.
    pub started_at: i64,
    /// `None` while it runs.
    pub duration_ms: Option<u64>,
    pub outcome: OperationOutcome,
    /// Why it failed, stopped or was cancelled.
    pub message: Option<String>,
    /// The failed reply's kind.
    pub error_kind: Option<ErrorKind>,
    /// A fetch's, pull's or push's failure, as its transfer progress reports it.
    pub transfer_error: Option<TransferErrorKind>,
    /// The remotes a Fetch all failed to fetch (the operation succeeds when one remote worked).
    pub remote_failures: Vec<RemoteFailure>,
    /// The programs it ran, in order, at most [`MAX_RUNS`].
    pub runs: Vec<OperationRun>,
    /// How to run it again; only fetches, pulls and pushes can be (owner decision on #890).
    pub rerun: Option<Rerun>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationParam {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    Running,
    Succeeded,
    /// It stopped part way, as asked of it: on conflicts, or with branches that diverged.
    Stopped,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFailure {
    pub remote_name: String,
    pub error: TransferErrorKind,
    pub message: String,
}

/// A program an operation ran (see `git_core::process::ProgramRun`), redacted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRun {
    pub program: String,
    pub args: Vec<String>,
    pub output: String,
    pub status: ProgramRunStatus,
}

/// What Run again repeats: the same command with the same parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rerun {
    Fetch {
        remote_name: String,
        prune: Option<bool>,
    },
    FetchAll {
        prune: Option<bool>,
    },
    Pull,
    PushCurrentBranch {
        remote_name: String,
        no_verify: bool,
    },
    PushBranch {
        remote_name: String,
        local_branch: String,
        remote_branch: String,
        set_upstream: bool,
        no_verify: bool,
    },
    PushTags {
        remote_name: String,
        names: Vec<String>,
        no_verify: bool,
    },
}

/// What a new entry starts with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Start {
    pub name: String,
    pub target: Option<String>,
    pub params: Vec<OperationParam>,
    pub rerun: Option<Rerun>,
}

/// How an entry ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finish {
    pub outcome: OperationOutcome,
    pub message: Option<String>,
    pub error_kind: Option<ErrorKind>,
    pub transfer_error: Option<TransferErrorKind>,
    pub remote_failures: Vec<RemoteFailure>,
    pub runs: Vec<ProgramRun>,
}

impl Finish {
    pub fn succeeded() -> Self {
        Self {
            outcome: OperationOutcome::Succeeded,
            message: None,
            error_kind: None,
            transfer_error: None,
            remote_failures: Vec::new(),
            runs: Vec::new(),
        }
    }
}

/// One page of a repository's log, newest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationPage {
    /// The repository key the entries belong to; change events name the same key.
    pub repo: String,
    pub entries: Vec<OperationEntry>,
    /// Older entries follow the last one.
    pub has_more: bool,
}

/// One repository's log.
pub struct Journal {
    key: String,
    entries: Mutex<VecDeque<OperationEntry>>,
}

impl Journal {
    fn new(key: String) -> Self {
        Self {
            key,
            entries: Mutex::new(VecDeque::new()),
        }
    }

    /// The repository key this log belongs to.
    pub fn key(&self) -> &str {
        &self.key
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<OperationEntry>> {
        self.entries
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Adds a `Running` entry for an operation starting now and returns its id.
    pub fn begin(&self, start: Start) -> u64 {
        let _ = start;
        0
    }

    /// Ends entry `id` with `finish` (a no-op once the entry was dropped for newer ones).
    pub fn finish(&self, id: u64, finish: Finish) {
        let _ = (id, finish);
    }

    /// Up to `limit` entries older than `before` (all when `None`), newest first.
    pub fn page(&self, before: Option<u64>, limit: usize) -> OperationPage {
        let _ = (before, limit, self.lock());
        OperationPage {
            repo: self.key.clone(),
            entries: Vec::new(),
            has_more: false,
        }
    }
}

/// The log of the repository with key `key`, made on first use.
pub fn journal(key: &str) -> Arc<Journal> {
    Arc::new(Journal::new(key.to_string()))
}

/// What a change listener is told: the repository key whose log changed, and the entry that
/// was added or ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub repo: String,
    pub entry_id: u64,
}

type Listener = Arc<dyn Fn(&Change) + Send + Sync>;

/// Keeps a listener registered with [`subscribe`] until dropped.
pub struct Subscription {
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let _ = self.id;
    }
}

/// Calls `listener` (on the thread that changed the log) each time an entry is added or ends,
/// until the returned subscription is dropped.
pub fn subscribe(listener: Listener) -> Subscription {
    let _ = listener;
    Subscription { id: 0 }
}

/// `text` without credentials: the user information of every URL in it (`scheme://user:pass@`
/// → `scheme://***@`) and the value of every `Authorization` header line.
pub fn redact(text: &str) -> String {
    text.to_string()
}

#[allow(dead_code)]
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
#[allow(dead_code)]
static JOURNALS: LazyLock<Mutex<HashMap<String, Arc<Journal>>>> = LazyLock::new(Default::default);
#[allow(dead_code)]
static LISTENERS: LazyLock<Mutex<Vec<(u64, Listener)>>> = LazyLock::new(Default::default);
#[allow(dead_code)]
fn unused() {
    let _ = Ordering::SeqCst;
}

#[cfg(test)]
mod tests;
