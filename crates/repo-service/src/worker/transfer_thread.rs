//! Fetches, pulls and pushes run so that a remote that doesn't answer never holds the worker
//! past a cancel (#462 for the background fetch, #547 for the transfers the user starts).
//!
//! A transfer blocked connecting (an unreachable host, a VPN that is down, a server that accepts
//! and never answers) calls none of libgit2's progress callbacks, so the cancel side channel
//! (`CancelRegistry`) can't reach it. The transfer therefore runs on a helper thread with its own
//! `Repository`, and the worker waits for it, polling: while the helper is still connecting, the
//! worker gives up on it when the user cancels (or, for a background fetch, when it has been
//! connecting too long) and goes on. The helper is abandoned, not stopped: it stays blocked until
//! the operating system gives up on the connection, but it writes nothing, because a fetch or push
//! only downloads, sends, runs `pre-push` and updates references after
//! `TransferReporter::connected`, which answers `false` once the worker gave up. Once connected,
//! the transfer reports progress and the worker waits for it as before; a cancel then reaches it
//! through the callbacks.
//!
//! The worker waits for the helper whatever happens, so the helper's writes never overlap
//! another command's. While it waits, its own `Repository` is idle, so it serves the read lane's
//! commands on it (`TransferHost::serve_read`, #734): they only read, and the helper's writes (a
//! new pack renamed into place, then remote-tracking refs) never leave a ref naming a missing
//! object. Everything else still waits for the transfer.
use std::sync::mpsc::{self, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use git_core::remote::{CredentialProvider, RemoteError, TransferProgress, TransferReporter};

use super::remote::TransferOp;
use super::TransferEvent;
use crate::credentials::{CredentialService, CredentialStore, RemoteCredentialProvider};

/// How often the worker looks at a connecting helper.
const POLL: Duration = Duration::from_millis(25);

/// What the worker lends a transfer: its repository, and the queued reads it serves while it
/// waits (see the module docs).
pub(super) trait TransferHost {
    fn repo(&mut self) -> &mut git2::Repository;
    /// Runs the next queued read-lane command, if there is one: `true` when one ran.
    fn serve_read(&mut self) -> bool;
}

/// How the worker waits for one transfer.
pub(super) struct Wait<'a> {
    /// The operation the transfer runs under: its cancel, and the ID its progress is sent with.
    pub(super) op: &'a TransferOp,
    /// Where the transfer's progress goes, stamped with `op`'s ID.
    pub(super) events: &'a Sender<TransferEvent>,
    /// How long it may take to connect before the worker gives up on it, as
    /// `RemoteError::Unreachable`. `None` (the transfers the user starts) waits for libgit2's own
    /// timeouts, which are process-wide and left alone.
    pub(super) connect_limit: Option<Duration>,
    /// `false` for a background fetch: HTTPS tokens are read without making the OS credential
    /// store prompt (see `CredentialSession`).
    pub(super) prompt: bool,
}

/// Runs `transfer` against `remote_name` on a helper thread and waits for it (see the module
/// docs), serving `host`'s reads meanwhile. The helper opens its own `Repository` and credential
/// provider for the remote; the transfer gets them, a reporter whose progress the worker passes
/// on, and the cancel to poll.
///
/// A cancel while the transfer is still connecting is `RemoteError::Cancelled` at once, a connect
/// that outlasts `connect_limit` `RemoteError::Unreachable`; the abandoned helper writes nothing.
pub(super) fn run<S, T, F>(
    host: &mut dyn TransferHost,
    credentials: &Arc<CredentialService<S>>,
    remote_name: &str,
    wait: Wait<'_>,
    transfer: F,
) -> Result<T, RemoteError>
where
    S: CredentialStore + Send + Sync + 'static,
    T: Send + 'static,
    F: FnOnce(
            &git2::Repository,
            &mut dyn CredentialProvider,
            &mut dyn TransferReporter,
            &dyn Fn() -> bool,
        ) -> Result<T, RemoteError>
        + Send
        + 'static,
{
    let phase = SharedPhase(Arc::new(Mutex::new(Phase::Connecting(Instant::now()))));
    let user_cancel = wait.op.cancel_check();
    let (tx, rx) = mpsc::channel();
    let spawned = {
        // The work tree when there is one, so the helper opens the repository the worker did
        // (its `pre-push` runs there, as on the worker).
        let repo = host.repo();
        let path = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();
        let credentials = Arc::clone(credentials);
        let remote_name = remote_name.to_string();
        let prompt = wait.prompt;
        let phase = phase.clone();
        let user_cancel = wait.op.cancel_check();
        // The step's program log (the operation log, #887): `pre-push` runs on the helper.
        let runs = git_core::process::current_run_log().unwrap_or_default();
        std::thread::Builder::new()
            .name("transfer".to_string())
            .spawn(move || {
                git_core::process::record_runs(&runs, || {
                    let cancel = || helper_cancelled(&phase, &user_cancel);
                    let mut reporter = HelperReporter {
                        messages: tx.clone(),
                        phase: phase.clone(),
                    };
                    let result = git2::Repository::open(&path)
                        .map_err(RemoteError::from)
                        .and_then(|repo| {
                            let profile =
                                git_core::remote::remote_auth_profile(&repo, &remote_name)?;
                            let mut provider = if prompt {
                                RemoteCredentialProvider::new(&credentials, profile)
                            } else {
                                RemoteCredentialProvider::without_prompt(&credentials, profile)
                            };
                            transfer(&repo, &mut provider, &mut reporter, &cancel)
                        });
                    let _ = tx.send(HelperMessage::Done(result));
                })
            })
    };
    if spawned.is_err() {
        return Err(helper_lost());
    }
    loop {
        // After a read, look at the helper without waiting: more reads may be queued.
        let message = if host.serve_read() {
            match rx.try_recv() {
                Ok(message) => Some(message),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return Err(helper_lost()),
            }
        } else {
            match rx.recv_timeout(POLL) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => None,
                // The helper panicked.
                Err(RecvTimeoutError::Disconnected) => return Err(helper_lost()),
            }
        };
        match message {
            Some(HelperMessage::Progress(progress)) => {
                let _ = wait.events.send(TransferEvent::Progress(TransferProgress {
                    operation_id: wait.op.id.clone(),
                    ..progress
                }));
            }
            Some(HelperMessage::Done(result)) => return result,
            None => {}
        }
        if let Some(error) = give_up(&phase, &user_cancel, wait.connect_limit) {
            return Err(error);
        }
    }
}

/// Whether the worker gives up on a helper now: only while it is still connecting, when the
/// user cancelled or the connect limit ran out. The phase becomes `Abandoned` under the same lock
/// the helper's `connected` takes, so the helper can't go on to transfer after this.
fn give_up(
    phase: &SharedPhase,
    user_cancel: &dyn Fn() -> bool,
    connect_limit: Option<Duration>,
) -> Option<RemoteError> {
    let mut current = phase.lock();
    let Phase::Connecting(since) = *current else {
        return None;
    };
    let error = if user_cancel() {
        RemoteError::Cancelled
    } else if connect_limit.is_some_and(|limit| since.elapsed() >= limit) {
        RemoteError::Unreachable(git2::Error::from_str("connecting took too long"))
    } else {
        return None;
    };
    *current = Phase::Abandoned;
    Some(error)
}

fn helper_lost() -> RemoteError {
    RemoteError::Git(git2::Error::from_str(
        "the transfer thread ended unexpectedly",
    ))
}

/// Where a helper's transfer is, as the worker and the helper agree on it under one lock: the
/// worker can only give up on a transfer that is still `Connecting`, and a transfer only goes on
/// from `Connecting` to `Transferring` while the worker hasn't given up.
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

    /// The transfer connected: it goes on unless the worker gave up on it.
    fn connected(&self) -> bool {
        let mut phase = self.lock();
        if *phase == Phase::Abandoned {
            return false;
        }
        *phase = Phase::Transferring;
        true
    }
}

/// What stops a helper's transfer: the worker gave up on it (the registry may have forgotten the
/// operation then), or the user cancelled while it transfers.
fn helper_cancelled(phase: &SharedPhase, user_cancel: &dyn Fn() -> bool) -> bool {
    *phase.lock() == Phase::Abandoned || user_cancel()
}

enum HelperMessage<T> {
    Progress(TransferProgress),
    Done(Result<T, RemoteError>),
}

/// Sends a helper's progress to the worker, which passes it on while it still waits for that
/// helper. The helper never holds the caller's event channel, so an abandoned helper keeps no
/// listener waiting for the operation's events to end.
struct HelperReporter<T> {
    messages: Sender<HelperMessage<T>>,
    phase: SharedPhase,
}

impl<T> TransferReporter for HelperReporter<T> {
    fn report(&mut self, progress: TransferProgress) {
        let _ = self.messages.send(HelperMessage::Progress(progress));
    }

    fn connected(&mut self) -> bool {
        self.phase.connected()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phase(phase: Phase) -> SharedPhase {
        SharedPhase(Arc::new(Mutex::new(phase)))
    }

    #[test]
    fn a_connected_transfer_goes_on_unless_the_worker_gave_up_on_it() {
        let connecting = phase(Phase::Connecting(Instant::now()));
        assert!(connecting.connected());
        assert_eq!(*connecting.lock(), Phase::Transferring);
        let abandoned = phase(Phase::Abandoned);
        assert!(!abandoned.connected());
        assert_eq!(*abandoned.lock(), Phase::Abandoned);
    }

    #[test]
    fn a_helpers_reporter_lets_the_transfer_go_on_only_if_not_abandoned() {
        let (messages, _rx) = mpsc::channel::<HelperMessage<()>>();
        let mut connecting = HelperReporter {
            messages: messages.clone(),
            phase: phase(Phase::Connecting(Instant::now())),
        };
        assert!(connecting.connected());
        assert_eq!(*connecting.phase.lock(), Phase::Transferring);
        let mut abandoned = HelperReporter {
            messages,
            phase: phase(Phase::Abandoned),
        };
        assert!(!abandoned.connected());
    }

    #[test]
    fn a_helper_stops_when_abandoned_or_cancelled() {
        let transferring = phase(Phase::Transferring);
        assert!(!helper_cancelled(&transferring, &|| false));
        assert!(helper_cancelled(&transferring, &|| true));
        assert!(helper_cancelled(&phase(Phase::Abandoned), &|| false));
    }

    #[test]
    fn the_worker_gives_up_on_a_connecting_transfer_when_the_user_cancels() {
        let connecting = phase(Phase::Connecting(Instant::now()));
        assert!(give_up(&connecting, &|| false, None).is_none());
        assert!(matches!(*connecting.lock(), Phase::Connecting(_)));
        assert!(matches!(
            give_up(&connecting, &|| true, None),
            Some(RemoteError::Cancelled)
        ));
        assert_eq!(*connecting.lock(), Phase::Abandoned);
    }

    #[test]
    fn the_worker_gives_up_on_a_transfer_connecting_past_its_limit() {
        let started = Instant::now() - Duration::from_secs(10);
        let connecting = phase(Phase::Connecting(started));
        assert!(give_up(&connecting, &|| false, Some(Duration::from_secs(60))).is_none());
        assert!(give_up(&connecting, &|| false, None).is_none());
        assert!(matches!(
            give_up(&connecting, &|| false, Some(Duration::from_secs(5))),
            Some(RemoteError::Unreachable(_))
        ));
        assert_eq!(*connecting.lock(), Phase::Abandoned);
    }

    #[test]
    fn the_worker_never_gives_up_on_a_transfer_that_connected() {
        let transferring = phase(Phase::Transferring);
        assert!(give_up(&transferring, &|| true, Some(Duration::ZERO)).is_none());
        assert_eq!(*transferring.lock(), Phase::Transferring);
    }

    /// A host whose "reads" are numbers on a channel; serving one acknowledges it.
    struct FakeHost {
        repo: git2::Repository,
        reads: mpsc::Receiver<u32>,
        served: Sender<u32>,
    }

    impl TransferHost for FakeHost {
        fn repo(&mut self) -> &mut git2::Repository {
            &mut self.repo
        }

        fn serve_read(&mut self) -> bool {
            match self.reads.try_recv() {
                Ok(read) => {
                    let _ = self.served.send(read);
                    true
                }
                Err(_) => false,
            }
        }
    }

    struct NoStore;

    impl crate::credentials::CredentialStore for NoStore {
        fn get(
            &self,
            _key: &crate::credentials::CredentialKey,
        ) -> Result<Option<String>, crate::credentials::CredentialStoreError> {
            Ok(None)
        }

        fn set(
            &self,
            _key: &crate::credentials::CredentialKey,
            _token: &str,
        ) -> Result<(), crate::credentials::CredentialStoreError> {
            Ok(())
        }

        fn delete(
            &self,
            _key: &crate::credentials::CredentialKey,
        ) -> Result<(), crate::credentials::CredentialStoreError> {
            Ok(())
        }
    }

    /// #734: a transfer blocked until the test lets it go; reads sent meanwhile are served
    /// while it is still blocked, and `run` returns its result once released.
    #[test]
    fn the_worker_serves_reads_while_a_transfer_is_blocked() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let repo = git2::Repository::init(dir.path()).expect("init");
        repo.remote("origin", "https://example.invalid/repo.git")
            .expect("remote");
        let (read_tx, reads) = mpsc::channel();
        let (served, served_rx) = mpsc::channel();
        let (release_tx, release) = mpsc::channel::<()>();
        let registry = crate::worker::CancelRegistry::default();
        let worker = std::thread::spawn(move || {
            let mut host = FakeHost {
                repo,
                reads,
                served,
            };
            let op = super::super::remote::TransferOp::new(registry.allocate("fetch"));
            let (events, _events_rx) = mpsc::channel();
            let credentials = Arc::new(CredentialService::new(NoStore));
            let wait = Wait {
                op: &op,
                events: &events,
                connect_limit: None,
                prompt: false,
            };
            run(
                &mut host,
                &credentials,
                "origin",
                wait,
                move |_repo, _provider, reporter, _cancel| {
                    assert!(reporter.connected());
                    release.recv().expect("released");
                    Ok(42)
                },
            )
        });

        for read in 1..=3 {
            read_tx.send(read).expect("send read");
            assert_eq!(
                served_rx
                    .recv_timeout(Duration::from_secs(20))
                    .expect("served while the transfer is blocked"),
                read
            );
        }
        release_tx.send(()).expect("release");
        assert_eq!(worker.join().expect("worker").expect("transfer"), 42);
    }
}
