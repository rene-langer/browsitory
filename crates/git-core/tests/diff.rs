mod common;

use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git_core::diff::DiffLineOrigin;

#[test]
fn working_diff_unstaged_shows_added_and_context_lines() {
    let (dir, repo) = init_repo();
    write_file(
        dir.path(),
        "tracked.txt",
        "line one\nline two\nline three\n",
    );
    commit_all(&repo, "initial commit");
    write_file(
        dir.path(),
        "tracked.txt",
        "line one\nline two changed\nline three\n",
    );

    let hunks = git_core::diff::working_diff(&repo, "tracked.txt", false).unwrap();

    assert_eq!(hunks.len(), 1);
    let lines = &hunks[0].lines;
    assert!(lines
        .iter()
        .any(|line| line.origin == DiffLineOrigin::Remove && line.content.trim() == "line two"));
    assert!(lines.iter().any(
        |line| line.origin == DiffLineOrigin::Add && line.content.trim() == "line two changed"
    ));
}

#[test]
fn working_diff_staged_shows_the_staged_content() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello\nworld\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    let staged_hunks = git_core::diff::working_diff(&repo, "new.txt", true).unwrap();

    assert!(!staged_hunks.is_empty());
    let concatenated: String = staged_hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .map(|line| {
            assert_eq!(line.origin, DiffLineOrigin::Add);
            line.content.clone()
        })
        .collect();
    assert_eq!(concatenated, "hello\nworld\n");

    let unstaged_hunks = git_core::diff::working_diff(&repo, "new.txt", false).unwrap();
    assert!(unstaged_hunks.is_empty());
}

#[test]
fn commit_diff_shows_the_change_introduced_by_that_commit() {
    let (dir, repo) = init_repo();
    write_file(
        dir.path(),
        "tracked.txt",
        "line one\nline two\nline three\n",
    );
    commit_all(&repo, "initial commit");
    write_file(
        dir.path(),
        "tracked.txt",
        "line one\nline two changed\nline three\n",
    );
    commit_all(&repo, "change middle line");

    let second_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let hunks = git_core::diff::commit_diff(&repo, &second_id, "tracked.txt").unwrap();

    assert_eq!(hunks.len(), 1);
    let lines = &hunks[0].lines;
    assert!(lines
        .iter()
        .any(|line| line.origin == DiffLineOrigin::Remove && line.content.trim() == "line two"));
    assert!(lines.iter().any(
        |line| line.origin == DiffLineOrigin::Add && line.content.trim() == "line two changed"
    ));
}

#[test]
fn commit_diff_on_the_first_commit_shows_every_line_as_added() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");

    let commit_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let hunks = git_core::diff::commit_diff(&repo, &commit_id, "tracked.txt").unwrap();

    assert!(!hunks.is_empty());
    for hunk in &hunks {
        for line in &hunk.lines {
            assert_eq!(line.origin, DiffLineOrigin::Add);
        }
    }
}

#[test]
fn commit_files_lists_every_changed_path() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "one.txt", "content one");
    write_file(dir.path(), "two.txt", "content two");
    commit_all(&repo, "add two files");

    let commit_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();

    let mut result = git_core::diff::commit_files(&repo, &commit_id).unwrap();
    result.sort();
    let mut expected = vec!["one.txt".to_string(), "two.txt".to_string()];
    expected.sort();

    assert_eq!(result, expected);
}
