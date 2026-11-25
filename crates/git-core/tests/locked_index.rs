//! A change that fails to write the index (another program holds `index.lock`) leaves nothing
//! in the repository's in-memory index, which the worker shares between commands: the next
//! write would otherwise save it (#1166).

mod common;

use std::path::Path;

use common::{commit_all, init_repo, write_file};

fn status_short(dir: &Path) -> String {
    let output = test_support::git(dir, &["status", "--short"]);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn lock(repo: &git2::Repository) -> std::path::PathBuf {
    let lock = repo.path().join("index.lock");
    std::fs::write(&lock, "").unwrap();
    lock
}

/// What the next command that writes the index does with the repository's index object.
fn next_write(repo: &git2::Repository) {
    repo.index().unwrap().write().unwrap();
}

#[test]
fn a_stage_that_failed_on_a_locked_index_is_not_written_later() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    write_file(d, "b.txt", "b\n");
    commit_all(&repo, "base");
    write_file(d, "a.txt", "a, edited\n");
    write_file(d, "b.txt", "b, edited\n");
    let lock = lock(&repo);

    assert!(git_core::stage::stage_file(&repo, "a.txt").is_err());
    std::fs::remove_file(lock).unwrap();
    next_write(&repo);

    assert_eq!(status_short(d), " M a.txt\n M b.txt\n");
}

#[test]
fn staging_an_untracked_folder_that_failed_on_a_locked_index_is_not_written_later() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(d, "new/one.txt", "one\n");
    write_file(d, "new/two.txt", "two\n");
    let lock = lock(&repo);

    assert!(git_core::stage::stage_file(&repo, "new/").is_err());
    std::fs::remove_file(lock).unwrap();
    next_write(&repo);

    assert_eq!(status_short(d), "?? new/\n");
}

#[test]
fn a_mixed_reset_that_failed_on_a_locked_index_is_not_written_later() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "one");
    write_file(d, "a.txt", "a, two\n");
    commit_all(&repo, "two");
    let one = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent_id(0)
        .unwrap();
    let lock = lock(&repo);

    assert!(
        git_core::reset::reset_to(&repo, &one.to_string(), git_core::reset::ResetMode::Mixed)
            .is_err()
    );
    std::fs::remove_file(lock).unwrap();
    next_write(&repo);

    assert_eq!(status_short(d), "");
}
