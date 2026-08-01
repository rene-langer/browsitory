mod common;

use common::{init_repo, write_file};
use git_core::{blame_file, create_commit, stage_path};

#[test]
fn single_commit_attributes_every_line_to_it() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "one\ntwo\nthree\n");
    stage_path(&repo, "a.txt").unwrap();
    let oid = create_commit(&repo, "add a.txt").unwrap();

    let lines = blame_file(&repo, "a.txt").unwrap();

    assert_eq!(lines.len(), 3);
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(line.commit, oid);
        assert_eq!(line.final_lineno, i + 1);
        assert_eq!(line.author_name, "Test User");
        assert_eq!(line.author_email, "test@example.com");
        assert_eq!(line.summary, "add a.txt");
    }
    assert_eq!(lines[0].content, "one");
    assert_eq!(lines[1].content, "two");
    assert_eq!(lines[2].content, "three");
}

#[test]
fn untouched_lines_keep_their_original_attributing_commit() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "one\ntwo\nthree\n");
    stage_path(&repo, "a.txt").unwrap();
    let first_oid = create_commit(&repo, "add a.txt").unwrap();

    // Only touch the middle line; "one" and "three" should keep attribution
    // to the first commit, "two changed" should attribute to the second.
    write_file(&dir, "a.txt", "one\ntwo changed\nthree\n");
    stage_path(&repo, "a.txt").unwrap();
    let second_oid = create_commit(&repo, "change line two").unwrap();

    let lines = blame_file(&repo, "a.txt").unwrap();

    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].content, "one");
    assert_eq!(lines[0].commit, first_oid);
    assert_eq!(lines[1].content, "two changed");
    assert_eq!(lines[1].commit, second_oid);
    assert_eq!(lines[2].content, "three");
    assert_eq!(lines[2].commit, first_oid);
}

#[test]
fn tracks_lines_across_a_file_rename_with_rename_tracking_enabled() {
    let (dir, repo) = init_repo();
    write_file(&dir, "old_name.txt", "alpha\nbeta\ngamma\n");
    stage_path(&repo, "old_name.txt").unwrap();
    let first_oid = create_commit(&repo, "add old_name.txt").unwrap();

    common::remove_file(&dir, "old_name.txt");
    write_file(&dir, "new_name.txt", "alpha\nbeta\ngamma\n");
    stage_path(&repo, "old_name.txt").unwrap();
    stage_path(&repo, "new_name.txt").unwrap();
    create_commit(&repo, "rename to new_name.txt").unwrap();

    let lines = blame_file(&repo, "new_name.txt").unwrap();

    assert_eq!(lines.len(), 3);
    // With track_copies_same_file (the git-core default) the unchanged
    // content still attributes to the commit that originally introduced it,
    // not the rename commit.
    for line in &lines {
        assert_eq!(line.commit, first_oid);
    }
}
