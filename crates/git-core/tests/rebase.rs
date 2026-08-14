mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn commits_since_lists_commits_oldest_first_between_onto_and_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto_id = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();

    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");

    let commits = git_core::rebase::commits_since(&repo, &onto_id).unwrap();

    assert_eq!(commits.len(), 2);
    assert_eq!(commits[0].summary, "add a");
    assert_eq!(commits[1].summary, "add b");
}

#[test]
fn commits_since_returns_empty_when_onto_is_head() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");

    let commits = git_core::rebase::commits_since(&repo, "HEAD").unwrap();

    assert!(commits.is_empty());
}
