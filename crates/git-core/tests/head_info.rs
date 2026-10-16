mod common;

use git_core::repo::{head_info, OperationKind};

#[test]
fn unborn_head_has_no_id_and_no_operation() {
    let (_dir, repo) = common::init_repo();
    let info = head_info(&repo).unwrap();
    assert_eq!(info.head_id, None);
    assert_eq!(info.detached_at, None);
    assert!(!info.head_pushed);
    assert_eq!(info.operation, None);
}

#[test]
fn attached_head_reports_full_id_and_not_detached() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let head = repo.head().unwrap().target().unwrap().to_string();
    let info = head_info(&repo).unwrap();
    assert_eq!(info.head_id.as_deref(), Some(head.as_str()));
    assert_eq!(info.detached_at, None);
}

#[test]
fn detached_head_reports_short_id() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let oid = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(oid).unwrap();
    let info = head_info(&repo).unwrap();
    assert_eq!(info.detached_at, Some(oid.to_string()[..7].to_string()));
}

#[test]
fn head_pushed_is_true_only_when_upstream_contains_head() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let oid = repo.head().unwrap().target().unwrap();
    // Fake a fetched upstream at HEAD.
    repo.reference(&format!("refs/remotes/origin/{branch}"), oid, true, "test")
        .unwrap();
    repo.remote("origin", "https://example.invalid/repo.git")
        .unwrap();
    let mut config = repo.config().unwrap();
    config
        .set_str(&format!("branch.{branch}.remote"), "origin")
        .unwrap();
    config
        .set_str(
            &format!("branch.{branch}.merge"),
            &format!("refs/heads/{branch}"),
        )
        .unwrap();
    assert!(head_info(&repo).unwrap().head_pushed);

    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "unpushed");
    assert!(!head_info(&repo).unwrap().head_pushed);
}

#[test]
fn reports_cherry_pick_in_progress() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let oid = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("CHERRY_PICK_HEAD"), format!("{oid}\n")).unwrap();
    assert_eq!(
        head_info(&repo).unwrap().operation,
        Some(OperationKind::CherryPick)
    );
}

#[test]
fn reports_revert_and_merge_in_progress() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    let oid = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("REVERT_HEAD"), format!("{oid}\n")).unwrap();
    assert_eq!(
        head_info(&repo).unwrap().operation,
        Some(OperationKind::Revert)
    );
    std::fs::remove_file(repo.path().join("REVERT_HEAD")).unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{oid}\n")).unwrap();
    assert_eq!(
        head_info(&repo).unwrap().operation,
        Some(OperationKind::Merge)
    );
}
