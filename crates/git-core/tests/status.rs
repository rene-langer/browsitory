mod common;

use common::{init_repo, write_file};
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
fn reports_a_clean_repository_as_empty() {
    let (_dir, repo) = init_repo();

    let entries = git_core::status::status(&repo).unwrap();

    assert!(entries.is_empty());
}
