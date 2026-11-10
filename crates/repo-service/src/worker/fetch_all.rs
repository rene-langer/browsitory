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
use super::transfer_thread::{self, Wait};
use super::TransferEvent;
use crate::credentials::{CredentialService, CredentialStore};

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
    repo: &git2::Repository,
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
                if matches!(mode, Mode::Background(_)) && !memory.may_try(&name, tick) {
                    failed.push((name, TransferErrorKind::TransferFailed));
                    continue;
                }
                match fetch_one(repo, credentials, memory, &name, mode, &op, &events) {
                    Ok(()) => {
                        memory.reachable(&name);
                        fetched_any = true;
                    }
                    Err(RemoteError::Cancelled) => {
                        cancelled = true;
                        break;
                    }
                    Err(remote_error) => {
                        if !matches!(remote_error, RemoteError::Unreachable(_)) {
                            memory.reachable(&name);
                        } else if let Mode::Background(options) = mode {
                            memory.unreachable(&name, tick, options.interval);
                        }
                        failed.push((name, remote_error.transfer_error_kind()));
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
                operation,
                remote_name: remote_name.clone(),
                error: *kind,
            });
        }
        if !fetched_any {
            error = failed.first().map(|(_, kind)| *kind);
        }
    }
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.finish(),
        operation,
        error,
        hook_output: None,
    });
}

/// Fetches one remote on a helper thread and waits for it (`transfer_thread::run`).
fn fetch_one<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
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
        repo,
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
