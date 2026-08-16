use git2::{Oid, Repository};
use git_core::reflog::{read_reflog, restore_reflog_entry, ReflogError};
use tempfile::TempDir;

#[test]
fn reading_or_restoring_a_local_ref_without_a_reflog_does_not_create_one() {
    let (_dir, repo) = init_bare_repo();
    let first = create_orphan_commit(&repo, "first commit");
    let second = create_orphan_commit(&repo, "second commit");
    write_direct_reference(&repo, "refs/heads/no-log", first);
    let log_path = repo.path().join("logs/refs/heads/no-log");
    if log_path.exists() {
        repo.reflog_delete("refs/heads/no-log").unwrap();
    }
    assert!(!log_path.exists());
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
    let (_dir, repo) = init_bare_repo();
    remove_reflog_file(&repo, "HEAD");
    let orphan = create_orphan_commit(&repo, "orphan commit");
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

fn init_bare_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    let repo = Repository::init_bare(&path).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    config.set_bool("core.logallrefupdates", false).unwrap();
    drop(config);

    (dir, Repository::open_bare(path).unwrap())
}

fn create_orphan_commit(repo: &Repository, message: &str) -> Oid {
    let tree = repo
        .find_tree(repo.treebuilder(None).unwrap().write().unwrap())
        .unwrap();
    let signature = repo.signature().unwrap();

    repo.commit(None, &signature, &signature, message, &tree, &[])
        .unwrap()
}

fn write_direct_reference(repo: &Repository, name: &str, target: Oid) {
    let path = repo.path().join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("{target}\n")).unwrap();
}

fn remove_reflog_file(repo: &Repository, name: &str) {
    let path = repo.path().join("logs").join(name);
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
}
