//! Fetch all: the one the user asks for, and the "Fetch automatically" tick (#440), which is the
//! same fetch reported as `BackgroundFetch`, with a connect limit and a backoff per remote
//! (`background_fetch`). Each remote is fetched on a helper thread the worker gives up on while
//! it connects (`transfer_thread`, #462/#547), so a cancel ends the whole Fetch all at once.
use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

use git_core::remote::{RemoteError, TransferErrorKind, TransferOperation};

use super::background_fetch::{BackgroundFetchOptions, Memory};
use super::remote::TransferOp;
use super::transfer_thread::{self, TransferHost, Wait};
use super::TransferEvent;
use crate::credentials::{CredentialService, CredentialStore};

/// The operation log's note for a remote a background fetch skipped while it backs off.
const SKIPPED_WHILE_UNREACHABLE: &str =
    "Skipped: the remote could not be reached on an earlier try.";

/// Which Fetch all runs.
#[derive(Debug, Clone, Copy)]
pub(super) enum Mode {
    /// The user's Fetch all.
    Manual { prune: Option<bool> },
    /// The "Fetch automatically" tick.
    Background(BackgroundFetchOptions),
}

impl Mode {
    fn operation(self) -> TransferOperation {
        match self {
            Self::Manual { .. } => TransferOperation::FetchAll,
            Self::Background(_) => TransferOperation::BackgroundFetch,
        }
    }
}

/// Fetches every remote in `repo.remotes()` order under one operation. A remote that fails is
/// reported as `RemoteFailed` and the next one is tried; only a Fetch all where nothing
/// succeeded fails as a whole. A cancel stops it as `Cancelled` (remotes fetched so far stay
/// fetched). Any remote that answered ends its backoff; a background fetch also starts or
/// lengthens the backoff of a remote it could not reach, and skips a remote backing off.
pub(super) fn run<S: CredentialStore + Send + Sync + 'static>(
    host: &mut dyn TransferHost,
    credentials: &Arc<CredentialService<S>>,
    memory: &Memory,
    mode: Mode,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    let operation = mode.operation();
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let tick = Instant::now();
    let cancel = op.cancel_check();
    let names: Result<Vec<String>, git2::Error> = host.repo().remotes().map(|names| {
        names
            .iter()
            .flatten()
            .flatten()
            .map(str::to_string)
            .collect()
    });
    let mut fetched_any = false;
    // Each failed remote, its error kind, and what went wrong.
    let mut failed: Vec<(String, TransferErrorKind, String)> = Vec::new();
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
                if matches!(mode, Mode::Background(_)) && !memory.may_try(&name, tick) {
                    failed.push((
                        name,
                        TransferErrorKind::TransferFailed,
                        SKIPPED_WHILE_UNREACHABLE.to_string(),
                    ));
                    continue;
                }
                match fetch_one(host, credentials, memory, &name, mode, &op, &events) {
                    Ok(()) => {
                        memory.reachable(&name);
                        fetched_any = true;
                    }
                    Err(RemoteError::Cancelled) => {
                        cancelled = true;
                        break;
                    }
                    Err(remote_error) => {
                        remember_failure(memory, mode, &name, &remote_error, tick);
                        failed.push((
                            name,
                            remote_error.transfer_error_kind(),
                            remote_error.to_string(),
                        ));
                    }
                }
            }
        }
    }
    if cancelled {
        error = Some(TransferErrorKind::Cancelled);
    } else if error.is_none() {
        for (remote_name, kind, _) in &failed {
            let _ = events.send(TransferEvent::RemoteFailed {
                operation_id: op.id.clone(),
                operation,
                remote_name: remote_name.clone(),
                error: *kind,
            });
        }
        if !fetched_any {
            error = failed.first().map(|(_, kind, _)| *kind);
        }
    }
    super::journal::note_transfer(super::journal::TransferNote {
        error,
        message: if fetched_any {
            None
        } else {
            failed.first().map(|(_, _, message)| message.clone())
        },
        remote_failures: failed
            .into_iter()
            .map(
                |(remote_name, error, message)| crate::oplog::RemoteFailure {
                    remote_name,
                    error,
                    message,
                },
            )
            .collect(),
        ..Default::default()
    });
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.finish(),
        operation,
        error,
        hook_output: None,
    });
}

/// What a failed fetch of `remote_name` says about reaching it: a remote that answered (the
/// fetch failed later, e.g. on credentials) ends its backoff; one that could not be reached
/// starts or lengthens it, but only for a background fetch.
fn remember_failure(
    memory: &Memory,
    mode: Mode,
    remote_name: &str,
    error: &RemoteError,
    tick: Instant,
) {
    if !matches!(error, RemoteError::Unreachable(_)) {
        memory.reachable(remote_name);
    } else if let Mode::Background(options) = mode {
        memory.unreachable(remote_name, tick, options.interval);
    }
}

/// Fetches one remote on a helper thread and waits for it (`transfer_thread::run`).
fn fetch_one<S: CredentialStore + Send + Sync + 'static>(
    host: &mut dyn TransferHost,
    credentials: &Arc<CredentialService<S>>,
    memory: &Memory,
    remote_name: &str,
    mode: Mode,
    op: &TransferOp,
    events: &Sender<TransferEvent>,
) -> Result<(), RemoteError> {
    let operation = mode.operation();
    let operation_id = op.id.clone();
    let name = remote_name.to_string();
    let (prune, connect_limit, connecting) = match mode {
        Mode::Manual { prune } => (prune, None, None),
        // While its helper lives, later ticks skip the remote (`Memory::may_try`).
        Mode::Background(options) => (
            options.prune,
            Some(options.connect_limit),
            Some(memory.connecting(remote_name)),
        ),
    };
    let wait = Wait {
        op,
        events,
        connect_limit,
        prompt: matches!(mode, Mode::Manual { .. }),
    };
    transfer_thread::run(
        host,
        credentials,
        remote_name,
        wait,
        move |repo, provider, reporter, cancel| {
            let _connecting = connecting;
            git_core::remote::fetch_remote_as(
                operation,
                repo,
                &name,
                prune,
                &operation_id,
                provider,
                reporter,
                cancel,
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn background() -> Mode {
        Mode::Background(BackgroundFetchOptions::new(None, 5))
    }

    fn unreachable() -> RemoteError {
        RemoteError::Unreachable(git2::Error::from_str("failed to connect to example.com"))
    }

    #[test]
    fn a_background_fetch_backs_off_a_remote_it_could_not_reach() {
        let memory = Memory::default();
        let tick = Instant::now();
        remember_failure(&memory, background(), "origin", &unreachable(), tick);
        assert!(!memory.may_try("origin", tick + Duration::from_secs(60)));
    }

    #[test]
    fn a_manual_fetch_all_starts_no_backoff() {
        let memory = Memory::default();
        let tick = Instant::now();
        remember_failure(
            &memory,
            Mode::Manual { prune: None },
            "origin",
            &unreachable(),
            tick,
        );
        assert!(memory.may_try("origin", tick + Duration::from_secs(60)));
    }

    #[test]
    fn a_remote_that_answered_but_failed_ends_its_backoff() {
        let memory = Memory::default();
        let tick = Instant::now();
        memory.unreachable("origin", tick, Duration::from_secs(300));
        let answered = RemoteError::Git(git2::Error::from_str("authentication failed"));
        remember_failure(&memory, background(), "origin", &answered, tick);
        assert!(memory.may_try("origin", tick + Duration::from_secs(60)));
    }
}
