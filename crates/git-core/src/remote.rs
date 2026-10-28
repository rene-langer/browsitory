use std::cell::{Cell, RefCell};

use git2::{
    build::CheckoutBuilder, BranchType, Cred, CredentialType, ErrorCode, FetchOptions, FetchPrune,
    PushOptions, RemoteCallbacks, Repository, StatusOptions, Tag,
};
use thiserror::Error;
use url::Url;

#[cfg(test)]
mod tests {
    use super::sanitize_remote_message;

    #[test]
    fn arbitrary_remote_messages_are_dropped_before_progress_records_are_built() {
        assert_eq!(
            sanitize_remote_message(b"https://alice:secret@example.test/repo.git"),
            None
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferOperation {
    Fetch,
    FetchAll,
    Pull,
    PushBranch,
    ForcePushBranch,
    PushTags,
    DeleteRemoteBranch,
    Clone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPhase {
    Receiving,
    Updating,
    /// A `pre-push` hook is running; no transfer progress yet.
    RunningHook,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferErrorKind {
    NonFastForward,
    RejectedRemoteRef,
    StaleLease,
    MissingCredential,
    CredentialStoreFailure,
    SshAgentFailure,
    TransferFailed,
    /// The caller asked for the in-flight transfer to stop — see the `cancel` parameter on
    /// `fetch_remote`/`push_current_branch`/`push_tags`.
    Cancelled,
    /// The `pre-push` hook failed (or could not run); nothing was sent.
    HookFailed,
}

/// Stable callback marker used internally to classify a missing keychain token without sending
/// the callback or transport diagnostic across the UI boundary.
pub const MISSING_CREDENTIAL_ERROR: &str = "missing credential for remote";
pub const CREDENTIAL_STORE_FAILURE_ERROR: &str = "credential keychain failure";
pub const SSH_AGENT_FAILURE_ERROR: &str = "SSH agent failure";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferProgress {
    pub operation_id: String,
    pub operation: TransferOperation,
    pub phase: TransferPhase,
    pub current: usize,
    pub total: usize,
    pub received_bytes: usize,
    pub message: Option<String>,
}

pub trait TransferReporter {
    fn report(&mut self, event: TransferProgress);
}

pub trait CredentialProvider {
    fn credential(
        &mut self,
        url: &str,
        username: Option<&str>,
        allowed: CredentialType,
    ) -> Result<Cred, git2::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteAuthMode {
    HttpsToken { username: String },
    SshAgent,
}

fn sanitize_remote_message(_message: &[u8]) -> Option<String> {
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: String,
    pub fetch_url: String,
    pub push_url: Option<String>,
    /// Effective prune default: `remote.<name>.prune`, else `fetch.prune`, else `false`.
    pub prune: bool,
}

/// A per-remote credential provider built by `fetch_all`'s `credentials_for` callback.
pub type BoxedCredentialProvider<'c> = Box<dyn CredentialProvider + 'c>;

/// Result of `fetch_all`: remotes fetched, and the ones that failed with why.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FetchAllOutcome {
    pub fetched: Vec<String>,
    pub failed: Vec<(String, TransferErrorKind)>,
}

/// `None` leaves pruning to git config (`remote.<name>.prune`, then `fetch.prune`).
fn fetch_prune(prune: Option<bool>) -> FetchPrune {
    match prune {
        None => FetchPrune::Unspecified,
        Some(true) => FetchPrune::On,
        Some(false) => FetchPrune::Off,
    }
}

fn config_bool(config: &git2::Config, key: &str) -> Result<Option<bool>, git2::Error> {
    match config.get_bool(key) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamInfo {
    pub local_branch: String,
    pub remote_name: String,
    pub remote_branch: String,
    /// Commits on the local branch not on its remote-tracking ref. `None` when the tracking ref
    /// does not exist locally (never fetched), so "unknown" is not shown as zero.
    pub ahead: Option<usize>,
    /// Commits on the remote-tracking ref not on the local branch; `None` as for `ahead`.
    pub behind: Option<usize>,
    /// Where pushing `local_branch` to `remote_name` lands under `push.default` (see
    /// `resolve_push_target`); `None` when that needs the user's choice.
    pub push_branch: Option<String>,
}

/// Where a push of a local branch to a remote lands, as `push.default` decides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushTarget {
    /// Push to `refs/heads/<branch>` on `remote`.
    Branch { remote: String, branch: String },
    /// `push.default` does not settle a target; ask (the UI opens "Push to…" prefilled with
    /// `remote` and `local_branch`).
    NeedsChoice {
        remote: String,
        local_branch: String,
    },
}

/// The single push-target rule shared by push, force push and the UI.
///
/// When `remote_name` is the upstream's remote (`upstream = Some((remote, branch))`):
/// - `upstream` / `tracking`: the upstream's branch name.
/// - `current`: the local branch name.
/// - `simple` (also when unset, git's default) and `matching`: the local name when it equals the
///   upstream's branch name, otherwise `NeedsChoice` — never silently push a branch that merely
///   tracks e.g. `origin/main` onto `main`. (`matching` pushes every same-named branch in git;
///   this app only ever pushes one branch, so it applies the `simple` check to it.)
/// - `nothing`: always `NeedsChoice`.
///
/// When the remote was chosen explicitly and is not the upstream's (or there is no upstream),
/// every mode but `nothing` pushes under the local name (git's `simple`/`current` behaviour for a
/// triangular push); `nothing` asks. Unknown values behave like `simple`.
pub fn resolve_push_target(
    push_default: Option<&str>,
    local_branch: &str,
    remote_name: &str,
    upstream: Option<(&str, &str)>,
) -> PushTarget {
    let branch = |name: &str| PushTarget::Branch {
        remote: remote_name.to_string(),
        branch: name.to_string(),
    };
    let ask = || PushTarget::NeedsChoice {
        remote: remote_name.to_string(),
        local_branch: local_branch.to_string(),
    };
    let mode = push_default.unwrap_or("simple");
    if mode == "nothing" {
        return ask();
    }
    match upstream {
        Some((remote, upstream_branch)) if remote == remote_name => match mode {
            "upstream" | "tracking" => branch(upstream_branch),
            "current" => branch(local_branch),
            _ if upstream_branch == local_branch => branch(local_branch),
            _ => ask(),
        },
        _ => branch(local_branch),
    }
}

fn read_push_default(config: &git2::Config) -> Result<Option<String>, git2::Error> {
    match config.get_string("push.default") {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// `resolve_push_target` for `local_branch` pushed to `remote_name`, reading `push.default` and
/// the branch's upstream config.
pub fn push_target(
    repo: &Repository,
    local_branch: &str,
    remote_name: &str,
) -> Result<PushTarget, RemoteError> {
    let config = repo.config()?;
    let push_default = read_push_default(&config)?;
    let upstream = configured_upstream(&config, local_branch)?;
    Ok(resolve_push_target(
        push_default.as_deref(),
        local_branch,
        remote_name,
        upstream
            .as_ref()
            .map(|(remote, branch)| (remote.as_str(), branch.as_str())),
    ))
}

/// The `push_branch` for an `UpstreamInfo` of `local_branch` tracking `remote/remote_branch`.
fn upstream_push_branch(
    config: &git2::Config,
    local_branch: &str,
    remote_name: &str,
    remote_branch: &str,
) -> Result<Option<String>, git2::Error> {
    let push_default = read_push_default(config)?;
    Ok(
        match resolve_push_target(
            push_default.as_deref(),
            local_branch,
            remote_name,
            Some((remote_name, remote_branch)),
        ) {
            PushTarget::Branch { branch, .. } => Some(branch),
            PushTarget::NeedsChoice { .. } => None,
        },
    )
}

/// `(remote, remote_branch)` from `branch.<local_branch>.remote` / `.merge`, or `None` when
/// either key is missing. Never checks that the tracking ref exists.
pub(crate) fn configured_upstream(
    config: &git2::Config,
    local_branch: &str,
) -> Result<Option<(String, String)>, git2::Error> {
    let read = |key: String| match config.get_string(&key) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(None),
        Err(error) => Err(error),
    };
    let Some(remote) = read(format!("branch.{local_branch}.remote"))? else {
        return Ok(None);
    };
    let Some(merge) = read(format!("branch.{local_branch}.merge"))? else {
        return Ok(None);
    };
    let branch = merge
        .strip_prefix("refs/heads/")
        .unwrap_or(&merge)
        .to_string();
    Ok(Some((remote, branch)))
}

/// Ahead/behind of `local_branch` against `refs/remotes/<remote>/<remote_branch>`, or
/// `(None, None)` when either side cannot be resolved.
pub(crate) fn ahead_behind(
    repo: &Repository,
    local_branch: &str,
    remote_name: &str,
    remote_branch: &str,
) -> (Option<usize>, Option<usize>) {
    let local = repo
        .find_reference(&format!("refs/heads/{local_branch}"))
        .ok()
        .and_then(|r| r.target());
    let remote = repo
        .find_reference(&format!("refs/remotes/{remote_name}/{remote_branch}"))
        .ok()
        .and_then(|r| r.target());
    match (local, remote) {
        (Some(local), Some(remote)) => match repo.graph_ahead_behind(local, remote) {
            Ok((ahead, behind)) => (Some(ahead), Some(behind)),
            Err(_) => (None, None),
        },
        _ => (None, None),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagInfo {
    pub name: String,
    /// The object the tag points at directly: for an annotated tag its tag object's target,
    /// which is another tag for a tag of a tag.
    pub target_id: String,
    /// The commit the tag peels to through any chain of tags; `None` for a tag of a tree or blob.
    pub commit_id: Option<String>,
    pub annotated: bool,
    pub message: Option<String>,
    pub tagger_name: Option<String>,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PullOutcome {
    UpToDate,
    FastForwarded { upstream_ref: String },
    Diverged { upstream_ref: String },
}

#[derive(Debug, Error)]
pub enum RemoteError {
    #[error("git remote operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("remote URLs must not contain embedded credentials")]
    CredentialBearingUrl,
    #[error("remote '{name}' is the upstream for local branch(es): {branches:?}")]
    RemoteInUse { name: String, branches: Vec<String> },
    #[error("cannot pull with a dirty worktree")]
    DirtyWorktree,
    #[error("cannot pull while HEAD is detached")]
    DetachedHead,
    #[error("untracked or ignored files would be overwritten by the pull")]
    CheckoutConflict,
    #[error("current branch has no upstream")]
    NoUpstream,
    #[error("push was rejected because it was not a fast-forward")]
    NonFastForward,
    #[error("the remote rejected a pushed reference")]
    RejectedRemoteRef,
    #[error("the remote branch changed since the last fetch")]
    StaleLease,
    #[error("the transfer was cancelled")]
    Cancelled,
    /// `push.default` does not settle where `local_branch` goes on `remote` (e.g. `simple` with a
    /// differently named upstream): the caller must ask, e.g. with the "Push to…" dialog.
    #[error(
        "push.default does not choose where to push '{local_branch}' on '{remote}'; use Push to… to pick the remote branch"
    )]
    PushTargetNeedsChoice {
        remote: String,
        local_branch: String,
    },
    /// A gate hook (`pre-push`) failed; nothing was sent. `output` is what the user sees.
    #[error("the {hook} hook failed")]
    HookFailed { hook: String, output: String },
    #[error("{0}")]
    Hook(#[from] crate::hooks::HookError),
}

impl RemoteError {
    pub fn transfer_error_kind(&self) -> TransferErrorKind {
        match self {
            Self::Cancelled => TransferErrorKind::Cancelled,
            Self::NonFastForward => TransferErrorKind::NonFastForward,
            Self::RejectedRemoteRef => TransferErrorKind::RejectedRemoteRef,
            Self::StaleLease => TransferErrorKind::StaleLease,
            Self::HookFailed { .. } | Self::Hook(crate::hooks::HookError::Spawn { .. }) => {
                TransferErrorKind::HookFailed
            }
            Self::Hook(crate::hooks::HookError::Cancelled { .. }) => TransferErrorKind::Cancelled,
            Self::Git(error) if error.code() == ErrorCode::NotFastForward => {
                TransferErrorKind::NonFastForward
            }
            Self::Git(error) if error.message() == MISSING_CREDENTIAL_ERROR => {
                TransferErrorKind::MissingCredential
            }
            Self::Git(error) if error.message() == CREDENTIAL_STORE_FAILURE_ERROR => {
                TransferErrorKind::CredentialStoreFailure
            }
            Self::Git(error) if error.message() == SSH_AGENT_FAILURE_ERROR => {
                TransferErrorKind::SshAgentFailure
            }
            _ => TransferErrorKind::TransferFailed,
        }
    }
}

/// Fetches `remote_name`, reporting progress through `reporter`.
///
/// `cancel` is polled from inside libgit2's own progress callbacks — the only place a caller can
/// interrupt a transfer that is already blocked in network I/O. Returning `false` from
/// `transfer_progress`/`sideband_progress` makes libgit2 abort the transfer, which surfaces here
/// as a generic `git2::Error` (libgit2 does not give the abort its own error code), so `cancel`
/// is polled once more after the failure to tell a cancellation from a real transport failure.
pub fn fetch_remote(
    repo: &Repository,
    remote_name: &str,
    prune: Option<bool>,
    operation_id: String,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    fetch_with(
        repo,
        remote_name,
        prune,
        TransferOperation::Fetch,
        &operation_id,
        credentials,
        reporter,
        cancel,
    )
}

/// Fetches every remote in `repo.remotes()` order under one `operation_id`, reporting progress
/// as `TransferOperation::FetchAll`. A remote that fails (transport, credentials) is recorded in
/// `failed` and the next one is tried; a cancel stops the loop with `Cancelled` (remotes fetched
/// so far stay fetched). `credentials_for` builds each remote's credential provider.
pub fn fetch_all<'c>(
    repo: &Repository,
    prune: Option<bool>,
    operation_id: String,
    credentials_for: &mut dyn FnMut(&str) -> Result<BoxedCredentialProvider<'c>, RemoteError>,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<FetchAllOutcome, RemoteError> {
    let names: Vec<String> = repo
        .remotes()?
        .iter()
        .flatten()
        .flatten()
        .map(str::to_string)
        .collect();
    let mut outcome = FetchAllOutcome::default();
    for name in names {
        if cancel() {
            return Err(RemoteError::Cancelled);
        }
        let result = credentials_for(&name).and_then(|mut provider| {
            fetch_with(
                repo,
                &name,
                prune,
                TransferOperation::FetchAll,
                &operation_id,
                provider.as_mut(),
                reporter,
                cancel,
            )
        });
        match result {
            Ok(()) => outcome.fetched.push(name),
            Err(RemoteError::Cancelled) => return Err(RemoteError::Cancelled),
            Err(error) => outcome.failed.push((name, error.transfer_error_kind())),
        }
    }
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
fn fetch_with(
    repo: &Repository,
    remote_name: &str,
    prune: Option<bool>,
    operation: TransferOperation,
    operation_id: &str,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    let mut remote = repo.find_remote(remote_name)?;
    let credentials = RefCell::new(credentials);
    let reporter = RefCell::new(reporter);
    let mut callbacks = RemoteCallbacks::new();

    callbacks.credentials(|url, username, allowed| {
        credentials.borrow_mut().credential(url, username, allowed)
    });
    callbacks.transfer_progress(|progress| {
        if cancel() {
            return false;
        }
        reporter.borrow_mut().report(TransferProgress {
            operation_id: operation_id.to_string(),
            operation,
            phase: TransferPhase::Receiving,
            current: progress.received_objects(),
            total: progress.total_objects(),
            received_bytes: progress.received_bytes(),
            message: None,
        });
        true
    });
    callbacks.sideband_progress(|message| {
        if cancel() {
            return false;
        }
        reporter.borrow_mut().report(TransferProgress {
            operation_id: operation_id.to_string(),
            operation,
            phase: TransferPhase::Receiving,
            current: 0,
            total: 0,
            received_bytes: 0,
            message: sanitize_remote_message(message),
        });
        true
    });
    callbacks.update_tips(|_reference, _old, _new| {
        if cancel() {
            return false;
        }
        reporter.borrow_mut().report(TransferProgress {
            operation_id: operation_id.to_string(),
            operation,
            phase: TransferPhase::Updating,
            current: 0,
            total: 0,
            received_bytes: 0,
            message: None,
        });
        true
    });

    let mut options = FetchOptions::new();
    options.remote_callbacks(callbacks);
    options.prune(fetch_prune(prune));
    match remote.fetch(&[] as &[&str], Some(&mut options), None) {
        Ok(()) => Ok(()),
        Err(error) => Err(cancellation_or(cancel, error)),
    }
}

/// Classifies a failed transfer: libgit2 reports a callback-initiated abort as an ordinary
/// error, so the only way to tell "the user cancelled" from "the transport broke" is to ask the
/// cancel signal again now that the call has returned.
fn cancellation_or(cancel: &dyn Fn() -> bool, error: git2::Error) -> RemoteError {
    if cancel() {
        RemoteError::Cancelled
    } else {
        RemoteError::Git(error)
    }
}

pub fn list_tags(repo: &Repository) -> Result<Vec<TagInfo>, RemoteError> {
    let mut tags = Vec::new();
    for name in repo.tag_names(None)?.iter().flatten().flatten() {
        let reference = repo.find_reference(&format!("refs/tags/{name}"))?;
        let target_id = reference
            .target()
            .ok_or_else(|| git2::Error::from_str("tag reference has no direct target"))?;
        let commit_id = reference
            .peel_to_commit()
            .ok()
            .map(|commit| commit.id().to_string());
        if let Ok(tag) = repo.find_tag(target_id) {
            let tagger = tag.tagger();
            tags.push(TagInfo {
                name: name.to_string(),
                target_id: tag.target_id().to_string(),
                commit_id,
                annotated: true,
                message: tag.message()?.map(str::to_string),
                tagger_name: tagger
                    .as_ref()
                    .and_then(|signature| signature.name().ok().map(str::to_string)),
                timestamp: tagger.map(|signature| signature.when().seconds()),
            });
        } else {
            tags.push(TagInfo {
                name: name.to_string(),
                target_id: target_id.to_string(),
                commit_id,
                annotated: false,
                message: None,
                tagger_name: None,
                timestamp: None,
            });
        }
    }
    Ok(tags)
}

pub fn create_tag(repo: &Repository, name: &str, message: Option<&str>) -> Result<(), RemoteError> {
    validate_tag_name(name)?;
    let target = repo.head()?.peel_to_commit()?.into_object();
    match message {
        Some(message) => {
            let tagger = repo.signature()?;
            repo.tag(name, &target, &tagger, message, false)?;
        }
        None => {
            repo.tag_lightweight(name, &target, false)?;
        }
    }
    Ok(())
}

pub fn delete_tag(repo: &Repository, name: &str) -> Result<(), RemoteError> {
    validate_tag_name(name)?;
    repo.find_reference(&format!("refs/tags/{name}"))?
        .delete()?;
    Ok(())
}

/// Pushes the current branch to `remote_name`, to the target `push_target` resolves from
/// `push.default` (`PushTargetNeedsChoice` when it settles none; nothing is sent). `cancel` is
/// checked once, at `push_negotiation` (before any data is sent); a cancel requested after that
/// is ignored so a push that lands is never reported as cancelled — see `push_refs`.
/// Runs the `pre-push` hook unless `no_verify`.
pub fn push_current_branch(
    repo: &Repository,
    remote_name: &str,
    no_verify: bool,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    let (local, target) = current_push_destination(repo, remote_name)?;
    push_branch(
        repo,
        remote_name,
        &local,
        &target,
        false,
        no_verify,
        credentials,
        reporter,
        cancel,
    )
}

/// Force-pushes the current branch to the same target `push_current_branch` resolves (so never
/// to a differently named upstream unless `push.default=upstream`), but only if the remote's
/// branch still points where our `refs/remotes/<remote>/<target>` says it did at the last fetch
/// (a missing tracking ref means "must not exist on the remote"). A mismatch is
/// `RemoteError::StaleLease`, remote untouched. Runs the `pre-push` hook unless `no_verify`.
pub fn force_push_current_branch(
    repo: &Repository,
    remote_name: &str,
    no_verify: bool,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    let (local, target) = current_push_destination(repo, remote_name)?;
    validate_branch_name(&target)?;
    let expected = repo
        .find_reference(&format!("refs/remotes/{remote_name}/{target}"))
        .ok()
        .and_then(|reference| reference.target())
        .unwrap_or(git2::Oid::ZERO_SHA1);
    let dst = format!("refs/heads/{target}");
    let refspec = format!("+refs/heads/{local}:{dst}");
    push_refs(
        repo,
        remote_name,
        &[refspec],
        TransferOperation::ForcePushBranch,
        credentials,
        reporter,
        cancel,
        Some((dst.as_str(), expected)),
        no_verify,
    )
}

/// `push_target` for the current branch.
pub fn current_push_target(
    repo: &Repository,
    remote_name: &str,
) -> Result<PushTarget, RemoteError> {
    let local = current_local_branch_name(repo)?;
    push_target(repo, &local, remote_name)
}

/// The current branch and the remote branch it pushes to, or `PushTargetNeedsChoice`.
fn current_push_destination(
    repo: &Repository,
    remote_name: &str,
) -> Result<(String, String), RemoteError> {
    let local = current_local_branch_name(repo)?;
    match push_target(repo, &local, remote_name)? {
        PushTarget::Branch { branch, .. } => Ok((local, branch)),
        PushTarget::NeedsChoice {
            remote,
            local_branch,
        } => Err(RemoteError::PushTargetNeedsChoice {
            remote,
            local_branch,
        }),
    }
}

/// Rejects a remote branch name that is empty, `HEAD`, `@`, starts with `refs/`, `-` or `+`
/// (`refs/heads/refs/...` would read as a full ref name to git), or is not a valid
/// `refs/heads/<name>`, so a refspec built from it can only ever address that one branch.
fn validate_branch_name(name: &str) -> Result<(), RemoteError> {
    if name.is_empty()
        || name == "HEAD"
        || name == "@"
        || name.starts_with("refs/")
        || name.starts_with('-')
        || name.starts_with('+')
        || !git2::Reference::is_valid_name(&format!("refs/heads/{name}"))
    {
        return Err(git2::Error::from_str("invalid branch name").into());
    }
    Ok(())
}

/// Records `<remote_name>/<remote_branch>` as `local_branch`'s upstream by writing
/// `branch.<local>.remote` and `.merge` directly (a branch can track a remote branch that has not
/// been fetched yet). Deliberately not `Branch::set_upstream`: it resolves its argument as a
/// *local* branch first, so a local branch literally named `origin/topic` made it write
/// `remote = .` and `merge = refs/heads/origin/topic`.
pub fn set_branch_upstream(
    repo: &Repository,
    local_branch: &str,
    remote_name: &str,
    remote_branch: &str,
) -> Result<(), RemoteError> {
    repo.find_remote(remote_name)?;
    repo.find_branch(local_branch, BranchType::Local)?;
    let mut config = repo.config()?;
    config.set_str(&format!("branch.{local_branch}.remote"), remote_name)?;
    config.set_str(
        &format!("branch.{local_branch}.merge"),
        &format!("refs/heads/{remote_branch}"),
    )?;
    Ok(())
}

/// Pushes `local_branch` to `refs/heads/<remote_branch>` on `remote_name` (never forced: a
/// non-fast-forward is `NonFastForward`). With `set_upstream`, a push that lands also makes that
/// remote branch the local branch's upstream; a failed push leaves the config untouched.
/// `cancel` behaves as for `push_current_branch`. Runs the `pre-push` hook unless `no_verify`.
#[allow(clippy::too_many_arguments)]
pub fn push_branch(
    repo: &Repository,
    remote_name: &str,
    local_branch: &str,
    remote_branch: &str,
    set_upstream: bool,
    no_verify: bool,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    repo.find_branch(local_branch, BranchType::Local)?;
    validate_branch_name(remote_branch)?;
    let refspec = format!("refs/heads/{local_branch}:refs/heads/{remote_branch}");
    push_refs(
        repo,
        remote_name,
        &[refspec],
        TransferOperation::PushBranch,
        credentials,
        reporter,
        cancel,
        None,
        no_verify,
    )?;
    if set_upstream {
        set_branch_upstream(repo, local_branch, remote_name, remote_branch)?;
    }
    Ok(())
}

/// Deletes `refs/heads/<branch>` on `remote_name`, then our `refs/remotes/<remote>/<branch>`.
/// Local branches that tracked it keep their config (git's "gone" upstream). A refusal
/// (protected branch, server hook) is `RejectedRemoteRef`; `cancel` behaves as for
/// `push_current_branch`. Runs the `pre-push` hook unless `no_verify`, like
/// `git push <remote> :<branch>` (its stdin line starts with `(delete)`).
pub fn delete_remote_branch(
    repo: &Repository,
    remote_name: &str,
    branch: &str,
    no_verify: bool,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    validate_branch_name(branch)?;
    push_refs(
        repo,
        remote_name,
        &[format!(":refs/heads/{branch}")],
        TransferOperation::DeleteRemoteBranch,
        credentials,
        reporter,
        cancel,
        None,
        no_verify,
    )?;
    // libgit2's push update_tips normally removed it already; this makes it not depend on that.
    // The local tracking ref is wherever the remote's fetch refspec maps the branch, not a
    // `refs/remotes/<remote>/<branch>` convention; no matching refspec means none is touched.
    if let Some(tracking_name) = tracking_ref_name(repo, remote_name, branch)? {
        match repo.find_reference(&tracking_name) {
            Ok(mut tracking) => tracking.delete()?,
            Err(error) if error.code() == ErrorCode::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// The local ref `remote_name`'s fetch refspecs map `refs/heads/<branch>` to, if any.
fn tracking_ref_name(
    repo: &Repository,
    remote_name: &str,
    branch: &str,
) -> Result<Option<String>, RemoteError> {
    let remote = repo.find_remote(remote_name)?;
    let source = format!("refs/heads/{branch}");
    for refspec in remote.refspecs() {
        if refspec.direction() != git2::Direction::Fetch || !refspec.src_matches(&source) {
            continue;
        }
        let mapped = refspec.transform(&source)?;
        if let Ok(name) = mapped.as_str() {
            return Ok(Some(name.to_string()));
        }
    }
    Ok(None)
}

/// Pushes `names` (or every local tag when empty). `cancel` behaves as for
/// `push_current_branch`. Runs the `pre-push` hook unless `no_verify`.
pub fn push_tags(
    repo: &Repository,
    remote_name: &str,
    names: &[String],
    no_verify: bool,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    let tag_names = if names.is_empty() {
        repo.tag_names(None)?
            .iter()
            .flatten()
            .flatten()
            .map(str::to_string)
            .collect::<Vec<_>>()
    } else {
        names.to_vec()
    };
    if tag_names.is_empty() {
        return Ok(());
    }

    let mut refspecs = Vec::with_capacity(tag_names.len());
    for name in &tag_names {
        validate_tag_name(name)?;
        refspecs.push(format!("refs/tags/{name}:refs/tags/{name}"));
    }
    push_refs(
        repo,
        remote_name,
        &refspecs,
        TransferOperation::PushTags,
        credentials,
        reporter,
        cancel,
        None,
        no_verify,
    )
}

pub fn pull_after_fetch(
    repo: &Repository,
    remote_name: &str,
    remote_branch: &str,
) -> Result<PullOutcome, RemoteError> {
    if !worktree_is_clean(repo)? {
        return Err(RemoteError::DirtyWorktree);
    }

    let local_branch = current_local_branch_name(repo)?;
    let upstream_ref = format!("refs/remotes/{remote_name}/{remote_branch}");
    let upstream_commit = repo.find_reference(&upstream_ref)?.peel_to_commit()?;
    let upstream_oid = upstream_commit.id();
    let local_oid = repo.head()?.peel_to_commit()?.id();

    if local_oid == upstream_oid || repo.graph_descendant_of(local_oid, upstream_oid)? {
        return Ok(PullOutcome::UpToDate);
    }
    if !repo.graph_descendant_of(upstream_oid, local_oid)? {
        return Ok(PullOutcome::Diverged { upstream_ref });
    }

    // Safe checkouts create files through a symlinked tracked folder (#114).
    crate::worktree_guard::refuse_linked_folders(repo)?;
    let local_ref = format!("refs/heads/{local_branch}");
    let mut checkout = CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(upstream_commit.as_object(), Some(&mut checkout))
        .map_err(|error| {
            if error.code() == ErrorCode::Conflict {
                RemoteError::CheckoutConflict
            } else {
                RemoteError::Git(error)
            }
        })?;
    repo.reference(&local_ref, upstream_oid, true, "fast-forward pull")?;

    Ok(PullOutcome::FastForwarded { upstream_ref })
}

pub fn list_remotes(repo: &Repository) -> Result<Vec<RemoteInfo>, RemoteError> {
    let config = repo.config()?;
    let mut remotes = Vec::new();
    for name in repo.remotes()?.iter().flatten().flatten() {
        let remote = repo.find_remote(name)?;
        let fetch_url = remote.url()?;
        let push_url = remote.pushurl()?;
        if contains_embedded_credentials(fetch_url)
            || push_url.is_some_and(contains_embedded_credentials)
        {
            return Err(RemoteError::CredentialBearingUrl);
        }
        remotes.push(RemoteInfo {
            name: name.to_string(),
            fetch_url: fetch_url.to_string(),
            push_url: push_url.map(str::to_string),
            prune: match config_bool(&config, &format!("remote.{name}.prune"))? {
                Some(prune) => prune,
                None => config_bool(&config, "fetch.prune")?.unwrap_or(false),
            },
        });
    }
    Ok(remotes)
}

pub fn list_remote_branches(
    repo: &Repository,
    remote_name: &str,
) -> Result<Vec<String>, RemoteError> {
    let prefix = format!("{remote_name}/");
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Remote))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        let Some(branch_name) = name.strip_prefix(prefix.as_str()) else {
            continue;
        };
        if branch_name == "HEAD" {
            continue;
        }
        branches.push(branch_name.to_string());
    }
    branches.sort();
    Ok(branches)
}

pub fn add_remote(
    repo: &Repository,
    name: &str,
    fetch_url: &str,
    push_url: Option<&str>,
) -> Result<(), RemoteError> {
    validate_urls(fetch_url, push_url)?;
    repo.remote(name, fetch_url)?;
    if let Some(push_url) = push_url {
        repo.remote_set_pushurl(name, Some(push_url))?;
    }
    Ok(())
}

pub fn rename_remote(repo: &Repository, old_name: &str, new_name: &str) -> Result<(), RemoteError> {
    let auth_profile = remote_auth_profile(repo, old_name)?;
    repo.remote_rename(old_name, new_name)?;
    if let Some(profile) = auth_profile {
        set_remote_auth_profile(repo, new_name, profile)?;
        clear_remote_auth_profile(repo, old_name)?;
    }
    Ok(())
}

pub fn set_remote_auth_profile(
    repo: &Repository,
    remote: &str,
    profile: RemoteAuthMode,
) -> Result<(), RemoteError> {
    repo.find_remote(remote)?;
    let mut config = repo.config()?;
    match profile {
        RemoteAuthMode::HttpsToken { username } => {
            config.set_str(&remote_auth_key(remote, "auth-mode"), "https-token")?;
            config.set_str(&remote_auth_key(remote, "username"), &username)?;
        }
        RemoteAuthMode::SshAgent => {
            config.set_str(&remote_auth_key(remote, "auth-mode"), "ssh-agent")?;
            remove_config_key(&mut config, &remote_auth_key(remote, "username"))?;
        }
    }
    Ok(())
}

pub fn clear_remote_auth_profile(repo: &Repository, remote: &str) -> Result<(), RemoteError> {
    let mut config = repo.config()?;
    remove_config_key(&mut config, &remote_auth_key(remote, "auth-mode"))?;
    remove_config_key(&mut config, &remote_auth_key(remote, "username"))?;
    Ok(())
}

pub fn update_remote_urls(
    repo: &Repository,
    name: &str,
    fetch_url: &str,
    push_url: Option<&str>,
) -> Result<(), RemoteError> {
    validate_urls(fetch_url, push_url)?;
    let remote = repo.find_remote(name)?;
    let has_push_url = remote.pushurl()?.is_some();
    drop(remote);
    repo.remote_set_url(name, fetch_url)?;
    if push_url.is_some() || has_push_url {
        repo.remote_set_pushurl(name, push_url)?;
    }
    Ok(())
}

pub fn current_upstream(repo: &Repository) -> Result<Option<UpstreamInfo>, RemoteError> {
    // Unlike `current_local_branch_name`'s other callers (pull, set/clear upstream — all
    // genuine user actions that don't make sense without a current branch), this is a read-only
    // status query polled on every `refresh()`. A detached HEAD (normal mid-rebase or mid-merge)
    // just has no upstream to report, not an error.
    if repo.head_detached()? {
        return Ok(None);
    }
    let local_branch = current_local_branch_name(repo)?;
    let config = repo.config()?;
    let Some((remote_name, remote_branch)) = configured_upstream(&config, &local_branch)? else {
        return Ok(None);
    };

    let (ahead, behind) = ahead_behind(repo, &local_branch, &remote_name, &remote_branch);
    let push_branch = upstream_push_branch(&config, &local_branch, &remote_name, &remote_branch)?;
    Ok(Some(UpstreamInfo {
        local_branch,
        remote_name,
        remote_branch,
        ahead,
        behind,
        push_branch,
    }))
}

pub fn set_current_upstream(
    repo: &Repository,
    remote_name: &str,
    remote_branch: &str,
) -> Result<(), RemoteError> {
    repo.find_remote(remote_name)?;
    let local_branch = current_local_branch_name(repo)?;
    set_branch_upstream(repo, &local_branch, remote_name, remote_branch)
}

pub fn clear_current_upstream(repo: &Repository) -> Result<(), RemoteError> {
    let local_branch = current_local_branch_name(repo)?;
    let mut branch = repo.find_branch(&local_branch, BranchType::Local)?;
    branch.set_upstream(None)?;
    Ok(())
}

pub fn remove_remote(repo: &Repository, name: &str) -> Result<(), RemoteError> {
    let branches = local_branches_using_remote(repo, name)?;
    if !branches.is_empty() {
        return Err(RemoteError::RemoteInUse {
            name: name.to_string(),
            branches,
        });
    }
    repo.remote_delete(name)?;
    clear_remote_auth_profile(repo, name)?;
    Ok(())
}

pub fn remote_upstreams(
    repo: &Repository,
    remote_name: &str,
) -> Result<Vec<UpstreamInfo>, RemoteError> {
    let config = repo.config()?;
    let mut upstreams = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(local_branch)) = branch.name() else {
            continue;
        };
        match config.get_string(&format!("branch.{local_branch}.remote")) {
            Ok(configured_remote) if configured_remote == remote_name => {}
            Ok(_) | Err(git2::Error { .. }) => continue,
        }
        let merge_ref = config.get_string(&format!("branch.{local_branch}.merge"))?;
        let remote_branch = merge_ref
            .strip_prefix("refs/heads/")
            .unwrap_or(&merge_ref)
            .to_string();
        let (ahead, behind) = ahead_behind(repo, local_branch, remote_name, &remote_branch);
        let push_branch = upstream_push_branch(&config, local_branch, remote_name, &remote_branch)?;
        upstreams.push(UpstreamInfo {
            local_branch: local_branch.to_string(),
            remote_name: remote_name.to_string(),
            remote_branch,
            ahead,
            behind,
            push_branch,
        });
    }
    Ok(upstreams)
}

pub fn remove_remote_and_clear_upstreams(repo: &Repository, name: &str) -> Result<(), RemoteError> {
    for upstream in remote_upstreams(repo, name)? {
        repo.find_branch(&upstream.local_branch, BranchType::Local)?
            .set_upstream(None)?;
    }
    repo.remote_delete(name)?;
    clear_remote_auth_profile(repo, name)?;
    Ok(())
}

fn current_local_branch_name(repo: &Repository) -> Result<String, RemoteError> {
    let head = repo.head()?;
    if !head.is_branch() {
        return Err(RemoteError::DetachedHead);
    }
    Ok(head.shorthand()?.to_string())
}

#[allow(clippy::too_many_arguments)]
fn push_refs(
    repo: &Repository,
    remote_name: &str,
    refspecs: &[String],
    operation: TransferOperation,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
    lease: Option<(&str, git2::Oid)>,
    no_verify: bool,
) -> Result<(), RemoteError> {
    let mut remote = repo.find_remote(remote_name)?;
    let push_url = remote
        .pushurl()
        .ok()
        .flatten()
        .or_else(|| remote.url().ok())
        .unwrap_or_default()
        .to_string();
    let run_pre_push = !no_verify
        && matches!(
            crate::hooks::find_hook(repo, "pre-push"),
            Ok(crate::hooks::HookLookup::Found(_))
        );
    let hook_failure: RefCell<Option<RemoteError>> = RefCell::new(None);
    let credentials = RefCell::new(credentials);
    let reporter = RefCell::new(reporter);
    let rejection = Cell::new(None);
    let mut callbacks = RemoteCallbacks::new();

    callbacks.credentials(|url, username, allowed| {
        credentials.borrow_mut().credential(url, username, allowed)
    });
    // `push_negotiation` fires once between ref negotiation and the pack upload; returning an
    // error there is libgit2's documented way to cancel a push, and it is the **only** point a
    // push is cancelled. Nothing has been sent yet, so the remote is guaranteed untouched.
    //
    // After it, the push is committed: git2 0.21's `push_transfer_progress` returns `()` and
    // cannot abort, and `push_update_reference`/`sideband_progress` run from the server's
    // report-status — i.e. *after* the remote already applied the update. Aborting there would
    // report "cancelled" for a push that actually landed (and skip the local tracking-ref
    // update), so a cancel pressed after negotiation is deliberately ignored and the push
    // reports its real outcome.
    let cancelled_at_negotiation = Cell::new(false);
    let stale_lease = Cell::new(false);
    callbacks.push_negotiation(|updates| {
        if cancel() {
            cancelled_at_negotiation.set(true);
            return Err(git2::Error::from_str("transfer cancelled"));
        }
        // Force-with-lease: `src()` is what the remote advertised for the destination ref
        // (zero when absent). This runs after advertisement and before any pack data is sent,
        // so refusing here leaves the remote untouched — the same window
        // `git push --force-with-lease` has.
        if let Some((dst, expected)) = lease {
            let current = updates
                .iter()
                .find(|update| update.dst_refname() == Ok(dst))
                .map(|update| update.src())
                .unwrap_or(git2::Oid::ZERO_SHA1);
            if current != expected {
                stale_lease.set(true);
                return Err(git2::Error::from_str("stale lease"));
            }
        }
        // git runs pre-push at exactly this point: refs advertised (so `src()` is the live
        // remote SHA), nothing sent yet. A stale lease was rejected above first, as git does.
        if run_pre_push {
            reporter.borrow_mut().report(TransferProgress {
                operation_id: String::new(),
                operation,
                phase: TransferPhase::RunningHook,
                current: 0,
                total: 0,
                received_bytes: 0,
                message: None,
            });
            let input = pre_push_input(updates);
            let args = [remote_name, push_url.as_str()];
            let invocation = crate::hooks::HookInvocation {
                name: "pre-push",
                args: &args,
                stdin: Some(input.as_bytes()),
                env: &[],
            };
            match crate::hooks::run_hook(repo, &invocation, cancel, crate::hooks::HOOK_TIMEOUT) {
                Ok(outcome) => {
                    if let Some(output) =
                        crate::hooks::failure_output(&outcome, crate::hooks::HOOK_TIMEOUT)
                    {
                        *hook_failure.borrow_mut() = Some(RemoteError::HookFailed {
                            hook: "pre-push".to_string(),
                            output,
                        });
                        return Err(git2::Error::from_str("pre-push hook failed"));
                    }
                }
                Err(crate::hooks::HookError::Cancelled { .. }) => {
                    cancelled_at_negotiation.set(true);
                    return Err(git2::Error::from_str("transfer cancelled"));
                }
                Err(error) => {
                    *hook_failure.borrow_mut() = Some(RemoteError::Hook(error));
                    return Err(git2::Error::from_str("pre-push hook could not run"));
                }
            }
        }
        Ok(())
    });
    callbacks.push_transfer_progress(|current, total, transferred_bytes| {
        reporter.borrow_mut().report(TransferProgress {
            operation_id: String::new(),
            operation,
            phase: TransferPhase::Receiving,
            current,
            total,
            received_bytes: transferred_bytes,
            message: None,
        });
    });
    callbacks.push_update_reference(|_reference, status| {
        if let Some(status) = status {
            let status = status.to_ascii_lowercase();
            let kind = if status.contains("non-fast-forward") || status.contains("nonfastforward") {
                TransferErrorKind::NonFastForward
            } else {
                TransferErrorKind::RejectedRemoteRef
            };
            if rejection.get() != Some(TransferErrorKind::NonFastForward) {
                rejection.set(Some(kind));
            }
            return Ok(());
        }
        reporter.borrow_mut().report(TransferProgress {
            operation_id: String::new(),
            operation,
            phase: TransferPhase::Updating,
            current: 0,
            total: 0,
            received_bytes: 0,
            message: None,
        });
        Ok(())
    });

    let mut options = PushOptions::new();
    options.remote_callbacks(callbacks);
    let pushed = remote.push(refspecs, Some(&mut options));
    // Only a negotiation-time abort is a cancellation; a genuine failure that happens while
    // Cancel is pressed keeps its real classification.
    if let Err(error) = pushed {
        return if stale_lease.get() {
            Err(RemoteError::StaleLease)
        } else if cancelled_at_negotiation.get() {
            Err(RemoteError::Cancelled)
        } else if let Some(failure) = hook_failure.borrow_mut().take() {
            Err(failure)
        } else if error.code() == ErrorCode::NotFastForward {
            Err(RemoteError::NonFastForward)
        } else {
            Err(error.into())
        };
    }
    match rejection.get() {
        Some(TransferErrorKind::NonFastForward) => Err(RemoteError::NonFastForward),
        Some(TransferErrorKind::RejectedRemoteRef) => Err(RemoteError::RejectedRemoteRef),
        // A lease failure aborts at negotiation, never as a per-ref rejection.
        Some(TransferErrorKind::StaleLease) => unreachable!(),
        Some(TransferErrorKind::MissingCredential) => unreachable!(),
        Some(TransferErrorKind::CredentialStoreFailure) => unreachable!(),
        Some(TransferErrorKind::SshAgentFailure) => unreachable!(),
        Some(TransferErrorKind::TransferFailed) => unreachable!(),
        // Cancellation aborts the `push` call itself (above); it is never recorded as a
        // per-reference rejection.
        Some(TransferErrorKind::Cancelled) => unreachable!(),
        // A hook failure aborts negotiation, never a per-ref rejection.
        Some(TransferErrorKind::HookFailed) => unreachable!(),
        None => Ok(()),
    }
}

/// git's pre-push stdin: `<local ref> <local sha> <remote ref> <remote sha>` per ref that will
/// change. `src()` is what the remote advertised (zero when the ref is new), `dst()` the new
/// value (zero for a deletion, whose local ref is `(delete)`).
fn pre_push_input(updates: &[git2::PushUpdate<'_>]) -> String {
    let mut input = String::new();
    for update in updates {
        if update.src() == update.dst() {
            continue;
        }
        let local_ref = if update.dst().is_zero() {
            "(delete)"
        } else {
            update.src_refname().unwrap_or("")
        };
        let remote_ref = update.dst_refname().unwrap_or("");
        input.push_str(&format!(
            "{local_ref} {} {remote_ref} {}\n",
            update.dst(),
            update.src()
        ));
    }
    input
}

fn validate_tag_name(name: &str) -> Result<(), RemoteError> {
    if name.starts_with('+') || !Tag::is_valid_name(name) {
        return Err(git2::Error::from_str("invalid tag name").into());
    }
    Ok(())
}

fn worktree_is_clean(repo: &Repository) -> Result<bool, RemoteError> {
    let mut options = StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    Ok(repo.statuses(Some(&mut options))?.is_empty())
}

fn validate_urls(fetch_url: &str, push_url: Option<&str>) -> Result<(), RemoteError> {
    if contains_embedded_credentials(fetch_url)
        || push_url.is_some_and(contains_embedded_credentials)
    {
        return Err(RemoteError::CredentialBearingUrl);
    }
    Ok(())
}

fn contains_embedded_credentials(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    !parsed.username().is_empty() || parsed.password().is_some()
}

pub fn remote_auth_profile(
    repo: &Repository,
    remote: &str,
) -> Result<Option<RemoteAuthMode>, RemoteError> {
    let config = repo.config()?;
    let auth_mode = match config.get_string(&remote_auth_key(remote, "auth-mode")) {
        Ok(auth_mode) => auth_mode,
        Err(error) if error.code() == ErrorCode::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    match auth_mode.as_str() {
        "https-token" => Ok(Some(RemoteAuthMode::HttpsToken {
            username: config.get_string(&remote_auth_key(remote, "username"))?,
        })),
        "ssh-agent" => Ok(Some(RemoteAuthMode::SshAgent)),
        _ => Ok(None),
    }
}

fn remote_auth_key(remote: &str, field: &str) -> String {
    format!("browsitory.remote.{remote}.{field}")
}

fn remove_config_key(config: &mut git2::Config, key: &str) -> Result<(), RemoteError> {
    match config.remove(key) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ErrorCode::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn local_branches_using_remote(
    repo: &Repository,
    remote_name: &str,
) -> Result<Vec<String>, RemoteError> {
    let config = repo.config()?;
    let mut branches = Vec::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        let remote_key = format!("branch.{name}.remote");
        match config.get_string(&remote_key) {
            Ok(configured_remote) if configured_remote == remote_name => {
                branches.push(name.to_string())
            }
            Ok(_) => {}
            Err(error) if error.code() == ErrorCode::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(branches)
}
