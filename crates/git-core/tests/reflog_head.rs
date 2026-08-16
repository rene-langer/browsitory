#[allow(dead_code)]
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::reflog::{read_reflog, restore_reflog_entry};

#[test]
fn restores_an_attached_symbolic_head_by_moving_its_local_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "history.txt", "first");
    commit_all(&repo, "first commit");
    let first = repo.head().unwrap().target().unwrap();

    write_file(dir.path(), "history.txt", "second");
    commit_all(&repo, "second commit");
    let second = repo.head().unwrap().target().unwrap();

    assert_eq!(
        read_reflog(&repo, "HEAD").unwrap()[0].new_id,
        second.to_string()
    );
    restore_reflog_entry(&repo, "HEAD", &first.to_string()).unwrap();

    let head = repo.find_reference("HEAD").unwrap();
    let target_branch = head.symbolic_target().unwrap().unwrap().to_string();
    assert!(target_branch.starts_with("refs/heads/"));
    assert_eq!(
        repo.find_reference(&target_branch).unwrap().target(),
        Some(first)
    );
    assert_eq!(repo.head().unwrap().target(), Some(first));
}

#[test]
fn rejects_head_restoration_when_symbolic_head_resolves_to_a_remote_tracking_ref() {
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
    repo.reference_symbolic(
        "HEAD",
        "refs/remotes/origin/main",
        true,
        "attach HEAD to remote tracking ref",
    )
    .unwrap();
    assert_eq!(
        repo.find_reference("HEAD")
            .unwrap()
            .symbolic_target()
            .unwrap(),
        Some("refs/remotes/origin/main")
    );
    assert!(read_reflog(&repo, "HEAD")
        .unwrap()
        .iter()
        .any(|entry| entry.new_id == first.to_string()));

    assert!(restore_reflog_entry(&repo, "HEAD", &first.to_string()).is_err());
    assert_eq!(
        repo.find_reference("refs/remotes/origin/main")
            .unwrap()
            .target(),
        Some(second)
    );
}

#[test]
fn restores_a_detached_head_directly() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "history.txt", "first");
    commit_all(&repo, "first commit");
    let first = repo.head().unwrap().target().unwrap();

    write_file(dir.path(), "history.txt", "second");
    commit_all(&repo, "second commit");
    let second = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(second).unwrap();
    assert!(repo.head_detached().unwrap());
    assert_eq!(
        repo.find_reference("HEAD")
            .unwrap()
            .symbolic_target()
            .unwrap(),
        None
    );
    assert!(read_reflog(&repo, "HEAD")
        .unwrap()
        .iter()
        .any(|entry| entry.new_id == first.to_string()));

    restore_reflog_entry(&repo, "HEAD", &first.to_string()).unwrap();

    assert!(repo.head_detached().unwrap());
    assert_eq!(repo.head().unwrap().target(), Some(first));
}
