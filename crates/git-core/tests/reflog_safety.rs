#[allow(dead_code)]
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::reflog::{read_reflog, restore_reflog_entry};

#[test]
fn rejects_unsafe_recovery_requests_without_moving_a_reference() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "history.txt", "first");
    commit_all(&repo, "first commit");
    let first = repo.head().unwrap().target().unwrap();

    write_file(dir.path(), "history.txt", "second");
    commit_all(&repo, "second commit");
    let second = repo.head().unwrap().target().unwrap();
    repo.reference(
        "refs/remotes/origin/main",
        second,
        true,
        "create remote tracking ref",
    )
    .unwrap();

    assert!(read_reflog(&repo, "refs/remotes/origin/main").is_err());
    assert!(restore_reflog_entry(&repo, "refs/remotes/origin/main", &first.to_string()).is_err());
    assert_eq!(
        repo.find_reference("refs/remotes/origin/main")
            .unwrap()
            .target(),
        Some(second)
    );

    assert!(restore_reflog_entry(&repo, "refs/heads/../other", &first.to_string()).is_err());
    assert_eq!(repo.head().unwrap().target(), Some(second));

    assert!(
        restore_reflog_entry(&repo, "HEAD", "0123456789012345678901234567890123456789").is_err()
    );
    assert_eq!(repo.head().unwrap().target(), Some(second));

    let (_unborn_dir, unborn) = init_repo();
    assert!(
        restore_reflog_entry(&unborn, "HEAD", "0000000000000000000000000000000000000000").is_err()
    );
    assert!(unborn.head().is_err());
}
