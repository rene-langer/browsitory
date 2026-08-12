mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn log_returns_an_empty_vec_for_a_repository_with_no_commits() {
    let (_dir, repo) = init_repo();

    let result = git_core::log::log(&repo, 10).unwrap();

    assert!(result.is_empty());
}

#[test]
fn log_returns_commits_most_recent_first() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "first commit");
    write_file(dir.path(), "file.txt", "updated");
    commit_all(&repo, "second commit");

    let entries = git_core::log::log(&repo, 10).unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].summary, "second commit");
    assert_eq!(entries[1].summary, "first commit");
    assert_eq!(entries[0].short_id.len(), 7);
    assert!(entries[0].id.starts_with(&entries[0].short_id));
    assert_eq!(entries[0].author_name, "Test User");
}

#[test]
fn log_respects_the_limit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second commit");
    write_file(dir.path(), "file.txt", "v3");
    commit_all(&repo, "third commit");

    let entries = git_core::log::log(&repo, 2).unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].summary, "third commit");
    assert_eq!(entries[1].summary, "second commit");
}
