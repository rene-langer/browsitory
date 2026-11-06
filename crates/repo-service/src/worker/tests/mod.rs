use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use git2::Repository;
use git_core::forge::ForgeProvider;
use git_core::merge::MergeOutcome;
use git_core::remote::{TransferErrorKind, TransferOperation};
use tempfile::TempDir;

#[cfg(unix)]
use super::HookEvent;
use super::{Command, TransferEvent, UndoOutcome, UndoState, Worker, WorkerRegistry};
use crate::credentials::{CredentialKey, CredentialStore, CredentialStoreError};
use crate::pull_requests::{ForgeApiError, ForgeHttpRequest, ForgeHttpResponse};

mod basics;
mod branch;
mod cancel;
mod commit;
mod forge;
mod history;
#[cfg(unix)]
mod hooks;
mod merge;
mod rebase;
mod remote;
mod restart;
mod search;
mod sequence;
mod staging;
mod stash;
mod undo;

/// A hook event sender for commands whose hook progress a test doesn't look at.
fn no_events() -> mpsc::Sender<super::HookEvent> {
    mpsc::channel().0
}

fn ok(status: u16, body: &str) -> Result<ForgeHttpResponse, ForgeApiError> {
    Ok(ForgeHttpResponse {
        status,
        body: body.to_string(),
        headers: Vec::new(),
    })
}

fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().expect("create temp dir");
    let repo = Repository::init(dir.path()).expect("init repo");
    {
        let mut config = repo.config().expect("repo config");
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("core.autocrlf", false).unwrap();
        config.set_str("core.eol", "lf").unwrap();
        config.set_bool("core.safecrlf", false).unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
        // Pin the hooks dir (git's default) so a global `core.hooksPath` never runs here.
        let hooks = repo.path().join("hooks");
        config
            .set_str("core.hooksPath", hooks.to_str().expect("utf-8 temp path"))
            .unwrap();
    }
    (dir, repo)
}

#[cfg(unix)]
fn install_hook(repo: &Repository, name: &str, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    let dir = git_core::hooks::hooks_dir(repo).expect("hooks dir");
    std::fs::create_dir_all(&dir).expect("create hooks dir");
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write hook");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn write_file(dir: &Path, relative_path: &str, contents: &str) {
    std::fs::write(dir.join(relative_path), contents).expect("write file");
}

/// Stages everything in the worktree and commits it on `HEAD`, creating the first commit
/// when there is none yet. Mirrors `crates/git-core/tests/common/mod.rs::commit_all`.
fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().expect("open index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("stage all");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let signature = repo.signature().expect("signature");

    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .expect("commit");
}

fn local_and_bare_remote() -> (TempDir, TempDir, TempDir) {
    let (source_dir, source) = init_repo();
    write_file(source_dir.path(), "README.md", "initial commit\n");
    commit_all(&source, "initial commit");

    let remote_dir = TempDir::new().expect("create bare remote");
    let remote = Repository::init_bare(remote_dir.path()).expect("init bare remote");
    let branch = source
        .head()
        .expect("source head")
        .shorthand()
        .expect("source branch")
        .to_string();
    let branch_ref = format!("refs/heads/{branch}");
    source
        .remote("origin", remote_dir.path().to_str().expect("remote path"))
        .expect("add source remote");
    source
        .find_remote("origin")
        .expect("source remote")
        .push(&[format!("{branch_ref}:{branch_ref}")], None)
        .expect("push source commit");
    drop(remote);

    let (local_dir, local) = init_repo();
    local
        .remote("origin", remote_dir.path().to_str().expect("remote path"))
        .expect("add local remote");

    (source_dir, remote_dir, local_dir)
}

/// base: a.txt="base\n". feature: "f one" (+b.txt), "f two" (a.txt="feature\n"),
/// "f three" (+c.txt). The base branch (checked out): a.txt="main\n". Returns the feature
/// ids oldest-first.
pub(super) fn side_commits_with_a_conflict() -> (TempDir, Repository, Vec<String>) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    let mut ids = Vec::new();
    for (path, contents, message) in [
        ("b.txt", "b\n", "f one"),
        ("a.txt", "feature\n", "f two"),
        ("c.txt", "c\n", "f three"),
    ] {
        write_file(dir.path(), path, contents);
        commit_all(&repo, message);
        ids.push(repo.head().unwrap().target().unwrap().to_string());
    }
    repo.set_head(&format!("refs/heads/{base_branch}")).unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .force()
            .remove_untracked(true),
    ))
    .unwrap();
    write_file(dir.path(), "a.txt", "main\n");
    commit_all(&repo, "main change");
    (dir, repo, ids)
}

fn sequence_file(repo: &Repository) -> std::path::PathBuf {
    repo.path().join("browsitory").join("sequence.json")
}

/// A cherry-pick sequence of `side_commits_with_a_conflict` stopped on its conflict (#171).
/// Returns the branch it runs on (short name).
fn paused_sequence() -> (TempDir, Repository, Worker, super::WorkerHandle, String) {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert!(matches!(
        handle.cherry_pick_many(ids, no_events()).unwrap(),
        git_core::pick::SequenceOutcome::Conflicted { .. }
    ));
    (dir, repo, worker, handle, branch)
}

fn completed(events: &[TransferEvent]) -> (TransferOperation, Option<TransferErrorKind>) {
    match events.last() {
        Some(TransferEvent::Completed {
            operation, error, ..
        }) => (*operation, *error),
        other => panic!("expected a Completed event last, got {other:?}"),
    }
}

/// `base` → `feature` and `main` both rewrite `a.txt`, so merging `feature` conflicts.
fn conflicting_branches() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    write_file(dir.path(), "a.txt", "feature\n");
    commit_all(&repo, "feature change");
    repo.set_head(&format!("refs/heads/{base_branch}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "a.txt", "main\n");
    commit_all(&repo, "main change");
    (dir, repo)
}

// --- Restart resilience (#108, #134) ----------------------------------------------------

/// An app restart: drop the worker and every handle (held replies mean everything was
/// persisted before the last reply), then start a fresh worker on the same path.
fn restart(
    worker: Worker,
    handle: super::WorkerHandle,
    path: &Path,
) -> (Worker, super::WorkerHandle) {
    drop(handle);
    drop(worker);
    let worker = Worker::spawn(path.to_path_buf()).unwrap();
    let handle = worker.handle();
    (worker, handle)
}

/// "Step N of M" of the stored rebase, as `HeadInfo.rebase` reports it.
fn rebase_progress(handle: &super::WorkerHandle) -> Option<(usize, usize)> {
    handle
        .get_head_info()
        .unwrap()
        .rebase
        .map(|rebase| (rebase.current_step, rebase.total_steps))
}

/// The folder `message` ("branch '<branch>' <how> in another worktree at '<folder>'", `how`
/// like "is checked out") names, canonicalized.
fn held_folder(message: &str, branch: &str, how: &str) -> std::path::PathBuf {
    let prefix = format!("branch '{branch}' {how} in another worktree at '");
    let folder = message
        .strip_prefix(&prefix)
        .and_then(|rest| rest.strip_suffix('\''))
        .unwrap_or_else(|| panic!("{message}"));
    std::fs::canonicalize(folder).unwrap()
}
