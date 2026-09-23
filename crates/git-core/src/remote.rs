use std::cell::{Cell, RefCell};

use git2::{
    build::CheckoutBuilder, BranchType, Cred, CredentialType, ErrorCode, FetchOptions, PushOptions,
    RemoteCallbacks, Repository, StatusOptions, Tag,
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
    Pull,
    PushBranch,
    PushTags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferPhase {
    Receiving,
    Updating,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferErrorKind {
    NonFastForward,
    RejectedRemoteRef,
    MissingCredential,
    CredentialStoreFailure,
    SshAgentFailure,
    TransferFailed,
    /// The caller asked for the in-flight transfer to stop — see the `cancel` parameter on
    /// `fetch_remote`/`push_current_branch`/`push_tags`.
    Cancelled,
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
}

/// Ahead/behind of `local_branch` against `refs/remotes/<remote>/<remote_branch>`, or
/// `(None, None)` when either side cannot be resolved.
fn ahead_behind(
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
    pub target_id: String,
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
    #[error("the transfer was cancelled")]
    Cancelled,
}

impl RemoteError {
    pub fn transfer_error_kind(&self) -> TransferErrorKind {
        match self {
            Self::Cancelled => TransferErrorKind::Cancelled,
            Self::NonFastForward => TransferErrorKind::NonFastForward,
            Self::RejectedRemoteRef => TransferErrorKind::RejectedRemoteRef,
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
    operation_id: String,
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
            operation_id: operation_id.clone(),
            operation: TransferOperation::Fetch,
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
            operation_id: operation_id.clone(),
            operation: TransferOperation::Fetch,
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
            operation_id: operation_id.clone(),
            operation: TransferOperation::Fetch,
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
        if let Ok(tag) = repo.find_tag(target_id) {
            let tagger = tag.tagger();
            tags.push(TagInfo {
                name: name.to_string(),
                target_id: tag.target_id().to_string(),
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

/// Pushes the current branch. `cancel` is checked once, at `push_negotiation` (before any data
/// is sent); a cancel requested after that is ignored so a push that lands is never reported as
/// cancelled — see `push_refs`.
pub fn push_current_branch(
    repo: &Repository,
    remote_name: &str,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    let branch = current_local_branch_name(repo)?;
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    push_refs(
        repo,
        remote_name,
        &[refspec],
        TransferOperation::PushBranch,
        credentials,
        reporter,
        cancel,
    )
}

/// Pushes `names` (or every local tag when empty). `cancel` behaves as for
/// `push_current_branch`.
pub fn push_tags(
    repo: &Repository,
    remote_name: &str,
    names: &[String],
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
    let remote_key = format!("branch.{local_branch}.remote");
    let remote_name = match config.get_string(&remote_key) {
        Ok(name) => name,
        Err(error) if error.code() == ErrorCode::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let merge_ref = config.get_string(&format!("branch.{local_branch}.merge"))?;
    let remote_branch = merge_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(&merge_ref)
        .to_string();

    let (ahead, behind) = ahead_behind(repo, &local_branch, &remote_name, &remote_branch);
    Ok(Some(UpstreamInfo {
        local_branch,
        remote_name,
        remote_branch,
        ahead,
        behind,
    }))
}

pub fn set_current_upstream(
    repo: &Repository,
    remote_name: &str,
    remote_branch: &str,
) -> Result<(), RemoteError> {
    repo.find_remote(remote_name)?;
    let local_branch = current_local_branch_name(repo)?;
    let mut branch = repo.find_branch(&local_branch, BranchType::Local)?;
    let upstream = format!("{remote_name}/{remote_branch}");
    match branch.set_upstream(Some(&upstream)) {
        Ok(()) => {}
        Err(error) if error.code() == ErrorCode::NotFound => {
            let mut config = repo.config()?;
            config.set_str(&format!("branch.{local_branch}.remote"), remote_name)?;
            config.set_str(
                &format!("branch.{local_branch}.merge"),
                &format!("refs/heads/{remote_branch}"),
            )?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
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
        upstreams.push(UpstreamInfo {
            local_branch: local_branch.to_string(),
            remote_name: remote_name.to_string(),
            remote_branch,
            ahead,
            behind,
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

fn push_refs(
    repo: &Repository,
    remote_name: &str,
    refspecs: &[String],
    operation: TransferOperation,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<(), RemoteError> {
    let mut remote = repo.find_remote(remote_name)?;
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
    callbacks.push_negotiation(|_updates| {
        if cancel() {
            cancelled_at_negotiation.set(true);
            return Err(git2::Error::from_str("transfer cancelled"));
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
        return if cancelled_at_negotiation.get() {
            Err(RemoteError::Cancelled)
        } else if error.code() == ErrorCode::NotFastForward {
            Err(RemoteError::NonFastForward)
        } else {
            Err(error.into())
        };
    }
    match rejection.get() {
        Some(TransferErrorKind::NonFastForward) => Err(RemoteError::NonFastForward),
        Some(TransferErrorKind::RejectedRemoteRef) => Err(RemoteError::RejectedRemoteRef),
        Some(TransferErrorKind::MissingCredential) => unreachable!(),
        Some(TransferErrorKind::CredentialStoreFailure) => unreachable!(),
        Some(TransferErrorKind::SshAgentFailure) => unreachable!(),
        Some(TransferErrorKind::TransferFailed) => unreachable!(),
        // Cancellation aborts the `push` call itself (above); it is never recorded as a
        // per-reference rejection.
        Some(TransferErrorKind::Cancelled) => unreachable!(),
        None => Ok(()),
    }
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
