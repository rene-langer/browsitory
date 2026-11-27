//! #1161: the Reflog section reads only the newest entries of a reflog, however long it is.

#[allow(dead_code)]
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::reflog::{read_reflog, read_reflog_newest};

/// A repository whose HEAD reflog has five entries, newest first: commits `c5` to `c1`.
fn five_commits() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    for n in 1..=5 {
        write_file(dir.path(), "f.txt", &n.to_string());
        commit_all(&repo, &format!("c{n}"));
    }
    (dir, repo)
}

fn summaries(entries: &[git_core::reflog::ReflogEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.summary.clone().unwrap_or_default())
        .collect()
}

#[test]
fn reads_only_the_newest_entries_up_to_the_limit() {
    let (_dir, repo) = five_commits();

    let newest = read_reflog_newest(&repo, "HEAD", 2).unwrap();

    assert_eq!(summaries(&newest), ["c5", "c4"]);
    assert_eq!(newest, read_reflog(&repo, "HEAD").unwrap()[..2]);
}

#[test]
fn a_limit_past_the_end_reads_every_entry() {
    let (_dir, repo) = five_commits();

    assert_eq!(
        summaries(&read_reflog_newest(&repo, "HEAD", 5).unwrap()),
        ["c5", "c4", "c3", "c2", "c1"]
    );
    assert_eq!(read_reflog_newest(&repo, "HEAD", 6).unwrap().len(), 5);
    assert!(read_reflog_newest(&repo, "HEAD", 0).unwrap().is_empty());
}

#[test]
fn a_reference_without_a_reflog_reads_nothing_and_a_bad_one_is_refused() {
    let (_dir, repo) = five_commits();
    repo.reference(
        "refs/heads/no-log",
        repo.head().unwrap().target().unwrap(),
        false,
        "",
    )
    .unwrap();
    std::fs::remove_file(repo.path().join("logs/refs/heads/no-log")).ok();

    assert!(read_reflog_newest(&repo, "refs/heads/no-log", 10)
        .unwrap()
        .is_empty());
    assert!(read_reflog_newest(&repo, "refs/remotes/origin/main", 10).is_err());
}
