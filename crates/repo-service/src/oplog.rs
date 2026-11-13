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

    /// Adds a `Running` entry for an operation starting now and returns its id.
    pub fn begin(&self, start: Start) -> u64 {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let entry = OperationEntry {
            id,
            name: start.name,
            target: start.target.as_deref().map(redact),
            params: start
                .params
                .into_iter()
                .map(|param| OperationParam {
                    name: param.name,
                    value: redact(&param.value),
                })
                .collect(),
            started_at: now_ms(),
            duration_ms: None,
            outcome: OperationOutcome::Running,
            message: None,
            error_kind: None,
            transfer_error: None,
            remote_failures: Vec::new(),
            runs: Vec::new(),
            rerun: start.rerun,
        };
        {
            let mut entries = lock(&self.entries);
            entries.push_back(entry);
            while entries.len() > MAX_ENTRIES {
                entries.pop_front();
            }
        }
        self.changed(id);
        id
    }

    /// Ends entry `id` with `finish` (a no-op once the entry was dropped for newer ones).
    pub fn finish(&self, id: u64, finish: Finish) {
        {
            let mut entries = lock(&self.entries);
            let Some(entry) = entries.iter_mut().rev().find(|entry| entry.id == id) else {
                return;
            };
            let elapsed = now_ms().saturating_sub(entry.started_at);
            entry.duration_ms = Some(u64::try_from(elapsed).unwrap_or(0));
            entry.outcome = finish.outcome;
            entry.message = finish.message.as_deref().map(redact);
            entry.error_kind = finish.error_kind;
            entry.transfer_error = finish.transfer_error;
            entry.remote_failures = finish
                .remote_failures
                .into_iter()
                .map(|failure| RemoteFailure {
                    message: redact(&failure.message),
                    ..failure
                })
                .collect();
            entry.runs = finish
                .runs
                .into_iter()
                .take(MAX_RUNS)
                .map(|run| OperationRun {
                    program: redact(&run.program),
                    args: run.args.iter().map(|arg| redact(arg)).collect(),
                    output: tail(&redact(&run.output)),
                    status: run.status,
                })
                .collect();
        }
        self.changed(id);
    }

    /// Up to `limit` entries older than `before` (all when `None`), newest first.
    pub fn page(&self, before: Option<u64>, limit: usize) -> OperationPage {
        let entries = lock(&self.entries);
        let mut older = entries
            .iter()
            .rev()
            .filter(|entry| before.is_none_or(|before| entry.id < before));
        let page: Vec<OperationEntry> = older.by_ref().take(limit).cloned().collect();
        let has_more = older.next().is_some();
        OperationPage {
            repo: self.key.clone(),
            entries: page,
            has_more,
        }
    }

    /// Tells every listener that entry `entry_id` was added or ended.
    fn changed(&self, entry_id: u64) {
        let listeners: Vec<Listener> = lock(&LISTENERS)
            .iter()
            .map(|(_, listener)| Arc::clone(listener))
            .collect();
        let change = Change {
            repo: self.key.clone(),
            entry_id,
        };
        for listener in listeners {
            listener(&change);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

/// The last `MAX_RUN_OUTPUT` bytes of `output` (from a character boundary), marked when cut.
fn tail(output: &str) -> String {
    if output.len() <= MAX_RUN_OUTPUT {
        return output.to_string();
    }
    let mut start = output.len() - MAX_RUN_OUTPUT;
    while !output.is_char_boundary(start) {
        start += 1;
    }
    format!("{OUTPUT_OMITTED}{}", &output[start..])
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static JOURNALS: LazyLock<Mutex<HashMap<String, Arc<Journal>>>> = LazyLock::new(Default::default);
static LISTENERS: LazyLock<Mutex<Vec<(u64, Listener)>>> = LazyLock::new(Default::default);
static NEXT_LISTENER: AtomicU64 = AtomicU64::new(1);

/// The log of the repository with key `key`, made on first use.
pub fn journal(key: &str) -> Arc<Journal> {
    Arc::clone(
        lock(&JOURNALS)
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Journal::new(key.to_string()))),
    )
}

/// What a change listener is told: the repository key whose log changed, and the entry that
/// was added or ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub repo: String,
    pub entry_id: u64,
}

pub type Listener = Arc<dyn Fn(&Change) + Send + Sync>;

/// Keeps a listener registered with [`subscribe`] until dropped.
pub struct Subscription {
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        lock(&LISTENERS).retain(|(id, _)| *id != self.id);
    }
}

/// Calls `listener` (on the thread that changed the log) each time an entry is added or ends,
/// until the returned subscription is dropped.
pub fn subscribe(listener: Listener) -> Subscription {
    let id = NEXT_LISTENER.fetch_add(1, Ordering::Relaxed);
    lock(&LISTENERS).push((id, listener));
    Subscription { id }
}

/// `text` without credentials: the user information of every URL in it (`scheme://user:pass@`
/// → `scheme://***@`) and the value of every `Authorization` header line.
pub fn redact(text: &str) -> String {
    let without_userinfo = redact_userinfo(text);
    if !without_userinfo
        .to_ascii_lowercase()
        .contains(AUTHORIZATION)
    {
        return without_userinfo;
    }
    without_userinfo
        .split_inclusive('\n')
        .map(redact_authorization)
        .collect()
}

const AUTHORIZATION: &str = "authorization:";

/// `line` with the value after `Authorization:` replaced, its line ending kept.
fn redact_authorization(line: &str) -> String {
    let Some(at) = line.to_ascii_lowercase().find(AUTHORIZATION) else {
        return line.to_string();
    };
    let end = at + AUTHORIZATION.len();
    let ending = &line[line.trim_end_matches(['\r', '\n']).len()..];
    format!("{} {REDACTED}{ending}", &line[..end])
}

/// Whether `c` ends a URL's authority (`user:pass@host:port`).
fn ends_authority(c: char) -> bool {
    matches!(
        c,
        '/' | '?' | '#' | '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '`' | ','
    ) || c.is_whitespace()
}

fn redact_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let authority_start = at + 3;
        out.push_str(&rest[..authority_start]);
        let after = &rest[authority_start..];
        let authority_len = after.find(ends_authority).unwrap_or(after.len());
        let authority = &after[..authority_len];
        match authority.rfind('@') {
            Some(user_end) => {
                out.push_str(REDACTED);
                out.push_str(&authority[user_end..]);
            }
            None => out.push_str(authority),
        }
        rest = &after[authority_len..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests;
