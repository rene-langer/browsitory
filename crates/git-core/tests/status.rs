mod common;

use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git_core::status::StatusKind;

#[test]
fn reports_an_untracked_file_as_unstaged_new() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "untracked.txt", "hello");

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "untracked.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::New);
}

#[test]
fn reports_a_staged_new_file_as_staged_new() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "staged.txt", "hello");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("staged.txt")).unwrap();
    index.write().unwrap();

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "staged.txt");
    assert!(entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::New);
}

#[test]
fn reports_a_modified_tracked_file_as_unstaged_modified() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("tracked.txt")).unwrap();
    let tree_id = index.write_tree().unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = repo.signature().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
        .unwrap();
    write_file(dir.path(), "tracked.txt", "changed");

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "tracked.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::Modified);
}

#[test]
fn reports_a_deleted_tracked_file_as_unstaged_deleted() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    commit_all(&repo, "initial commit");
    std::fs::remove_file(dir.path().join("tracked.txt")).unwrap();

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "tracked.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::Deleted);
}

#[test]
fn reports_a_staged_rename_as_staged_renamed() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.txt", "line one\nline two\nline three\n");
    commit_all(&repo, "initial commit");

    std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("old.txt")).unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert!(entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::Renamed);
}

#[test]
fn reports_two_entries_for_a_path_staged_then_modified_again() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    commit_all(&repo, "initial commit");

    write_file(dir.path(), "tracked.txt", "staged change");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("tracked.txt")).unwrap();
    index.write().unwrap();
    write_file(dir.path(), "tracked.txt", "further worktree change");

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry.path == "tracked.txt"));
    let staged = entries.iter().find(|entry| entry.staged).unwrap();
    let unstaged = entries.iter().find(|entry| !entry.staged).unwrap();
    assert_eq!(staged.kind, StatusKind::Modified);
    assert_eq!(unstaged.kind, StatusKind::Modified);
}

#[test]
fn ignores_files_matched_by_gitignore() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), ".gitignore", "ignored.txt\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "ignored.txt", "noise");

    let entries = git_core::status::status(&repo).unwrap();

    assert!(entries.is_empty());
}

#[test]
fn reports_a_clean_repository_as_empty() {
    let (_dir, repo) = init_repo();

    let entries = git_core::status::status(&repo).unwrap();

    assert!(entries.is_empty());
}
