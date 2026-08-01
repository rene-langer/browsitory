//! Shared progress-reporting types for push/fetch/pull.
//!
//! `fetch`/`pull` and `push`/`push_tag` are added on top of these by
//! separate workstreams (Phase 3); this module intentionally starts out
//! holding only the types both sides need, so neither workstream's branch
//! has to fight the other over ownership of this file's function bodies.

use git2::{FetchOptions, PushOptions, Repository};

use crate::credentials;
use crate::merge::{self, MergeOutcome};
use crate::repo::{GitError, Result};

/// A snapshot of transfer progress, cheap to copy so it can be sent through
/// an `mpsc` channel on every libgit2 progress callback invocation (which
/// can fire many times per second) without allocating.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProgressUpdate {
    pub received_objects: usize,
    pub total_objects: usize,
    pub indexed_objects: usize,
    pub received_bytes: usize,
    pub stage: TransferStage,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransferStage {
    #[default]
    Negotiating,
    Receiving,
    Indexing,
    Pushing,
    Done,
}

/// Fetches from `remote_name`, using the remote's own configured refspecs
/// (e.g. `+refs/heads/*:refs/remotes/origin/*`) so this behaves like a plain
/// `git fetch <remote>` — passing an empty refspec slice tells libgit2 to
/// fall back to those configured refspecs rather than requiring a caller to
/// know/re-derive them.
///
/// This only updates remote-tracking refs (`refs/remotes/{remote}/*`) and
/// `FETCH_HEAD` — it never touches a local branch. See `pull` below for the
/// fetch-then-merge combination that does.
pub fn fetch(
    repo: &Repository,
    remote_name: &str,
    mut on_progress: impl FnMut(ProgressUpdate),
) -> Result<()> {
    let mut remote = repo.find_remote(remote_name)?;

    let mut callbacks = credentials::make_callbacks(repo);
    callbacks.transfer_progress(move |progress| {
        let stage = if progress.indexed_objects() < progress.total_objects() {
            TransferStage::Receiving
        } else {
            TransferStage::Indexing
        };
        on_progress(ProgressUpdate {
            received_objects: progress.received_objects(),
            total_objects: progress.total_objects(),
            indexed_objects: progress.indexed_objects(),
            received_bytes: progress.received_bytes(),
            stage,
        });
        true
    });

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    remote.fetch(&[] as &[&str], Some(&mut fetch_options), None)?;
    Ok(())
}

/// Fetches from `remote_name`, then merges the updated `refs/remotes/{remote_name}/{branch}`
/// remote-tracking ref into the current branch, reusing `merge::merge_branch` so a
/// conflicting pull produces the exact same `MergeOutcome`/conflict-index behavior a
/// conflicting local merge does — the existing Phase 2 conflict-resolution UI just works
/// for pull too, with no new conflict UI needed.
pub fn pull(
    repo: &Repository,
    remote_name: &str,
    branch: &str,
    on_progress: impl FnMut(ProgressUpdate),
) -> Result<MergeOutcome> {
    fetch(repo, remote_name, on_progress)?;

    let tracking_ref = format!("refs/remotes/{remote_name}/{branch}");
    merge::merge_branch(repo, &tracking_ref)
}

/// Pushes `refspecs` (already fully built by the caller — e.g.
/// `refs/heads/{local}:refs/heads/{remote}`, or force-prefixed
/// `+refs/heads/{local}:refs/heads/{remote}`; `push_tag` below builds a
/// `refs/tags/{tag}:refs/tags/{tag}` one) to `remote_name`. Never pass an
/// empty slice expecting "push everything" — that silently pushes nothing
/// unless the remote has configured push refspecs of its own.
///
/// `Remote::push()` returning `Ok(())` does NOT mean every ref updated —
/// non-fast-forward (and other) rejections surface only via the
/// `push_update_reference` callback, which libgit2 calls once per ref with
/// `Some(status_msg)` when the server rejected that ref. This collects any
/// such rejections and turns them into an `Err` rather than reporting
/// success.
pub fn push(
    repo: &Repository,
    remote_name: &str,
    refspecs: &[String],
    mut on_progress: impl FnMut(ProgressUpdate),
) -> Result<()> {
    let mut remote = repo.find_remote(remote_name)?;

    let mut rejections: Vec<String> = Vec::new();
    let mut callbacks = credentials::make_callbacks(repo);
    callbacks.push_update_reference(|refname, status| {
        if let Some(msg) = status {
            rejections.push(format!("{refname}: {msg}"));
        }
        Ok(())
    });
    callbacks.push_transfer_progress(|current, total, bytes| {
        on_progress(ProgressUpdate {
            received_objects: current,
            total_objects: total,
            indexed_objects: current,
            received_bytes: bytes,
            stage: TransferStage::Pushing,
        });
    });

    let mut opts = PushOptions::new();
    opts.remote_callbacks(callbacks);

    remote.push(refspecs, Some(&mut opts))?;

    // `opts` (and the `callbacks` it owns) still holds the closures that
    // mutably borrow `rejections`/`on_progress` after `remote.push` returns
    // — `&mut opts` only lends it for the call, it isn't consumed. Drop it
    // explicitly to end those borrows before reading `rejections` or
    // calling `on_progress` again below.
    drop(opts);

    if !rejections.is_empty() {
        return Err(GitError::Rejected(rejections.join("; ")));
    }

    on_progress(ProgressUpdate {
        stage: TransferStage::Done,
        ..Default::default()
    });
    Ok(())
}

/// Pushes a single tag ref to `remote_name`.
pub fn push_tag(
    repo: &Repository,
    remote_name: &str,
    tag: &str,
    on_progress: impl FnMut(ProgressUpdate),
) -> Result<()> {
    let refspec = format!("refs/tags/{tag}:refs/tags/{tag}");
    push(repo, remote_name, &[refspec], on_progress)
}
