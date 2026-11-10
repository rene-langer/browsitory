use std::sync::mpsc::Sender;

use git_core::remote::{PullOutcome, RemoteInfo, TransferOperation, UpstreamInfo};

use super::{Command, TransferEvent, WorkerHandle};
use crate::credentials::{CredentialService, CredentialStore, CredentialStoreError};

struct ChannelReporter {
    events: Sender<TransferEvent>,
    operation_id: String,
}

impl git_core::remote::TransferReporter for ChannelReporter {
    fn report(&mut self, progress: git_core::remote::TransferProgress) {
        let _ = self.events.send(TransferEvent::Progress(
            git_core::remote::TransferProgress {
                operation_id: self.operation_id.clone(),
                ..progress
            },
        ));
    }
}

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<RemoteInfo>, String>>) {
    let _ = reply.send(git_core::remote::list_remotes(repo).map_err(|error| error.to_string()));
}

pub(super) fn list_branches(
    repo: &git2::Repository,
    remote_name: String,
    reply: Sender<Result<Vec<String>, String>>,
) {
    let _ = reply.send(
        git_core::remote::list_remote_branches(repo, &remote_name)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn current_upstream(
    repo: &git2::Repository,
    reply: Sender<Result<Option<UpstreamInfo>, String>>,
) {
    let _ = reply.send(git_core::remote::current_upstream(repo).map_err(|error| error.to_string()));
}

pub(super) fn add(
    repo: &git2::Repository,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::add_remote(repo, &name, &fetch_url, push_url.as_deref())
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn rename(
    repo: &git2::Repository,
    old_name: String,
    new_name: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::rename_remote(repo, &old_name, &new_name)
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn update_urls(
    repo: &git2::Repository,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::update_remote_urls(repo, &name, &fetch_url, push_url.as_deref())
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn upstreams(
    repo: &git2::Repository,
    name: String,
    reply: Sender<Result<Vec<UpstreamInfo>, String>>,
) {
    let _ = reply
        .send(git_core::remote::remote_upstreams(repo, &name).map_err(|error| error.to_string()));
}
pub(super) fn auth_mode(
    repo: &git2::Repository,
    name: String,
    reply: Sender<Result<Option<git_core::remote::RemoteAuthMode>, String>>,
) {
    let _ = reply.send(
        git_core::remote::remote_auth_profile(repo, &name)
            .map_err(|_| "could not read remote authentication settings".to_string()),
    );
}
pub(super) fn save_https<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    username: String,
    token: String,
    reply: Sender<Result<(), String>>,
) {
    let result = git_core::remote::list_remotes(repo)
        .map_err(|_| "could not find remote".to_string())
        .and_then(|remotes| {
            remotes
                .into_iter()
                .find(|remote| remote.name == remote_name)
                .ok_or_else(|| "could not find remote".to_string())
        })
        .and_then(|remote| {
            credentials
                .save_https(&remote.fetch_url, &username, &token)
                .map_err(credential_operation_error)
        });
    let _ = reply.send(result);
}
pub(super) fn forget_https<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    reply: Sender<Result<(), String>>,
) {
    let result = (|| {
        let profile = git_core::remote::remote_auth_profile(repo, &remote_name)
            .map_err(|_| "could not read remote authentication settings".to_string())?;
        let Some(git_core::remote::RemoteAuthMode::HttpsToken { username }) = profile else {
            return Ok(());
        };
        let remote = git_core::remote::list_remotes(repo)
            .map_err(|_| "could not find remote".to_string())?
            .into_iter()
            .find(|remote| remote.name == remote_name)
            .ok_or_else(|| "could not find remote".to_string())?;
        credentials
            .forget_https(&remote.fetch_url, &username)
            .map_err(credential_operation_error)
    })();
    let _ = reply.send(result);
}

fn credential_operation_error(error: CredentialStoreError) -> String {
    match error {
        CredentialStoreError::Keychain(diagnostic) => {
            crate::wire::credential_store_failure_message(&diagnostic)
        }
        CredentialStoreError::InvalidHttpsUrl => error.to_string(),
    }
}
pub(super) fn set_auth_mode(
    repo: &git2::Repository,
    remote_name: String,
    mode: git_core::remote::RemoteAuthMode,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::set_remote_auth_profile(repo, &remote_name, mode)
            .map_err(|_| "could not configure remote authentication".to_string()),
    );
}

pub(super) fn remove(
    repo: &git2::Repository,
    name: String,
    clear_upstreams: bool,
    reply: Sender<Result<(), String>>,
) {
    let result = if clear_upstreams {
        git_core::remote::remove_remote_and_clear_upstreams(repo, &name)
    } else {
        git_core::remote::remove_remote(repo, &name)
    }
    .map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn set_current_upstream(
    repo: &git2::Repository,
    remote_name: String,
    remote_branch: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::set_current_upstream(repo, &remote_name, &remote_branch)
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn clear_current_upstream(repo: &git2::Repository, reply: Sender<Result<(), String>>) {
    let _ = reply
        .send(git_core::remote::clear_current_upstream(repo).map_err(|error| error.to_string()));
}

/// The per-operation state every cancellable operation carries: the ID the UI tracks the
/// operation by, and its registration in the shared registry a cancel request for that ID lands
/// in. Dropping it (with the command's other parts) ends the registration.
pub(super) struct TransferOp {
    pub(super) id: String,
    registered: super::cancel::OperationId,
}

impl TransferOp {
    pub(super) fn new(registered: super::cancel::OperationId) -> Self {
        Self {
            id: registered.as_str().to_string(),
            registered,
        }
    }

    /// Builds the closure `git-core` polls from inside its `git2` progress callbacks.
    ///
    /// Owns a clone of the shared registry and the operation's own ID, so the check stays valid
    /// for the whole (blocking) transfer without borrowing anything from the command loop.
    pub(super) fn cancel_check(&self) -> impl Fn() -> bool {
        let cancelled = self.registered.registry().clone();
        let operation_id = self.id.clone();
        move || cancelled.is_cancelled(&operation_id)
    }

    /// Forgets a cancel asked for so far, so a later step (such as a post hook) only honours a
    /// cancel asked for while it runs. The operation stays cancellable until dropped.
    pub(super) fn clear(&self) {
        self.registered.registry().reset(&self.id);
    }

    /// Ends the operation's registration and returns its ID, for its `Completed` event. Called
    /// before that event is sent, so whoever sees it (the "Fetch automatically" scheduler starting
    /// its next tick, a test) finds the worker idle (#567); a cancel for the ID is dropped from
    /// then on, as for any finished operation.
    pub(super) fn finish(self) -> String {
        let Self { id, registered } = self;
        drop(registered);
        id
    }
}

fn complete(
    events: &Sender<TransferEvent>,
    op: TransferOp,
    operation: TransferOperation,
    result: Result<(), git_core::remote::RemoteError>,
) {
    let hook_output = match &result {
        Err(git_core::remote::RemoteError::HookFailed { output, .. }) => Some(output.clone()),
        Err(git_core::remote::RemoteError::Hook(
            error @ git_core::hooks::HookError::Spawn { .. },
        )) => Some(git_core::hooks::spawn_failure_output(error)),
        _ => None,
    };
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.finish(),
        operation,
        error: result.err().map(|error| error.transfer_error_kind()),
        hook_output,
    });
}
#[allow(clippy::too_many_arguments)]
pub(super) fn fetch<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    background: &super::background_fetch::Memory,
    remote_name: String,
    prune: Option<bool>,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::Fetch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: op.id.clone(),
    };
    let cancel = op.cancel_check();
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::fetch_remote(
            repo,
            &remote_name,
            prune,
            op.id.clone(),
            &mut provider,
            &mut reporter,
            &cancel,
        )
    });
    if result.is_ok() {
        background.reachable(&remote_name);
    }
    complete(&events, op, TransferOperation::Fetch, result);
}
pub(super) fn pull<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<PullOutcome, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::Pull,
    });
    let cancel = op.cancel_check();
    let result = (|| -> Result<PullOutcome, git_core::remote::RemoteError> {
        let upstream = git_core::remote::current_upstream(repo)?
            .ok_or(git_core::remote::RemoteError::NoUpstream)?;
        let mut reporter = ChannelReporter {
            events: events.clone(),
            operation_id: op.id.clone(),
        };
        let profile = git_core::remote::remote_auth_profile(repo, &upstream.remote_name)?;
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::fetch_remote(
            repo,
            &upstream.remote_name,
            None,
            op.id.clone(),
            &mut provider,
            &mut reporter,
            &cancel,
        )?;
        let upstream = git_core::remote::current_upstream(repo)?
            .ok_or(git_core::remote::RemoteError::NoUpstream)?;
        git_core::remote::pull_after_fetch(repo, &upstream.remote_name, &upstream.remote_branch)
    })();
    // git runs post-merge after a pull that fast-forwarded. It runs on the pull's own operation:
    // the transfer panel shows it (`RunningHook`), and its Cancel stops only the hook; the pull's
    // outcome stands (#185). A Cancel pressed earlier (during the fetch) that came too late to
    // stop the pull is dropped first, so post-merge only honours a Cancel pressed while it runs.
    if matches!(result, Ok(PullOutcome::FastForwarded { .. })) {
        op.clear();
        let installed = matches!(
            git_core::hooks::find_hook(repo, "post-merge"),
            Ok(git_core::hooks::HookLookup::Found(_))
        );
        if installed {
            let _ = events.send(TransferEvent::Progress(
                git_core::remote::TransferProgress {
                    operation_id: op.id.clone(),
                    operation: TransferOperation::Pull,
                    phase: git_core::remote::TransferPhase::RunningHook,
                    current: 0,
                    total: 0,
                    received_bytes: 0,
                    message: None,
                },
            ));
        }
        super::hooks::run_post_hook_reported(
            repo,
            "post-merge",
            &["0"],
            &mut git_core::hooks::HookOptions {
                cancel: Some(&cancel),
                ..Default::default()
            },
        );
    }
    let error = result
        .as_ref()
        .err()
        .map(|error| error.transfer_error_kind());
    let _ = reply.send(result.map_err(pull_error_message));
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.finish(),
        operation: TransferOperation::Pull,
        error,
        hook_output: None,
    });
}

/// What a failed pull says: git-core's own text for the refusals and errors whose text is
/// static or names only a path in the work tree, "pull failed" for anything else (a transport
/// error may quote a URL with credentials).
fn pull_error_message(error: git_core::remote::RemoteError) -> String {
    match error {
        git_core::remote::RemoteError::DirtyWorktree
        | git_core::remote::RemoteError::NoUpstream
        | git_core::remote::RemoteError::CheckoutConflict
        | git_core::remote::RemoteError::LinkedFolder(_)
        | git_core::remote::RemoteError::CaseCollision(_)
        // Names only the setting and libgit2's parse error (#225).
        | git_core::remote::RemoteError::CaseConfig(_)
        | git_core::remote::RemoteError::DetachedHead
        // A file another program holds open; the pull was rolled back (#321).
        | git_core::remote::RemoteError::HeldOpen(_)
        // Static, secret-free text: the user asked for this by pressing Cancel.
        | git_core::remote::RemoteError::Cancelled => error.to_string(),
        _ => crate::wire::PULL_FAILED.to_string(),
    }
}

/// Refuses a current-branch push before any transfer starts when `push.default` does not settle
/// its target (the frontend then offers "Push to…"); the reply carries git-core's static,
/// secret-free message. Other resolution errors are left to the transfer itself to report.
fn unresolved_push_target(repo: &git2::Repository, remote_name: &str) -> Option<String> {
    match git_core::remote::current_push_target(repo, remote_name) {
        Ok(git_core::remote::PushTarget::NeedsChoice {
            remote,
            local_branch,
        }) => Some(
            git_core::remote::RemoteError::PushTargetNeedsChoice {
                remote,
                local_branch,
            }
            .to_string(),
        ),
        _ => None,
    }
}
pub(super) fn push_current<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    if let Some(message) = unresolved_push_target(repo, &remote_name) {
        op.clear();
        let _ = reply.send(Err(message));
        return;
    }
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::PushBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: op.id.clone(),
    };
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let cancel = op.cancel_check();
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::push_current_branch(
            repo,
            &remote_name,
            no_verify,
            &mut provider,
            &mut reporter,
            &cancel,
        )
    });
    complete(&events, op, TransferOperation::PushBranch, result);
}
pub(super) fn force_push_branch<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    if let Some(message) = unresolved_push_target(repo, &remote_name) {
        op.clear();
        let _ = reply.send(Err(message));
        return;
    }
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::ForcePushBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: op.id.clone(),
    };
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let cancel = op.cancel_check();
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::force_push_current_branch(
            repo,
            &remote_name,
            no_verify,
            &mut provider,
            &mut reporter,
            &cancel,
        )
    });
    complete(&events, op, TransferOperation::ForcePushBranch, result);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn push_tags<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    names: Vec<String>,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::PushTags,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: op.id.clone(),
    };
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let cancel = op.cancel_check();
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::push_tags(
            repo,
            &remote_name,
            &names,
            no_verify,
            &mut provider,
            &mut reporter,
            &cancel,
        )
    });
    complete(&events, op, TransferOperation::PushTags, result);
}

/// What a `PushBranch` command pushes, grouped to keep the handler at a readable arity.
pub(super) struct PushTarget {
    pub(super) remote_name: String,
    pub(super) local_branch: String,
    pub(super) remote_branch: String,
    pub(super) set_upstream: bool,
    pub(super) no_verify: bool,
}

pub(super) fn fetch_all<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    background: &super::background_fetch::Memory,
    prune: Option<bool>,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    const OPERATION: TransferOperation = TransferOperation::FetchAll;
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: OPERATION,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: op.id.clone(),
    };
    let cancel = op.cancel_check();
    let result = git_core::remote::fetch_all(
        repo,
        prune,
        op.id.clone(),
        &mut |remote_name| {
            let profile = git_core::remote::remote_auth_profile(repo, remote_name)?;
            let provider: git_core::remote::BoxedCredentialProvider<'_> = Box::new(
                crate::credentials::RemoteCredentialProvider::new(credentials, profile),
            );
            Ok(provider)
        },
        &mut reporter,
        &cancel,
    );
    let error = match &result {
        Ok(outcome) => {
            for remote_name in &outcome.fetched {
                background.reachable(remote_name);
            }
            for (remote_name, kind) in &outcome.failed {
                let _ = events.send(TransferEvent::RemoteFailed {
                    operation_id: op.id.clone(),
                    operation: OPERATION,
                    remote_name: remote_name.clone(),
                    error: *kind,
                });
            }
            // Only a Fetch all where nothing succeeded fails as a whole.
            if outcome.fetched.is_empty() {
                outcome.failed.first().map(|(_, kind)| *kind)
            } else {
                None
            }
        }
        Err(error) => Some(error.transfer_error_kind()),
    };
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.finish(),
        operation: OPERATION,
        error,
        hook_output: None,
    });
}
pub(super) fn push_branch<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    target: PushTarget,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::PushBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: op.id.clone(),
    };
    super::hooks::log_skipped_pre_push(repo, target.no_verify);
    let cancel = op.cancel_check();
    let result =
        git_core::remote::remote_auth_profile(repo, &target.remote_name).and_then(|profile| {
            let mut provider =
                crate::credentials::RemoteCredentialProvider::new(credentials, profile);
            git_core::remote::push_branch(
                repo,
                &target.remote_name,
                &target.local_branch,
                &target.remote_branch,
                target.set_upstream,
                target.no_verify,
                &mut provider,
                &mut reporter,
                &cancel,
            )
        });
    complete(&events, op, TransferOperation::PushBranch, result);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn delete_remote_branch<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    branch_name: String,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::DeleteRemoteBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: op.id.clone(),
    };
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let cancel = op.cancel_check();
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::delete_remote_branch(
            repo,
            &remote_name,
            &branch_name,
            no_verify,
            &mut provider,
            &mut reporter,
            &cancel,
        )
    });
    complete(&events, op, TransferOperation::DeleteRemoteBranch, result);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn delete_remote_tag<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    tag_name: String,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::DeleteRemoteTag,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: op.id.clone(),
    };
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let cancel = op.cancel_check();
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::delete_remote_tag(
            repo,
            &remote_name,
            &tag_name,
            no_verify,
            &mut provider,
            &mut reporter,
            &cancel,
        )
    });
    complete(&events, op, TransferOperation::DeleteRemoteTag, result);
}

impl WorkerHandle {
    pub fn fetch_remote(
        &self,
        remote_name: String,
        prune: Option<bool>,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("fetch");
        self.tx
            .send(Command::FetchRemote {
                remote_name,
                prune,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn fetch_all_remotes(
        &self,
        prune: Option<bool>,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("fetch-all");
        self.tx
            .send(Command::FetchAllRemotes {
                prune,
                background: None,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    /// Starts a quiet Fetch all for the "Fetch automatically" setting, every `interval_minutes`,
    /// reported as `TransferOperation::BackgroundFetch`. Returns `None`, without queueing
    /// anything, while any cancellable operation of this worker is queued or running (a fetch,
    /// pull or push, a commit, merge, checkout and the rest that `CancelRegistry` tracks): a
    /// background fetch never overlaps a manual fetch, another background fetch or a mutation,
    /// whichever tab or spelling of the repository started it. Like every fetch it records no
    /// undo entry. A remote still connecting never holds the worker past a cancel, and a remote
    /// that can't be reached backs off (see `background_fetch`).
    pub fn background_fetch(
        &self,
        prune: Option<bool>,
        interval_minutes: u32,
        events: Sender<TransferEvent>,
    ) -> Result<Option<String>, String> {
        self.background_fetch_with(
            super::background_fetch::BackgroundFetchOptions::new(prune, interval_minutes),
            events,
        )
    }

    pub(super) fn background_fetch_with(
        &self,
        options: super::background_fetch::BackgroundFetchOptions,
        events: Sender<TransferEvent>,
    ) -> Result<Option<String>, String> {
        let Some(operation_id) = self.cancelled.allocate_if_idle("background-fetch") else {
            return Ok(None);
        };
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::FetchAllRemotes {
                prune: options.prune,
                background: Some(options),
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
            .map(Some)
    }
    pub fn push_branch(
        &self,
        remote_name: String,
        local_branch: String,
        remote_branch: String,
        set_upstream: bool,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("push");
        self.tx
            .send(Command::PushBranch {
                remote_name,
                local_branch,
                remote_branch,
                set_upstream,
                no_verify,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn delete_remote_branch(
        &self,
        remote_name: String,
        branch_name: String,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("delete-remote");
        self.tx
            .send(Command::DeleteRemoteBranch {
                remote_name,
                branch_name,
                no_verify,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn delete_remote_tag(
        &self,
        remote_name: String,
        tag_name: String,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("delete-remote");
        self.tx
            .send(Command::DeleteRemoteTag {
                remote_name,
                tag_name,
                no_verify,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn pull_current_upstream(
        &self,
        events: Sender<TransferEvent>,
    ) -> Result<PullOutcome, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("pull");
        self.tx
            .send(Command::PullCurrentUpstream {
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn push_current_branch(
        &self,
        remote_name: String,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("push");
        self.tx
            .send(Command::PushCurrentBranch {
                remote_name,
                no_verify,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn force_push_current_branch(
        &self,
        remote_name: String,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("force-push");
        self.tx
            .send(Command::ForcePushCurrentBranch {
                remote_name,
                no_verify,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn push_tags(
        &self,
        remote_name: String,
        names: Vec<String>,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = self.cancelled.allocate("push");
        self.tx
            .send(Command::PushTags {
                remote_name,
                names,
                no_verify,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn list_remotes(&self) -> Result<Vec<RemoteInfo>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListRemotes { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn list_remote_branches(&self, remote_name: String) -> Result<Vec<String>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListRemoteBranches { remote_name, reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn get_current_upstream(&self) -> Result<Option<UpstreamInfo>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCurrentUpstream { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn get_remote_upstreams(&self, name: String) -> Result<Vec<UpstreamInfo>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetRemoteUpstreams { name, reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn get_remote_auth_mode(
        &self,
        name: String,
    ) -> Result<Option<git_core::remote::RemoteAuthMode>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetRemoteAuthMode { name, reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn save_https_credential(
        &self,
        remote_name: String,
        username: String,
        token: String,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SaveHttpsCredential {
                remote_name,
                username,
                token,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn forget_https_credential(&self, remote_name: String) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ForgetHttpsCredential { remote_name, reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn set_remote_auth_mode(
        &self,
        remote_name: String,
        mode: git_core::remote::RemoteAuthMode,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SetRemoteAuthMode {
                remote_name,
                mode,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn add_remote(
        &self,
        name: String,
        fetch_url: String,
        push_url: Option<String>,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::AddRemote {
                name,
                fetch_url,
                push_url,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn rename_remote(&self, old_name: String, new_name: String) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::RenameRemote {
                old_name,
                new_name,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn update_remote_urls(
        &self,
        name: String,
        fetch_url: String,
        push_url: Option<String>,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::UpdateRemoteUrls {
                name,
                fetch_url,
                push_url,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn remove_remote(&self, name: String, clear_upstreams: bool) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::RemoveRemote {
                name,
                clear_upstreams,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn set_current_upstream(
        &self,
        remote_name: String,
        remote_branch: String,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SetCurrentUpstream {
                remote_name,
                remote_branch,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn clear_current_upstream(&self) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ClearCurrentUpstream { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::{credential_operation_error, pull_error_message};
    use crate::credentials::CredentialStoreError;

    /// A pull stopped at a file another program holds open names it (#321), not "pull failed".
    #[test]
    fn a_pull_stopped_at_a_held_file_says_so() {
        let error = git_core::remote::RemoteError::HeldOpen(git_core::HeldOpen {
            path: "d/f.txt".to_string(),
            during: git_core::HeldDuring::RolledBack,
        });

        assert_eq!(
            pull_error_message(error),
            "'d/f.txt' is open in another program. Close it, then try again."
        );
    }

    /// A pull stopped at an unreadable `core.ignorecase` says so (#225), not "pull failed".
    #[test]
    fn a_pull_stopped_at_an_unreadable_ignorecase_says_so() {
        let error = git_core::remote::RemoteError::CaseConfig(git_core::CaseConfigUnreadable(
            "failed to parse 'maybe' as a boolean".to_string(),
        ));

        assert_eq!(
            pull_error_message(error),
            "Can't read core.ignorecase from the Git config: failed to parse 'maybe' as a \
             boolean. Set it to true or false (or remove it), then try again."
        );
    }

    #[test]
    fn credential_operation_error_keeps_the_backend_diagnostic() {
        let error =
            CredentialStoreError::Keychain("CredWrite failed with Windows error 1312".to_owned());

        let message = credential_operation_error(error);

        assert_eq!(
            message,
            "credential keychain failure: CredWrite failed with Windows error 1312"
        );
        assert_eq!(
            crate::wire::ErrorKind::of(&message),
            crate::wire::ErrorKind::CredentialStoreFailure
        );
    }

    /// #496: a cancelled pull answers `Cancelled`, so the frontend leaves it to the pull's
    /// terminal transfer event ("Pull cancelled.").
    #[test]
    fn a_cancelled_pull_answers_the_cancelled_kind() {
        let message = pull_error_message(git_core::remote::RemoteError::Cancelled);

        assert_eq!(
            crate::wire::ErrorKind::of(&message),
            crate::wire::ErrorKind::Cancelled
        );
    }
}
