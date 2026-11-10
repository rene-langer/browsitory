use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;
use std::sync::Arc;

use git_core::remote::{
    CredentialProvider, PullOutcome, RemoteError, RemoteInfo, TransferOperation, TransferReporter,
    UpstreamInfo,
};

use super::transfer_thread::{self, Wait};
use super::{Command, TransferEvent, WorkerHandle};
use crate::credentials::{CredentialService, CredentialStore};

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<RemoteInfo>, ErrorDto>>) {
    let _ = reply.send(git_core::remote::list_remotes(repo).map_err(ErrorDto::from_error));
}

pub(super) fn list_branches(
    repo: &git2::Repository,
    remote_name: String,
    reply: Sender<Result<Vec<String>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::remote::list_remote_branches(repo, &remote_name).map_err(ErrorDto::from_error),
    );
}

pub(super) fn current_upstream(
    repo: &git2::Repository,
    reply: Sender<Result<Option<UpstreamInfo>, ErrorDto>>,
) {
    let _ = reply.send(git_core::remote::current_upstream(repo).map_err(ErrorDto::from_error));
}

pub(super) fn add(
    repo: &git2::Repository,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::remote::add_remote(repo, &name, &fetch_url, push_url.as_deref())
            .map_err(ErrorDto::from_error),
    );
}
pub(super) fn rename(
    repo: &git2::Repository,
    old_name: String,
    new_name: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::remote::rename_remote(repo, &old_name, &new_name).map_err(ErrorDto::from_error),
    );
}
pub(super) fn update_urls(
    repo: &git2::Repository,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::remote::update_remote_urls(repo, &name, &fetch_url, push_url.as_deref())
            .map_err(ErrorDto::from_error),
    );
}
pub(super) fn upstreams(
    repo: &git2::Repository,
    name: String,
    reply: Sender<Result<Vec<UpstreamInfo>, ErrorDto>>,
) {
    let _ =
        reply.send(git_core::remote::remote_upstreams(repo, &name).map_err(ErrorDto::from_error));
}
pub(super) fn auth_mode(
    repo: &git2::Repository,
    name: String,
    reply: Sender<Result<Option<git_core::remote::RemoteAuthMode>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::remote::remote_auth_profile(repo, &name)
            .map_err(|_| "could not read remote authentication settings".into()),
    );
}
pub(super) fn save_https<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    username: String,
    token: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let result = git_core::remote::list_remotes(repo)
        .map_err(|_| ErrorDto::from("could not find remote"))
        .and_then(|remotes| {
            remotes
                .into_iter()
                .find(|remote| remote.name == remote_name)
                .ok_or_else(|| "could not find remote".into())
        })
        .and_then(|remote| {
            credentials
                .save_https(&remote.fetch_url, &username, &token)
                .map_err(ErrorDto::from)
        });
    let _ = reply.send(result);
}
pub(super) fn forget_https<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    reply: Sender<Result<(), ErrorDto>>,
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
            .map_err(ErrorDto::from)
    })();
    let _ = reply.send(result);
}

pub(super) fn set_auth_mode(
    repo: &git2::Repository,
    remote_name: String,
    mode: git_core::remote::RemoteAuthMode,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::remote::set_remote_auth_profile(repo, &remote_name, mode)
            .map_err(|_| "could not configure remote authentication".into()),
    );
}

pub(super) fn remove(
    repo: &git2::Repository,
    name: String,
    clear_upstreams: bool,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let result = if clear_upstreams {
        git_core::remote::remove_remote_and_clear_upstreams(repo, &name)
    } else {
        git_core::remote::remove_remote(repo, &name)
    }
    .map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn set_current_upstream(
    repo: &git2::Repository,
    remote_name: String,
    remote_branch: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::remote::set_current_upstream(repo, &remote_name, &remote_branch)
            .map_err(ErrorDto::from_error),
    );
}
pub(super) fn clear_current_upstream(repo: &git2::Repository, reply: Sender<Result<(), ErrorDto>>) {
    let _ =
        reply.send(git_core::remote::clear_current_upstream(repo).map_err(ErrorDto::from_error));
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

/// Runs a transfer the user started against `remote_name` on a helper thread, which the worker
/// gives up on when the user cancels while it is still connecting (`transfer_thread`, #547). Its
/// progress is sent as `op`'s.
fn on_helper<S, T, F>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    remote_name: &str,
    op: &TransferOp,
    events: &Sender<TransferEvent>,
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
    let wait = Wait {
        op,
        events,
        connect_limit: None,
        prompt: true,
    };
    transfer_thread::run(repo, credentials, remote_name, wait, transfer)
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
pub(super) fn fetch<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    background: &super::background_fetch::Memory,
    remote_name: String,
    prune: Option<bool>,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::Fetch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    let name = remote_name.clone();
    let operation_id = op.id.clone();
    let result = on_helper(
        repo,
        credentials,
        &remote_name,
        &op,
        &events,
        move |repo, provider, reporter, cancel| {
            git_core::remote::fetch_remote(
                repo,
                &name,
                prune,
                operation_id,
                provider,
                reporter,
                cancel,
            )
        },
    );
    if result.is_ok() {
        background.reachable(&remote_name);
    }
    complete(&events, op, TransferOperation::Fetch, result);
}
pub(super) fn pull<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<PullOutcome, ErrorDto>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::Pull,
    });
    let cancel = op.cancel_check();
    let result = (|| -> Result<PullOutcome, git_core::remote::RemoteError> {
        let upstream = git_core::remote::current_upstream(repo)?
            .ok_or(git_core::remote::RemoteError::NoUpstream)?;
        // The fetch runs on a helper thread a cancel abandons while it connects (#547); the
        // merge then runs here, on the worker, as before.
        let name = upstream.remote_name.clone();
        let operation_id = op.id.clone();
        on_helper(
            repo,
            credentials,
            &upstream.remote_name,
            &op,
            &events,
            move |repo, provider, reporter, cancel| {
                git_core::remote::fetch_remote(
                    repo,
                    &name,
                    None,
                    operation_id,
                    provider,
                    reporter,
                    cancel,
                )
            },
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
    let _ = reply.send(result.map_err(pull_error));
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.finish(),
        operation: TransferOperation::Pull,
        error,
        hook_output: None,
    });
}

/// What a failed pull says: git-core's own text for the refusals and errors whose text is
/// static or names only a path in the work tree, [`ErrorDto::pull_failed`] for anything else (a
/// transport error may quote a URL with credentials).
fn pull_error(error: git_core::remote::RemoteError) -> ErrorDto {
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
        | git_core::remote::RemoteError::Cancelled => ErrorDto::from_error(error),
        _ => ErrorDto::pull_failed(),
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
pub(super) fn push_current<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    remote_name: String,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    if let Some(message) = unresolved_push_target(repo, &remote_name) {
        op.clear();
        let _ = reply.send(Err(message.into()));
        return;
    }
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::PushBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let name = remote_name.clone();
    let result = on_helper(
        repo,
        credentials,
        &remote_name,
        &op,
        &events,
        move |repo, provider, reporter, cancel| {
            git_core::remote::push_current_branch(
                repo, &name, no_verify, provider, reporter, cancel,
            )
        },
    );
    complete(&events, op, TransferOperation::PushBranch, result);
}
pub(super) fn force_push_branch<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    remote_name: String,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    if let Some(message) = unresolved_push_target(repo, &remote_name) {
        op.clear();
        let _ = reply.send(Err(message.into()));
        return;
    }
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::ForcePushBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let name = remote_name.clone();
    let result = on_helper(
        repo,
        credentials,
        &remote_name,
        &op,
        &events,
        move |repo, provider, reporter, cancel| {
            git_core::remote::force_push_current_branch(
                repo, &name, no_verify, provider, reporter, cancel,
            )
        },
    );
    complete(&events, op, TransferOperation::ForcePushBranch, result);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn push_tags<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    remote_name: String,
    names: Vec<String>,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::PushTags,
    });
    let _ = reply.send(Ok(op.id.clone()));
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let name = remote_name.clone();
    let result = on_helper(
        repo,
        credentials,
        &remote_name,
        &op,
        &events,
        move |repo, provider, reporter, cancel| {
            git_core::remote::push_tags(repo, &name, &names, no_verify, provider, reporter, cancel)
        },
    );
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

pub(super) fn push_branch<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    target: PushTarget,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::PushBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    super::hooks::log_skipped_pre_push(repo, target.no_verify);
    let remote_name = target.remote_name.clone();
    let result = on_helper(
        repo,
        credentials,
        &remote_name,
        &op,
        &events,
        move |repo, provider, reporter, cancel| {
            git_core::remote::push_branch(
                repo,
                &target.remote_name,
                &target.local_branch,
                &target.remote_branch,
                target.set_upstream,
                target.no_verify,
                provider,
                reporter,
                cancel,
            )
        },
    );
    complete(&events, op, TransferOperation::PushBranch, result);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn delete_remote_branch<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    remote_name: String,
    branch_name: String,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::DeleteRemoteBranch,
    });
    let _ = reply.send(Ok(op.id.clone()));
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let name = remote_name.clone();
    let result = on_helper(
        repo,
        credentials,
        &remote_name,
        &op,
        &events,
        move |repo, provider, reporter, cancel| {
            git_core::remote::delete_remote_branch(
                repo,
                &name,
                &branch_name,
                no_verify,
                provider,
                reporter,
                cancel,
            )
        },
    );
    complete(&events, op, TransferOperation::DeleteRemoteBranch, result);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn delete_remote_tag<S: CredentialStore + Send + Sync + 'static>(
    repo: &git2::Repository,
    credentials: &Arc<CredentialService<S>>,
    remote_name: String,
    tag_name: String,
    no_verify: bool,
    op: TransferOp,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, ErrorDto>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: op.id.clone(),
        operation: TransferOperation::DeleteRemoteTag,
    });
    let _ = reply.send(Ok(op.id.clone()));
    super::hooks::log_skipped_pre_push(repo, no_verify);
    let name = remote_name.clone();
    let result = on_helper(
        repo,
        credentials,
        &remote_name,
        &op,
        &events,
        move |repo, provider, reporter, cancel| {
            git_core::remote::delete_remote_tag(
                repo, &name, &tag_name, no_verify, provider, reporter, cancel,
            )
        },
    );
    complete(&events, op, TransferOperation::DeleteRemoteTag, result);
}

impl WorkerHandle {
    pub fn fetch_remote(
        &self,
        remote_name: String,
        prune: Option<bool>,
        events: Sender<TransferEvent>,
    ) -> Result<String, ErrorDto> {
        let operation_id = self.cancelled.allocate("fetch");
        self.request(|reply| Command::FetchRemote {
            remote_name,
            prune,
            operation_id,
            events,
            reply,
        })
    }
    pub fn fetch_all_remotes(
        &self,
        prune: Option<bool>,
        events: Sender<TransferEvent>,
    ) -> Result<String, ErrorDto> {
        let operation_id = self.cancelled.allocate("fetch-all");
        self.request(|reply| Command::FetchAllRemotes {
            prune,
            background: None,
            operation_id,
            events,
            reply,
        })
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
    ) -> Result<Option<String>, ErrorDto> {
        self.background_fetch_with(
            super::background_fetch::BackgroundFetchOptions::new(prune, interval_minutes),
            events,
        )
    }

    pub(super) fn background_fetch_with(
        &self,
        options: super::background_fetch::BackgroundFetchOptions,
        events: Sender<TransferEvent>,
    ) -> Result<Option<String>, ErrorDto> {
        let Some(operation_id) = self.cancelled.allocate_if_idle("background-fetch") else {
            return Ok(None);
        };
        self.request(|reply| Command::FetchAllRemotes {
            prune: options.prune,
            background: Some(options),
            operation_id,
            events,
            reply,
        })
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
    ) -> Result<String, ErrorDto> {
        let operation_id = self.cancelled.allocate("push");
        self.request(|reply| Command::PushBranch {
            remote_name,
            local_branch,
            remote_branch,
            set_upstream,
            no_verify,
            operation_id,
            events,
            reply,
        })
    }
    pub fn delete_remote_branch(
        &self,
        remote_name: String,
        branch_name: String,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, ErrorDto> {
        let operation_id = self.cancelled.allocate("delete-remote");
        self.request(|reply| Command::DeleteRemoteBranch {
            remote_name,
            branch_name,
            no_verify,
            operation_id,
            events,
            reply,
        })
    }
    pub fn delete_remote_tag(
        &self,
        remote_name: String,
        tag_name: String,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, ErrorDto> {
        let operation_id = self.cancelled.allocate("delete-remote");
        self.request(|reply| Command::DeleteRemoteTag {
            remote_name,
            tag_name,
            no_verify,
            operation_id,
            events,
            reply,
        })
    }
    pub fn pull_current_upstream(
        &self,
        events: Sender<TransferEvent>,
    ) -> Result<PullOutcome, ErrorDto> {
        let operation_id = self.cancelled.allocate("pull");
        self.request(|reply| Command::PullCurrentUpstream {
            operation_id,
            events,
            reply,
        })
    }
    pub fn push_current_branch(
        &self,
        remote_name: String,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, ErrorDto> {
        let operation_id = self.cancelled.allocate("push");
        self.request(|reply| Command::PushCurrentBranch {
            remote_name,
            no_verify,
            operation_id,
            events,
            reply,
        })
    }
    pub fn force_push_current_branch(
        &self,
        remote_name: String,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, ErrorDto> {
        let operation_id = self.cancelled.allocate("force-push");
        self.request(|reply| Command::ForcePushCurrentBranch {
            remote_name,
            no_verify,
            operation_id,
            events,
            reply,
        })
    }
    pub fn push_tags(
        &self,
        remote_name: String,
        names: Vec<String>,
        no_verify: bool,
        events: Sender<TransferEvent>,
    ) -> Result<String, ErrorDto> {
        let operation_id = self.cancelled.allocate("push");
        self.request(|reply| Command::PushTags {
            remote_name,
            names,
            no_verify,
            operation_id,
            events,
            reply,
        })
    }
    pub fn list_remotes(&self) -> Result<Vec<RemoteInfo>, ErrorDto> {
        self.request(|reply| Command::ListRemotes { reply })
    }
    pub fn list_remote_branches(&self, remote_name: String) -> Result<Vec<String>, ErrorDto> {
        self.request(|reply| Command::ListRemoteBranches { remote_name, reply })
    }
    pub fn get_current_upstream(&self) -> Result<Option<UpstreamInfo>, ErrorDto> {
        self.request(|reply| Command::GetCurrentUpstream { reply })
    }
    pub fn get_remote_upstreams(&self, name: String) -> Result<Vec<UpstreamInfo>, ErrorDto> {
        self.request(|reply| Command::GetRemoteUpstreams { name, reply })
    }
    pub fn get_remote_auth_mode(
        &self,
        name: String,
    ) -> Result<Option<git_core::remote::RemoteAuthMode>, ErrorDto> {
        self.request(|reply| Command::GetRemoteAuthMode { name, reply })
    }
    pub fn save_https_credential(
        &self,
        remote_name: String,
        username: String,
        token: String,
    ) -> Result<(), ErrorDto> {
        self.request(|reply| Command::SaveHttpsCredential {
            remote_name,
            username,
            token,
            reply,
        })
    }
    pub fn forget_https_credential(&self, remote_name: String) -> Result<(), ErrorDto> {
        self.request(|reply| Command::ForgetHttpsCredential { remote_name, reply })
    }
    pub fn set_remote_auth_mode(
        &self,
        remote_name: String,
        mode: git_core::remote::RemoteAuthMode,
    ) -> Result<(), ErrorDto> {
        self.request(|reply| Command::SetRemoteAuthMode {
            remote_name,
            mode,
            reply,
        })
    }
    pub fn add_remote(
        &self,
        name: String,
        fetch_url: String,
        push_url: Option<String>,
    ) -> Result<(), ErrorDto> {
        self.request(|reply| Command::AddRemote {
            name,
            fetch_url,
            push_url,
            reply,
        })
    }
    pub fn rename_remote(&self, old_name: String, new_name: String) -> Result<(), ErrorDto> {
        self.request(|reply| Command::RenameRemote {
            old_name,
            new_name,
            reply,
        })
    }
    pub fn update_remote_urls(
        &self,
        name: String,
        fetch_url: String,
        push_url: Option<String>,
    ) -> Result<(), ErrorDto> {
        self.request(|reply| Command::UpdateRemoteUrls {
            name,
            fetch_url,
            push_url,
            reply,
        })
    }
    pub fn remove_remote(&self, name: String, clear_upstreams: bool) -> Result<(), ErrorDto> {
        self.request(|reply| Command::RemoveRemote {
            name,
            clear_upstreams,
            reply,
        })
    }
    pub fn set_current_upstream(
        &self,
        remote_name: String,
        remote_branch: String,
    ) -> Result<(), ErrorDto> {
        self.request(|reply| Command::SetCurrentUpstream {
            remote_name,
            remote_branch,
            reply,
        })
    }
    pub fn clear_current_upstream(&self) -> Result<(), ErrorDto> {
        self.request(|reply| Command::ClearCurrentUpstream { reply })
    }
}

#[cfg(test)]
mod tests {
    use super::pull_error;
    use crate::credentials::CredentialStoreError;
    use crate::wire::{ErrorDto, ErrorKind};

    /// A pull stopped at a file another program holds open names it (#321), not "pull failed".
    #[test]
    fn a_pull_stopped_at_a_held_file_says_so() {
        let error = git_core::remote::RemoteError::HeldOpen(git_core::HeldOpen {
            path: "d/f.txt".to_string(),
            during: git_core::HeldDuring::RolledBack,
        });

        assert_eq!(
            pull_error(error).message,
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
            pull_error(error).message,
            "Can't read core.ignorecase from the Git config: failed to parse 'maybe' as a \
             boolean. Set it to true or false (or remove it), then try again."
        );
    }

    #[test]
    fn a_credential_store_failure_keeps_the_backend_diagnostic() {
        let error =
            CredentialStoreError::Keychain("CredWrite failed with Windows error 1312".to_owned());

        let error = ErrorDto::from(error);

        assert_eq!(
            error.message,
            "credential keychain failure: CredWrite failed with Windows error 1312"
        );
        assert_eq!(error.kind, ErrorKind::CredentialStoreFailure);
    }

    /// #496: a cancelled pull answers `Cancelled`, so the frontend leaves it to the pull's
    /// terminal transfer event ("Pull cancelled."); a refused one `DirtyWorktree`; a transport
    /// failure `PullFailed`, without its text.
    #[test]
    fn a_pull_error_has_its_kind() {
        use git_core::remote::RemoteError;
        assert_eq!(
            pull_error(RemoteError::Cancelled).kind,
            ErrorKind::Cancelled
        );
        assert_eq!(
            pull_error(RemoteError::DirtyWorktree).kind,
            ErrorKind::DirtyWorktree
        );
        assert_eq!(pull_error(RemoteError::NoUpstream).kind, ErrorKind::Other);
        let failed = pull_error(RemoteError::Git(git2::Error::from_str(
            "https://user:secret@example.com/repo.git: 403",
        )));
        assert_eq!(failed, ErrorDto::pull_failed());
    }
}
