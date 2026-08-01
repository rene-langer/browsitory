mod common;

use common::{init_repo, write_file};
use git_core::{
    LineKind, create_commit, stage_path, staged_file_diff, unstaged_file_diff, word_diff,
};
use similar::ChangeTag;

#[test]
fn staged_diff_shows_new_file_as_all_additions() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "one\ntwo\n");
    stage_path(&repo, "a.txt").unwrap();

    let diff = staged_file_diff(&repo, "a.txt").unwrap();

    assert_eq!(diff.lines.len(), 2);
    assert!(diff.lines.iter().all(|l| l.kind == LineKind::Addition));
    assert_eq!(diff.lines[0].content, "one");
    assert_eq!(diff.lines[1].content, "two");
}

#[test]
fn unstaged_diff_shows_working_tree_edit_against_the_index() {
    let (dir, repo) = init_repo();
    write_file(&dir, "a.txt", "one\ntwo\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&repo, "add a.txt").unwrap();

    write_file(&dir, "a.txt", "one\ntwo changed\n");

    let diff = unstaged_file_diff(&repo, "a.txt").unwrap();

    assert!(
        diff.lines
            .iter()
            .any(|l| l.kind == LineKind::Deletion && l.content == "two")
    );
    assert!(
        diff.lines
            .iter()
            .any(|l| l.kind == LineKind::Addition && l.content == "two changed")
    );
}

#[test]
fn word_diff_highlights_only_the_changed_word() {
    let changes = word_diff("the quick fox", "the slow fox");

    let deleted: Vec<_> = changes
        .iter()
        .filter(|(tag, _)| *tag == ChangeTag::Delete)
        .map(|(_, text)| text.trim())
        .collect();
    let inserted: Vec<_> = changes
        .iter()
        .filter(|(tag, _)| *tag == ChangeTag::Insert)
        .map(|(_, text)| text.trim())
        .collect();

    assert_eq!(deleted, vec!["quick"]);
    assert_eq!(inserted, vec!["slow"]);
}
