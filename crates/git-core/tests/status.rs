mod common;

use common::{init_repo, write_file};
use git_core::{FileState, stage_path, status};

#[test]
fn reports_untracked_file_as_unstaged_new() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "hello\n");

    let entries = status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "a.txt");
    assert_eq!(entries[0].staged, None);
    assert_eq!(entries[0].unstaged, Some(FileState::New));
}

#[test]
fn staging_moves_new_file_from_unstaged_to_staged() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "hello\n");

    stage_path(&repo, "a.txt").unwrap();
    let entries = status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].staged, Some(FileState::New));
    assert_eq!(entries[0].unstaged, None);
}

#[test]
fn clean_repo_has_no_status_entries() {
    let (_dir, repo) = init_repo();
    let entries = status(&repo).unwrap();
    assert!(entries.is_empty());
}
