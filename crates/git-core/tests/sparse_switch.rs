//! #1191: in a sparse checkout, switching to a branch (or checking out a commit, or pulling)
//! that changes, adds or deletes files outside the cone works as with git: a changed file keeps
//! its skip-worktree mark with the new blob in the index, a new one is marked, and neither is
//! written. The git CLI builds every repository; a twin built the same way runs the same git
//! command and is the reference: the index with its marks, the status, HEAD and its tree, and
//! the files on disk.

mod common;

use std::path::Path;

use common::{git_ok, write_file};
use git_core::branch::{checkout_commit, stash_and_switch, switch_branch, switch_conflicts};
use git_core::branch::{BranchError, StashSwitchOutcome};
use git_core::remote::{pull_after_fetch, PullOutcome};
use git_core::undo::{capture, restore, SnapshotScope};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn ls_files_v(dir: &Path) -> String {
    git_out(dir, &["ls-files", "-v"])
}

/// Every file of the work tree but `.git`, with its content, sorted by path.
fn files(dir: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                let rel = rel.to_string_lossy().replace('\\', "/");
                out.push((rel, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The repository's index with its marks, its status, HEAD (symbolic or not) and its tree,
/// and its files.
fn state(dir: &Path) -> (String, String, String, String, Vec<(String, String)>) {
    let head = test_support::git(dir, &["symbolic-ref", "-q", "HEAD"]);
    (
        git_out(dir, &["ls-files", "-v", "-s"]),
        git_out(dir, &["status", "--short"]),
        format!(
            "{} {}",
            String::from_utf8_lossy(&head.stdout).trim(),
            git_out(dir, &["rev-parse", "HEAD"]).trim()
        ),
        git_out(dir, &["rev-parse", "HEAD^{tree}"]),
        files(dir),
    )
}

/// Our repository `ours` ends as git left `twin`.
fn assert_same_as_git(ours: &Path, twin: &Path) {
    let (ours, twin) = (state(ours), state(twin));
    assert_eq!(ours.0, twin.0, "ls-files -v -s");
    assert_eq!(ours.1, twin.1, "status --short");
    assert_eq!(ours.2, twin.2, "HEAD");
    assert_eq!(ours.3, twin.3, "HEAD's tree");
    assert_eq!(ours.4, twin.4, "files");
}

fn init(d: &Path) {
    git_ok(d, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Test User"),
        ("user.email", "test@example.com"),
        ("core.autocrlf", "false"),
        ("commit.gpgSign", "false"),
    ] {
        git_ok(d, &["config", key, value]);
    }
}

/// Commits at a fixed time, so a twin's commits have the same ids.
fn commit(d: &Path, message: &str) {
    git_ok(d, &["add", "-A"]);
    let output = test_support::git_command(d)
        .args(["commit", "-q", "-m", message])
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
}

const MARKS: &str = "H app/main.rs\nS docs/guide.md\nS lib/other/o.rs\nH top.txt\n";

/// `app/main.rs`, `docs/guide.md`, `lib/other/o.rs` and `top.txt` committed on `main`; `side`
/// changes `app/main.rs` and `docs/guide.md`, adds `docs/new.md` and deletes `lib/other/o.rs`;
/// `adds` only adds `docs/new.md`. `refs/remotes/origin/main` is `side`, the upstream of
/// `main`. Then `git sparse-checkout set app` (cone mode) on `main`.
fn build(d: &Path) {
    init(d);
    for (path, content) in [
        ("app/main.rs", "fn main() {}\n"),
        ("docs/guide.md", "guide\n"),
        ("lib/other/o.rs", "other\n"),
        ("top.txt", "top\n"),
    ] {
        write_file(d, path, content);
    }
    commit(d, "base");
    git_ok(d, &["checkout", "-q", "-b", "side"]);
    write_file(d, "app/main.rs", "fn main() { side() }\n");
    write_file(d, "docs/guide.md", "guide from side\n");
    write_file(d, "docs/new.md", "new\n");
    std::fs::remove_file(d.join("lib/other/o.rs")).unwrap();
    commit(d, "side");
    git_ok(d, &["checkout", "-q", "-b", "adds", "main"]);
    write_file(d, "docs/new.md", "new\n");
    commit(d, "adds");
    git_ok(d, &["checkout", "-q", "main"]);
    git_ok(d, &["update-ref", "refs/remotes/origin/main", "side"]);
    git_ok(d, &["config", "branch.main.remote", "origin"]);
    git_ok(d, &["config", "branch.main.merge", "refs/heads/main"]);
    git_ok(d, &["sparse-checkout", "set", "app"]);
    assert_eq!(ls_files_v(d), MARKS);
}

fn twins() -> (tempfile::TempDir, tempfile::TempDir) {
    let ours = tempfile::TempDir::new().unwrap();
    let twin = tempfile::TempDir::new().unwrap();
    build(ours.path());
    build(twin.path());
    (ours, twin)
}

fn open(dir: &Path) -> git2::Repository {
    git2::Repository::open(dir).unwrap()
}

/// Nothing outside the cone is on disk.
fn assert_nothing_written_outside(d: &Path) {
    assert!(!d.join("docs").exists(), "{:?}", files(d));
    assert!(!d.join("lib").exists(), "{:?}", files(d));
}

#[test]
fn switching_to_a_branch_that_changes_files_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["switch", "-q", "side"]);

    switch_branch(&open(ours.path()), "side").unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
    assert!(ls_files_v(ours.path()).contains("S docs/new.md\n"));
}

#[test]
fn switching_to_a_branch_that_only_adds_a_file_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["switch", "-q", "adds"]);

    switch_branch(&open(ours.path()), "adds").unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

#[test]
fn switching_there_and_back_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["switch", "-q", "side"]);
    git_ok(twin.path(), &["switch", "-q", "main"]);
    let repo = open(ours.path());

    switch_branch(&repo, "side").unwrap();
    switch_branch(&repo, "main").unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_eq!(ls_files_v(ours.path()), MARKS);
    assert_nothing_written_outside(ours.path());
}

#[test]
fn checking_out_a_commit_that_changes_files_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["checkout", "-q", "--detach", "side"]);
    let side = git_out(ours.path(), &["rev-parse", "side"]);

    checkout_commit(&open(ours.path()), side.trim()).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

/// A local change inside the cone that the switch would overwrite still refuses it, and only
/// that file is named: the missing files outside the cone aren't local changes.
#[test]
fn a_refused_switch_names_only_the_changed_file_inside_the_cone() {
    let (ours, _twin) = twins();
    write_file(ours.path(), "app/main.rs", "fn main() { mine() }\n");
    let repo = open(ours.path());
    let before = state(ours.path());

    let result = switch_branch(&repo, "side");

    assert!(
        matches!(result, Err(BranchError::LocalChanges(ref name)) if name == "side"),
        "{result:?}"
    );
    assert_eq!(state(ours.path()), before);
    assert_eq!(
        switch_conflicts(&repo, "side").unwrap(),
        vec!["app/main.rs".to_string()]
    );
}

#[test]
fn stash_and_switch_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins();
    for d in [ours.path(), twin.path()] {
        write_file(d, "app/main.rs", "fn main() { mine() }\n");
    }
    git_ok(twin.path(), &["stash", "push", "-q"]);
    git_ok(twin.path(), &["switch", "-q", "adds"]);
    git_ok(twin.path(), &["stash", "pop", "-q"]);

    let outcome = stash_and_switch(&mut open(ours.path()), "adds", true).unwrap();

    assert_eq!(outcome, StashSwitchOutcome::Reapplied);
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

/// Undo of the switch puts the repository back as it was: the marks and nothing on disk.
#[test]
fn undo_of_a_switch_outside_the_cone_restores_the_sparse_checkout() {
    let (ours, twin) = twins();
    let repo = open(ours.path());
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    switch_branch(&repo, "side").unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_eq!(ls_files_v(ours.path()), MARKS);
    assert_nothing_written_outside(ours.path());
}

/// A fast-forward pull (`origin/main` is `side`), as `git merge --ff-only` does it: the missing
/// files outside the cone aren't local changes either (#1152).
#[test]
fn a_pull_that_changes_files_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["merge", "-q", "--ff-only", "origin/main"]);

    let outcome = pull_after_fetch(&open(ours.path()), "origin", "main").unwrap();

    assert!(
        matches!(outcome, PullOutcome::FastForwarded { .. }),
        "{outcome:?}"
    );
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

/// Non-cone patterns aren't matched: a path new to the index is outside them when its file is
/// missing, so `docs/new.md` is marked, as git marks it here.
#[test]
fn switching_with_non_cone_patterns_matches_git() {
    let (ours, twin) = twins();
    for d in [ours.path(), twin.path()] {
        git_ok(d, &["sparse-checkout", "set", "--no-cone", "/app/"]);
    }
    git_ok(twin.path(), &["switch", "-q", "side"]);

    switch_branch(&open(ours.path()), "side").unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}
