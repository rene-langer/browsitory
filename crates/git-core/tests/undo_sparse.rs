//! #1174: Undo and Redo in a sparse checkout, or with files marked skip-worktree or added with
//! intent to add (`git add -N`): the index is restored with those marks kept, instead of the
//! restore being refused. The git CLI sets the repositories up and checks the results.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, write_file};
use git_core::undo::{capture, restore, SnapshotScope};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// `git ls-files -v`: `S` for skip-worktree, `H` otherwise.
fn ls_files_v(dir: &Path) -> String {
    git_out(dir, &["ls-files", "-v"])
}

fn status_short(dir: &Path) -> String {
    git_out(dir, &["status", "--short"])
}

const SPARSE_MARKS: &str = "H app/main.rs\nS docs/guide.md\nH top.txt\nS vendor/lib/x.c\n";

/// `app/main.rs`, `docs/guide.md`, `vendor/lib/x.c` and `top.txt` committed, then `git
/// sparse-checkout set app`.
fn sparse_checkout() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "app/main.rs", "fn main() {}\n");
    write_file(d, "docs/guide.md", "guide\n");
    write_file(d, "vendor/lib/x.c", "int x;\n");
    write_file(d, "top.txt", "top\n");
    commit_all(&repo, "base");
    git_ok(d, &["sparse-checkout", "set", "app"]);
    assert_eq!(ls_files_v(d), SPARSE_MARKS);
    (dir, repo)
}

#[test]
fn undo_and_redo_of_a_stage_in_a_sparse_checkout_keep_the_marks() {
    let (dir, repo) = sparse_checkout();
    let d = dir.path();
    write_file(d, "app/main.rs", "fn main() { run() }\n");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::stage::stage_file(&repo, "app/main.rs").unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(status_short(d), " M app/main.rs\n");
    assert_eq!(ls_files_v(d), SPARSE_MARKS);

    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(status_short(d), "M  app/main.rs\n");
    assert_eq!(ls_files_v(d), SPARSE_MARKS);
    assert!(!d.join("docs/guide.md").exists());
}

#[test]
fn undo_of_a_discard_in_a_sparse_checkout_brings_the_change_back() {
    let (dir, repo) = sparse_checkout();
    let d = dir.path();
    write_file(d, "app/main.rs", "fn main() { run() }\n");
    git_ok(d, &["add", "app/main.rs"]);
    write_file(d, "app/main.rs", "fn main() { run(); stop() }\n");
    let before = capture(&repo, SnapshotScope::FullUncapped).unwrap();
    git_core::stage::discard_file(&repo, "app/main.rs").unwrap();
    let after = capture(&repo, SnapshotScope::FullUncapped).unwrap();
    assert_eq!(status_short(d), "M  app/main.rs\n");

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(status_short(d), "MM app/main.rs\n");
    assert_eq!(
        std::fs::read_to_string(d.join("app/main.rs")).unwrap(),
        "fn main() { run(); stop() }\n"
    );
    assert_eq!(ls_files_v(d), SPARSE_MARKS);
    assert!(!d.join("vendor/lib/x.c").exists());
}

/// A file marked `--skip-worktree` to keep local edits out of status (a local config): Undo of
/// an unrelated stage keeps the mark and the edits.
#[test]
fn undo_keeps_a_skip_worktree_file_with_local_edits() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    write_file(d, "config.local", "shared\n");
    commit_all(&repo, "base");
    git_ok(d, &["update-index", "--skip-worktree", "config.local"]);
    write_file(d, "config.local", "my local\n");
    write_file(d, "a.txt", "b\n");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::stage::stage_file(&repo, "a.txt").unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();

    assert_eq!(status_short(d), " M a.txt\n");
    assert_eq!(ls_files_v(d), "H a.txt\nS config.local\n");
    assert_eq!(
        std::fs::read_to_string(d.join("config.local")).unwrap(),
        "my local\n"
    );
}

/// `git add -N new.txt`: Undo and Redo of an unrelated stage keep it added with intent to add
/// (` A`), not staged as an empty file.
#[test]
fn undo_and_redo_keep_an_intent_to_add_file() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(d, "new.txt", "new\n");
    git_ok(d, &["add", "-N", "new.txt"]);
    write_file(d, "a.txt", "b\n");
    assert_eq!(status_short(d), " M a.txt\n A new.txt\n");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::stage::stage_file(&repo, "a.txt").unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(status_short(d), " M a.txt\n A new.txt\n");

    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(status_short(d), "M  a.txt\n A new.txt\n");
}

/// Undo of `git add -N` itself (recorded as a stage of nothing else): the file is untracked
/// again; Redo marks it again.
#[test]
fn undo_and_redo_of_an_intent_to_add_put_the_mark_back() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(d, "new.txt", "new\n");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_ok(d, &["add", "-N", "new.txt"]);
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert_ne!(before, after);

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(status_short(d), "?? new.txt\n");

    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(status_short(d), " A new.txt\n");
}

/// The current index must hold the expected side's intent-to-add marks too: a file staged for
/// real since (the same empty blob) is a change, and nothing is written.
#[test]
fn undo_refuses_when_an_intent_to_add_file_was_staged_since() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(d, "new.txt", "");
    git_ok(d, &["add", "-N", "new.txt"]);
    write_file(d, "a.txt", "b\n");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::stage::stage_file(&repo, "a.txt").unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_ok(d, &["add", "new.txt"]);
    assert_eq!(status_short(d), "M  a.txt\nA  new.txt\n");

    let result = restore(&repo, &after, &before, "browsitory: undo test");

    assert!(
        matches!(result, Err(git_core::undo::UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert_eq!(status_short(d), "M  a.txt\nA  new.txt\n");
}
