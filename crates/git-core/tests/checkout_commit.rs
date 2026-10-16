mod common;

use git_core::branch::checkout_commit;

#[test]
fn checks_out_a_commit_as_detached_head() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "1\n");
    common::commit_all(&repo, "one");
    let first = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "2\n");
    common::commit_all(&repo, "two");

    checkout_commit(&repo, &first.to_string()).unwrap();

    assert!(repo.head_detached().unwrap());
    assert_eq!(repo.head().unwrap().target().unwrap(), first);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "1\n"
    );
}

#[test]
fn refuses_to_overwrite_local_changes_and_leaves_head_attached() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "1\n");
    common::commit_all(&repo, "one");
    let first = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "2\n");
    common::commit_all(&repo, "two");
    let second = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "dirty\n");

    assert!(checkout_commit(&repo, &first.to_string()).is_err());

    assert!(!repo.head_detached().unwrap());
    assert_eq!(repo.head().unwrap().target().unwrap(), second);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "dirty\n"
    );
}
