//! A write that another Git program's lock file stopped (#1033): `held_lock::is_lock_held`
//! tells it from every other failure by libgit2's error code, so the worker can give it a kind
//! of its own and the frontend a precise next step.

mod common;

use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git_core::held_lock::is_lock_held;

/// Holds `name` (a path in the git folder) as another Git program would while it writes.
fn hold(repo: &git2::Repository, name: &str) {
    std::fs::write(repo.path().join(format!("{name}.lock")), "").unwrap();
}

/// git refuses `args` too, on the same lock.
fn git_refuses_on_the_lock(dir: &Path, args: &[&str], lock: &str) {
    let output = test_support::git(dir, args);
    assert!(!output.status.success(), "git {args:?} succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(lock), "git {args:?}: {stderr}");
}

fn repo_with_a_commit() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    (dir, repo)
}

#[test]
fn staging_while_the_index_is_locked_is_a_held_lock() {
    let (dir, repo) = repo_with_a_commit();
    write_file(dir.path(), "a.txt", "a, edited\n");
    hold(&repo, "index");

    let error = git_core::stage::stage_file(&repo, "a.txt").unwrap_err();

    assert!(is_lock_held(&error), "{error}");
    git_refuses_on_the_lock(dir.path(), &["add", "a.txt"], "index.lock");
}

#[test]
fn creating_a_branch_whose_ref_is_locked_is_a_held_lock() {
    let (dir, repo) = repo_with_a_commit();
    std::fs::create_dir_all(repo.path().join("refs/heads")).unwrap();
    hold(&repo, "refs/heads/feature");

    let error = git_core::branch::create_branch_only(&repo, "feature", "HEAD").unwrap_err();

    assert!(is_lock_held(&error), "{error}");
    git_refuses_on_the_lock(dir.path(), &["branch", "feature"], "feature.lock");
}

#[test]
fn creating_a_tag_whose_ref_is_locked_is_a_held_lock() {
    let (dir, repo) = repo_with_a_commit();
    std::fs::create_dir_all(repo.path().join("refs/tags")).unwrap();
    hold(&repo, "refs/tags/v1");

    let error = git_core::remote::create_tag(&repo, "v1", None, None).unwrap_err();

    assert!(is_lock_held(&error), "{error}");
    git_refuses_on_the_lock(dir.path(), &["tag", "v1"], "v1.lock");
}

#[test]
fn changing_the_config_while_it_is_locked_is_a_held_lock() {
    let (dir, repo) = repo_with_a_commit();
    hold(&repo, "config");

    let error = git_core::remote::add_remote(&repo, "origin", "https://example.test/r.git", None)
        .unwrap_err();

    assert!(is_lock_held(&error), "{error}");
    git_refuses_on_the_lock(
        dir.path(),
        &["remote", "add", "origin", "https://example.test/r.git"],
        "config.lock",
    );
}

/// libgit2 gives a file the system refused to remove or replace the same code (`Locked`, "failed
/// remove - '…' is locked"): on Windows another program holds it open, elsewhere it is `EACCES`.
/// That is no lock file of another Git program.
#[test]
fn a_file_the_system_refused_is_no_held_lock() {
    let refused = git2::Error::new(
        git2::ErrorCode::Locked,
        git2::ErrorClass::Os,
        "failed remove - '/work/a.txt' is locked",
    );
    assert!(!is_lock_held(&refused));
}

#[test]
fn other_failures_are_no_held_lock() {
    let (_dir, repo) = repo_with_a_commit();
    let missing = git_core::branch::create_branch_only(&repo, "feature", "nope").unwrap_err();
    assert!(!is_lock_held(&missing), "{missing}");
    let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    assert!(!is_lock_held(&denied));
    // A worktree locked with `git worktree lock` is no lock file either.
    let worktree = git2::Error::new(
        git2::ErrorCode::Locked,
        git2::ErrorClass::Worktree,
        "worktree is locked",
    );
    assert!(!is_lock_held(&worktree));
}
