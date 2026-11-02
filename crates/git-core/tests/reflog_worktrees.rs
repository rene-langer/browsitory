//! Reflog restore and linked worktrees (#287): a branch another worktree has checked out isn't
//! moved under it, as `git branch -f` refuses. Each case is twinned with the git CLI (isolated
//! from the developer's config, #282).
mod common;

use std::path::{Path, PathBuf};

use git2::{Oid, Repository};
use git_core::reflog::restore_reflog_entry;
use tempfile::TempDir;
use test_support::git;

fn git_ok(dir: &Path, args: &[&str]) {
    let output = git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

struct Fixture {
    dir: TempDir,
    repo: Repository,
    /// Holds the linked worktree's folder.
    _elsewhere: TempDir,
    linked_path: PathBuf,
    linked: Repository,
    first: Oid,
    second: Oid,
}

/// main: "first", "second"; topic moved from "first" to "second" (both in its reflog). The main
/// worktree is on main, a linked one on topic.
fn fixture() -> Fixture {
    let (dir, repo) = common::init_repo();
    repo.set_head("refs/heads/main").unwrap();
    common::write_file(dir.path(), "history.txt", "first");
    common::commit_all(&repo, "first");
    let first = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "history.txt", "second");
    common::commit_all(&repo, "second");
    let second = repo.head().unwrap().target().unwrap();
    repo.reference("refs/heads/topic", first, false, "create")
        .unwrap();
    repo.reference("refs/heads/topic", second, true, "move")
        .unwrap();
    let elsewhere = TempDir::new().unwrap();
    let linked_path = elsewhere.path().join("linked");
    git_ok(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            linked_path.to_str().unwrap(),
            "topic",
        ],
    );
    let linked = Repository::open(&linked_path).unwrap();
    Fixture {
        dir,
        repo,
        _elsewhere: elsewhere,
        linked_path,
        linked,
        first,
        second,
    }
}

fn target(repo: &Repository, name: &str) -> Option<Oid> {
    repo.find_reference(name).unwrap().target()
}

/// git refuses to move a branch another worktree has checked out ("cannot force update the
/// branch 'topic' used by worktree at …"); so does Restore, from either worktree, and moves
/// nothing.
#[test]
fn a_branch_checked_out_in_another_worktree_isnt_restored() {
    let f = fixture();
    let first = f.first.to_string();
    let twin = git(f.dir.path(), &["branch", "-f", "topic", &first]);
    assert!(!twin.status.success(), "{twin:?}");
    assert!(String::from_utf8_lossy(&twin.stderr).contains("used by worktree"));
    let twin = git(&f.linked_path, &["branch", "-f", "main", &first]);
    assert!(!twin.status.success(), "{twin:?}");

    let from_main = restore_reflog_entry(&f.repo, "refs/heads/topic", &first).unwrap_err();
    let from_linked = restore_reflog_entry(&f.linked, "refs/heads/main", &first).unwrap_err();

    assert_eq!(
        from_main.to_string(),
        "branch 'topic' is checked out in another worktree"
    );
    assert_eq!(
        from_linked.to_string(),
        "branch 'main' is checked out in another worktree"
    );
    assert_eq!(target(&f.repo, "refs/heads/topic"), Some(f.second));
    assert_eq!(target(&f.repo, "refs/heads/main"), Some(f.second));
}

/// The branch checked out in this worktree is still restored, from either worktree, as before:
/// the ref moves and the work tree and index stay (like `git reset --soft`). git's `branch -f`
/// refuses the current branch too, but restoring it is what Reflog restore is for.
#[test]
fn the_branch_checked_out_here_is_still_restored() {
    let f = fixture();
    let first = f.first.to_string();
    assert!(!git(&f.linked_path, &["branch", "-f", "topic", &first])
        .status
        .success());

    restore_reflog_entry(&f.linked, "refs/heads/topic", &first).unwrap();
    restore_reflog_entry(&f.repo, "HEAD", &first).unwrap();

    assert_eq!(target(&f.repo, "refs/heads/topic"), Some(f.first));
    assert_eq!(target(&f.repo, "refs/heads/main"), Some(f.first));
    assert_eq!(
        common::read(f.dir.path(), "history.txt"),
        "second",
        "the work tree stays"
    );
}

/// A branch no worktree has checked out is restored as before.
#[test]
fn a_branch_nobody_has_checked_out_is_restored() {
    let f = fixture();
    let first = f.first.to_string();
    f.repo
        .reference("refs/heads/spare", f.first, false, "create")
        .unwrap();
    f.repo
        .reference("refs/heads/spare", f.second, true, "move")
        .unwrap();

    restore_reflog_entry(&f.linked, "refs/heads/spare", &first).unwrap();

    assert_eq!(target(&f.repo, "refs/heads/spare"), Some(f.first));
}
