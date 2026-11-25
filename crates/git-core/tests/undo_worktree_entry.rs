//! #1147: work-tree snapshots, for a discard while an operation is paused or the index has
//! conflicts: taken without the index (so conflicts don't stop them), and restored file by file
//! with the usual checks while the operation stays in progress, HEAD, refs and the index left
//! alone. The git CLI sets up the merge and checks the results.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, write_file};
use git_core::undo::{capture_worktree, restore_worktree, UndoError};

fn status_short(dir: &Path) -> String {
    let output = test_support::git(dir, &["status", "--short"]);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn read(dir: &Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap()
}

/// `a.txt` changed on `feature` and `main`, `b.txt` committed and then edited ("work in
/// progress"), and `git merge feature` stopped on the conflict in `a.txt`.
fn paused_merge() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "base\n");
    write_file(d, "b.txt", "b\n");
    commit_all(&repo, "base");
    git_ok(d, &["branch", "-M", "main"]);
    git_ok(d, &["checkout", "-q", "-b", "feature"]);
    write_file(d, "a.txt", "feature\n");
    git_ok(d, &["commit", "-qam", "feature"]);
    git_ok(d, &["checkout", "-q", "main"]);
    write_file(d, "a.txt", "main\n");
    git_ok(d, &["commit", "-qam", "main"]);
    write_file(d, "b.txt", "work in progress\n");
    let merge = test_support::git(d, &["merge", "feature"]);
    assert!(!merge.status.success(), "the merge should conflict");
    assert_eq!(status_short(d), "UU a.txt\n M b.txt\n");
    (dir, repo)
}

#[test]
fn a_work_tree_snapshot_is_taken_while_the_index_has_conflicts() {
    let (_dir, repo) = paused_merge();

    let snapshot = capture_worktree(&repo, None).unwrap();

    assert_eq!(snapshot.index_tree, None);
    let tree = repo
        .find_tree(git2::Oid::from_str(snapshot.worktree_tree.as_ref().unwrap()).unwrap())
        .unwrap();
    let b = tree.get_path(Path::new("b.txt")).unwrap();
    assert_eq!(
        repo.find_blob(b.id()).unwrap().content(),
        b"work in progress\n"
    );
}

#[test]
fn a_discard_during_a_merge_undoes_and_redoes_with_the_merge_left_paused() {
    let (dir, repo) = paused_merge();
    let d = dir.path();
    let whole = vec!["b.txt".to_string()];
    let before = capture_worktree(&repo, Some(&whole)).unwrap();
    git_core::stage::discard_file(&repo, "b.txt").unwrap();
    let after = capture_worktree(&repo, Some(&whole)).unwrap();
    assert_eq!(read(d, "b.txt"), "b\n");

    restore_worktree(&repo, &after, &before).unwrap();
    assert_eq!(read(d, "b.txt"), "work in progress\n");
    assert_eq!(status_short(d), "UU a.txt\n M b.txt\n");
    assert_eq!(repo.state(), git2::RepositoryState::Merge);

    restore_worktree(&repo, &before, &after).unwrap();
    assert_eq!(read(d, "b.txt"), "b\n");
    assert_eq!(status_short(d), "UU a.txt\n");
    assert_eq!(read(d, "a.txt").lines().next(), Some("<<<<<<< HEAD"));
}

#[test]
fn a_work_tree_restore_refuses_when_the_file_changed_since() {
    let (dir, repo) = paused_merge();
    let d = dir.path();
    let before = capture_worktree(&repo, None).unwrap();
    git_core::stage::discard_file(&repo, "b.txt").unwrap();
    let after = capture_worktree(&repo, None).unwrap();
    write_file(d, "b.txt", "typed since\n");

    let result = restore_worktree(&repo, &after, &before);

    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert_eq!(read(d, "b.txt"), "typed since\n");
}

/// A discarded untracked file comes back, and goes again on Redo, during the merge.
#[test]
fn a_deleted_untracked_file_comes_back_during_a_merge() {
    let (dir, repo) = paused_merge();
    let d = dir.path();
    write_file(d, "notes/n.txt", "notes\n");
    let before = capture_worktree(&repo, None).unwrap();
    git_core::stage::discard_file(&repo, "notes/n.txt").unwrap();
    let after = capture_worktree(&repo, None).unwrap();
    assert!(!d.join("notes/n.txt").exists());

    restore_worktree(&repo, &after, &before).unwrap();
    assert_eq!(read(d, "notes/n.txt"), "notes\n");

    restore_worktree(&repo, &before, &after).unwrap();
    assert!(!d.join("notes/n.txt").exists());
}
