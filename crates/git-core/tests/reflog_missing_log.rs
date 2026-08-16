#[allow(dead_code)]
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::reflog::{read_reflog, restore_reflog_entry, ReflogError};
use git2::{Oid, Repository};

#[test]
fn reading_or_restoring_a_local_ref_without_a_reflog_does_not_create_one() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "history.txt", "first");
    commit_all(&repo, "first commit");
    let first = repo.head().unwrap().target().unwrap();

    write_file(dir.path(), "history.txt", "second");
    commit_all(&repo, "second commit");
    let second = repo.head().unwrap().target().unwrap();
    repo.reference("refs/heads/no-log", first, false, "create no-log branch")
        .unwrap();
    if repo.reference_has_log("refs/heads/no-log").unwrap() {
        repo.reflog_delete("refs/heads/no-log").unwrap();
    }
    assert!(!repo.reference_has_log("refs/heads/no-log").unwrap());

    assert!(read_reflog(&repo, "refs/heads/no-log").unwrap().is_empty());
    assert!(!repo.reference_has_log("refs/heads/no-log").unwrap());

    assert!(matches!(
        restore_reflog_entry(&repo, "refs/heads/no-log", &second.to_string()),
        Err(ReflogError::TargetNotInReflog)
    ));
    assert_eq!(
        repo.find_reference("refs/heads/no-log").unwrap().target(),
        Some(first)
    );
    assert!(!repo.reference_has_log("refs/heads/no-log").unwrap());
}

#[test]
fn restoring_an_unborn_head_to_an_orphan_commit_does_not_create_a_ref_or_reflog() {
    let (dir, repo) = init_repo();
    let orphan = create_orphan_commit(&repo, dir.path());
    let head_target = repo
        .find_reference("HEAD")
        .unwrap()
        .symbolic_target()
        .unwrap()
        .unwrap()
        .to_string();

    assert!(repo.find_reference(&head_target).is_err());
    assert!(!repo.reference_has_log("HEAD").unwrap());
    assert!(repo.head().is_err());

    assert!(matches!(
        restore_reflog_entry(&repo, "HEAD", &orphan.to_string()),
        Err(ReflogError::TargetNotInReflog)
    ));

    assert!(repo.find_reference(&head_target).is_err());
    assert!(!repo.reference_has_log("HEAD").unwrap());
    assert!(repo.head().is_err());
}

fn create_orphan_commit(repo: &Repository, dir: &std::path::Path) -> Oid {
    write_file(dir, "orphan.txt", "orphan");
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = repo.signature().unwrap();

    repo.commit(None, &signature, &signature, "orphan commit", &tree, &[])
        .unwrap()
}
