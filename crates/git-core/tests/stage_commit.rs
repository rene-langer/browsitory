mod common;

use common::{init_repo, write_file};
use git_core::{FileState, create_commit, stage_path, status, unstage_path};

#[test]
fn commit_creates_a_commit_with_the_given_message_and_clears_status() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "hello\n");
    stage_path(&repo, "a.txt").unwrap();

    let oid = create_commit(&repo, "add a.txt").unwrap();

    let commit = repo.find_commit(oid).unwrap();
    assert_eq!(commit.summary(), Ok(Some("add a.txt")));
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn unstage_restores_head_state_for_a_modified_tracked_file() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "hello\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&repo, "add a.txt").unwrap();

    write_file(&dir, "a.txt", "hello again\n");
    stage_path(&repo, "a.txt").unwrap();
    assert_eq!(status(&repo).unwrap()[0].staged, Some(FileState::Modified));

    unstage_path(&repo, "a.txt").unwrap();

    let entries = status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].staged, None);
    assert_eq!(entries[0].unstaged, Some(FileState::Modified));
}

#[test]
fn unstage_on_unborn_head_just_empties_the_index() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "hello\n");
    stage_path(&repo, "a.txt").unwrap();

    unstage_path(&repo, "a.txt").unwrap();

    let entries = status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].staged, None);
    assert_eq!(entries[0].unstaged, Some(FileState::New));
}
