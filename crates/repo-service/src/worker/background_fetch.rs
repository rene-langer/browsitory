//! The "Fetch automatically" tick (#440), run so that a remote that doesn't answer never holds
//! the worker (#462).
//!
//! A fetch blocked connecting (an unreachable host, a VPN that is down, a server that accepts
//! and never answers) calls none of libgit2's callbacks, so the cancel side channel
//! (`CancelRegistry`) can't reach it. A background fetch therefore fetches each remote on a
//! helper thread with its own `Repository`, and the worker waits for it, polling: while the
//! helper is still connecting, the worker gives up on it when the user cancels (they started an
//! action) or when it has been connecting for `BACKGROUND_CONNECT_LIMIT`, and goes on. The helper
//! is abandoned, not stopped: it stays blocked until the operating system gives up on the
//! connection, but it writes nothing, because a fetch only downloads and updates references
//! after `ConnectWatch::connected`, which answers `false` once the worker gave up. Once
//! connected, the fetch reports progress and the worker waits for it as for any fetch; a cancel
//! then reaches it through the callbacks.
//!
//! A remote that could not be reached is skipped by later background fetches for a while: the
//! interval doubles with each failure in a row, up to an hour (`Memory`). A remote whose
//! abandoned helper is still connecting is skipped too, so at most one helper per remote lingers.
use crate::wire::ErrorDto;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use git_core::remote::{RemoteError, TransferErrorKind, TransferOperation, TransferProgress};

use super::remote::TransferOp;
use super::TransferEvent;
use crate::credentials::{CredentialService, CredentialStore};

/// How long a background fetch may take to connect to one remote (reach the host and receive
/// its references) before the worker gives up on that remote for this tick. Manual fetches have
/// no such limit: libgit2's own timeouts are process-wide, so they are left alone.
pub(super) const BACKGROUND_CONNECT_LIMIT: Duration = Duration::from_secs(30);

/// The longest a remote that could not be reached waits for its next background fetch, unless
/// the interval itself is longer.
const MAX_BACKOFF: Duration = Duration::from_secs(60 * 60);

/// How often the worker looks at a connecting helper.
const POLL: Duration = Duration::from_millis(25);

/// What one background fetch is asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackgroundFetchOptions {
    pub(crate) prune: Option<bool>,
    /// The "Fetch automatically" interval, which a backoff doubles.
    pub(crate) interval: Duration,
    /// `BACKGROUND_CONNECT_LIMIT`, shorter in tests.
    pub(crate) connect_limit: Duration,
}

impl BackgroundFetchOptions {
    pub(crate) fn new(prune: Option<bool>, interval_minutes: u32) -> Self {
        Self {
            prune,
            interval: Duration::from_secs(u64::from(interval_minutes.max(1)) * 60),
            connect_limit: BACKGROUND_CONNECT_LIMIT,
        }
    }
}

/// What the background fetches of one worker remember between ticks: remotes backing off after
/// they could not be reached, and remotes whose abandoned helper is still connecting. Shared
/// with the helpers, which leave `connecting` when they end.
#[derive(Clone, Default)]
pub(crate) struct Memory(Arc<Mutex<MemoryState>>);

#[derive(Default)]
struct MemoryState {
    connecting: HashSet<String>,
    backoff: HashMap<String, Backoff>,
}

#[derive(Debug, Clone, Copy)]
struct Backoff {
    /// Failures to reach the remote in a row.
    failures: u32,
    /// Background fetches starting before this skip the remote.
    retry_at: Instant,
}

impl Memory {
    fn state(&self) -> MutexGuard<'_, MemoryState> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Whether the background fetch starting at `tick` tries `remote_name`.
    pub(super) fn may_try(&self, remote_name: &str, tick: Instant) -> bool {
        let state = self.state();
        !state.connecting.contains(remote_name)
            && state
                .backoff
                .get(remote_name)
                .is_none_or(|backoff| tick >= backoff.retry_at)
    }

    /// `remote_name` could not be reached by the background fetch that started at `tick`: skip it
    /// until the doubled interval is (about) over. Ticks come every `interval` after the previous
    /// one started, so the next try is due half an interval early, to land on a tick.
    pub(super) fn unreachable(&self, remote_name: &str, tick: Instant, interval: Duration) {
        let mut state = self.state();
        let failures = state
            .backoff
            .get(remote_name)
            .map_or(0, |backoff| backoff.failures)
            .saturating_add(1);
        let delay = interval
            .saturating_mul(2u32.saturating_pow(failures))
            .min(MAX_BACKOFF.max(interval));
        state.backoff.insert(
            remote_name.to_string(),
            Backoff {
                failures,
                retry_at: tick + delay.saturating_sub(interval / 2),
            },
        );
    }

    /// `remote_name` answered (a fetch of it connected, background or not): no more backoff.
    pub(super) fn reachable(&self, remote_name: &str) {
        self.state().backoff.remove(remote_name);
    }

    /// Marks `remote_name` as having a helper; the returned guard unmarks it when dropped.
    fn connecting(&self, remote_name: &str) -> ConnectingGuard {
        self.state().connecting.insert(remote_name.to_string());
        ConnectingGuard {
            memory: self.clone(),
            remote_name: remote_name.to_string(),
        }
    }
}

/// Held by a helper thread for as long as it lives (see `Memory::connecting`).
struct ConnectingGuard {
    memory: Memory,
    remote_name: String,
}

impl Drop for ConnectingGuard {
    fn drop(&mut self) {
        self.memory.state().connecting.remove(&self.remote_name);
    }
}

/// Where a helper's fetch is, as the worker and the helper agree on it under one lock: the worker
/// can only give up on a fetch that is still `Connecting`, and a fetch only goes on from
/// `Connecting` to `Transferring` while the worker hasn't given up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Connecting(Instant),
    Transferring,
    Abandoned,
}

#[derive(Clone)]
struct SharedPhase(Arc<Mutex<Phase>>);

impl SharedPhase {
    fn lock(&self) -> MutexGuard<'_, Phase> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
}

impl git_core::remote::ConnectWatch for SharedPhase {
    fn connected(&self) -> bool {
        let mut phase = self.lock();
        if *phase == Phase::Abandoned {
            return false;
        }
        *phase = Phase::Transferring;
        true
    }
}

/// What stops a helper's fetch: the worker gave up on it (the registry has forgotten the
/// operation then), or the user cancelled while it transfers.
fn helper_cancelled(phase: &SharedPhase, user_cancel: &dyn Fn() -> bool) -> bool {
    *phase.lock() == Phase::Abandoned || user_cancel()
}

enum HelperMessage {
    Progress(TransferProgress),
    Done(Result<(), RemoteError>),
}

/// Sends a helper's progress to the worker, which passes it on while it still waits for that
/// helper. The helper never holds the caller's event channel, so an abandoned helper keeps no
/// listener waiting for the operation's events to end.
struct HelperReporter(Sender<HelperMessage>);

impl git_core::remote::TransferReporter for HelperReporter {
    fn report(&mut self, progress: TransferProgress) {
        let _ = self.0.send(HelperMessage::Progress(progress));
    }
}

enum RemoteOutcome {
    Fetched,
    Failed {
        error: TransferErrorKind,
        /// Not reached: the connect failed at the network level or ran out of time.
        unreachable: bool,
    },
    Cancelled,
}

pub(super) fn run<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    memory: &Memory,
    options: BackgroundFetchOptions,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    const OPERATION: TransferOperation = TransferOperation::BackgroundFetch;
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: OPERATION,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let tick = Instant::now();
    let cancel = op.cancel_check();
    let names: Result<Vec<String>, git2::Error> = repo.remotes().map(|names| {
        names
            .iter()
            .flatten()
            .flatten()
            .map(str::to_string)
            .collect()
    });
    let mut fetched_any = false;
    let mut failed: Vec<(String, TransferErrorKind)> = Vec::new();
    let mut cancelled = false;
    let mut error = None;
    match names {
        Err(_) => error = Some(TransferErrorKind::TransferFailed),
        Ok(names) => {
            for name in names {
                if cancel() {
                    cancelled = true;
                    break;
                }
                // Skipped quietly, as a failed remote: the tick does not count as a fetch.
                if !memory.may_try(&name, tick) {
                    failed.push((name, TransferErrorKind::TransferFailed));
                    continue;
                }
                match fetch_one(repo, credentials, memory, &name, &options, &op, &events) {
                    RemoteOutcome::Fetched => {
                        memory.reachable(&name);
                        fetched_any = true;
                    }
                    RemoteOutcome::Failed { error, unreachable } => {
                        if unreachable {
                            memory.unreachable(&name, tick, options.interval);
                        } else {
                            memory.reachable(&name);
                        }
                        failed.push((name, error));
                    }
                    RemoteOutcome::Cancelled => {
                        cancelled = true;
                        break;
                    }
                }
            }
        }
    }
    if cancelled {
        error = Some(TransferErrorKind::Cancelled);
    } else if error.is_none() {
        for (remote_name, kind) in &failed {
            let _ = events.send(TransferEvent::RemoteFailed {
                operation_id: op.id.clone(),
                operation: OPERATION,
                remote_name: remote_name.clone(),
                error: *kind,
            });
        }
        // As for any Fetch all, only a fetch where nothing succeeded fails as a whole.
        if !fetched_any {
            error = failed.first().map(|(_, kind)| *kind);
        }
    }
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.finish(),
        operation: OPERATION,
        error,
        hook_output: None,
    });
}

/// Fetches one remote on a helper thread and waits for it, giving up on it while it is still
/// connecting when the user cancels or the connect limit runs out (see the module docs).
fn fetch_one<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    memory: &Memory,
    remote_name: &str,
    options: &BackgroundFetchOptions,
    op: &TransferOp,
    events: &Sender<TransferEvent>,
) -> RemoteOutcome {
    let phase = SharedPhase(Arc::new(Mutex::new(Phase::Connecting(Instant::now()))));
    let user_cancel = op.cancel_check();
    let (tx, rx) = mpsc::channel();
    let spawned = {
        let guard = memory.connecting(remote_name);
        let git_dir = repo.path().to_path_buf();
        let credentials = Arc::clone(credentials);
        let remote_name = remote_name.to_string();
        let operation_id = op.id.clone();
        let prune = options.prune;
        let phase = phase.clone();
        let user_cancel = op.cancel_check();
        std::thread::Builder::new()
            .name("background-fetch".to_string())
            .spawn(move || {
                let _guard = guard;
                let cancel = || helper_cancelled(&phase, &user_cancel);
                let result = git2::Repository::open(&git_dir)
                    .map_err(RemoteError::from)
                    .and_then(|repo| {
                        let profile = git_core::remote::remote_auth_profile(&repo, &remote_name)?;
                        let mut provider =
                            crate::credentials::RemoteCredentialProvider::without_prompt(
                                &credentials,
                                profile,
                            );
                        git_core::remote::fetch_remote_watched(
                            TransferOperation::BackgroundFetch,
                            &repo,
                            &remote_name,
                            prune,
                            &operation_id,
                            &mut provider,
                            &mut HelperReporter(tx.clone()),
                            &cancel,
                            &phase,
                        )
                    });
                let _ = tx.send(HelperMessage::Done(result));
            })
    };
    if spawned.is_err() {
        return RemoteOutcome::Failed {
            error: TransferErrorKind::TransferFailed,
            unreachable: false,
        };
    }
    loop {
        match rx.recv_timeout(POLL) {
            // Already stamped with the operation's id (the helper fetches under it).
            Ok(HelperMessage::Progress(progress)) => {
                let _ = events.send(TransferEvent::Progress(progress));
            }
            Ok(HelperMessage::Done(result)) => return outcome_of(result),
            // The helper panicked.
            Err(RecvTimeoutError::Disconnected) => {
                return RemoteOutcome::Failed {
                    error: TransferErrorKind::TransferFailed,
                    unreachable: false,
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        let mut current = phase.lock();
        if let Phase::Connecting(since) = *current {
            if user_cancel() {
                *current = Phase::Abandoned;
                return RemoteOutcome::Cancelled;
            }
            if since.elapsed() >= options.connect_limit {
                *current = Phase::Abandoned;
                return RemoteOutcome::Failed {
                    error: TransferErrorKind::TransferFailed,
                    unreachable: true,
                };
            }
        }
    }
}

fn outcome_of(result: Result<(), RemoteError>) -> RemoteOutcome {
    match result {
        Ok(()) => RemoteOutcome::Fetched,
        Err(RemoteError::Cancelled) => RemoteOutcome::Cancelled,
        Err(error @ RemoteError::Unreachable(_)) => RemoteOutcome::Failed {
            error: error.transfer_error_kind(),
            unreachable: true,
        },
        Err(error) => RemoteOutcome::Failed {
            error: error.transfer_error_kind(),
            unreachable: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn an_unreachable_remote_backs_off_doubling_the_interval_up_to_an_hour() {
        let memory = Memory::default();
        let start = Instant::now();
        let interval = 5 * MINUTE;
        // Ticks every five minutes; the remote is never reachable.
        let mut tried = Vec::new();
        for minute in (0..=180).step_by(5) {
            let tick = start + minute * MINUTE;
            if memory.may_try("origin", tick) {
                tried.push(minute);
                memory.unreachable("origin", tick, interval);
            }
        }
        assert_eq!(tried, [0, 10, 30, 70, 130]);
    }

    #[test]
    fn a_tick_that_comes_a_little_early_still_tries_again() {
        let memory = Memory::default();
        let start = Instant::now() + 10 * MINUTE;
        let interval = 5 * MINUTE;
        memory.unreachable("origin", start, interval);
        // The tick after next, a second earlier than two intervals.
        let early = start + 2 * interval - Duration::from_secs(1);
        assert!(!memory.may_try("origin", start + interval));
        assert!(memory.may_try("origin", early));
    }

    #[test]
    fn an_interval_of_an_hour_or_more_is_not_lengthened() {
        let memory = Memory::default();
        let start = Instant::now();
        memory.unreachable("origin", start, 90 * MINUTE);
        assert!(!memory.may_try("origin", start + 30 * MINUTE));
        assert!(memory.may_try("origin", start + 90 * MINUTE));
    }

    #[test]
    fn a_reachable_remote_is_tried_again_at_once() {
        let memory = Memory::default();
        let start = Instant::now();
        memory.unreachable("origin", start, 5 * MINUTE);
        assert!(!memory.may_try("origin", start + 5 * MINUTE));
        assert!(memory.may_try("upstream", start + 5 * MINUTE));
        memory.reachable("origin");
        assert!(memory.may_try("origin", start + 5 * MINUTE));
    }

    fn phase(phase: Phase) -> SharedPhase {
        SharedPhase(Arc::new(Mutex::new(phase)))
    }

    #[test]
    fn a_connected_fetch_goes_on_unless_the_worker_gave_up_on_it() {
        use git_core::remote::ConnectWatch;
        let connecting = phase(Phase::Connecting(Instant::now()));
        assert!(connecting.connected());
        assert_eq!(*connecting.lock(), Phase::Transferring);
        let abandoned = phase(Phase::Abandoned);
        assert!(!abandoned.connected());
        assert_eq!(*abandoned.lock(), Phase::Abandoned);
    }

    #[test]
    fn a_helper_stops_when_abandoned_or_cancelled() {
        let transferring = phase(Phase::Transferring);
        assert!(!helper_cancelled(&transferring, &|| false));
        assert!(helper_cancelled(&transferring, &|| true));
        assert!(helper_cancelled(&phase(Phase::Abandoned), &|| false));
    }

    #[test]
    fn a_remote_with_a_helper_still_connecting_is_skipped() {
        let memory = Memory::default();
        let guard = memory.connecting("origin");
        assert!(!memory.may_try("origin", Instant::now()));
        drop(guard);
        assert!(memory.may_try("origin", Instant::now()));
    }
}
