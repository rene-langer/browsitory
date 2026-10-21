mod common;

use common::{commit_all, init_repo, write_file};
use git_core::signing::{create_commit, HeadUpdate};

fn last_reflog_message(repo: &git2::Repository, name: &str) -> String {
    let reflog = repo.reflog(name).unwrap();
    reflog
        .get(0)
        .unwrap()
        .message()
        .unwrap()
        .unwrap()
        .to_string()
}

fn head_tree(repo: &git2::Repository) -> git2::Oid {
    let mut index = repo.index().unwrap();
    index.write_tree().unwrap()
}

#[test]
fn initial_commit_matches_libgit2_reflog_and_moves_unborn_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(head_tree(&repo)).unwrap();
    let sig = repo.signature().unwrap();

    let oid = create_commit(
        &repo,
        Some(HeadUpdate {
            expected: None,
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "first\n\nbody",
        &tree,
        &[],
    )
    .unwrap();

    assert_eq!(repo.head().unwrap().target(), Some(oid));
    assert_eq!(
        last_reflog_message(&repo, "HEAD"),
        "commit (initial): first"
    );
    let branch = repo.head().unwrap().name().unwrap().to_string();
    assert_eq!(
        last_reflog_message(&repo, &branch),
        "commit (initial): first"
    );
}

#[test]
fn normal_commit_matches_libgit2_reflog() {
    // Control: what libgit2's own repo.commit(Some("HEAD")) writes.
    let (control_dir, control) = init_repo();
    write_file(control_dir.path(), "a.txt", "a\n");
    commit_all(&control, "base");
    write_file(control_dir.path(), "b.txt", "b\n");
    commit_all(&control, "second");
    let expected = last_reflog_message(&control, "HEAD");

    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "b.txt", "b\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("b.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(head_tree(&repo)).unwrap();
    let sig = repo.signature().unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();

    create_commit(
        &repo,
        Some(HeadUpdate {
            expected: Some(parent.id()),
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "second",
        &tree,
        &[&parent],
    )
    .unwrap();

    assert_eq!(last_reflog_message(&repo, "HEAD"), expected);
    assert_eq!(expected, "commit: second");
}

#[test]
fn two_parent_commit_uses_the_merge_suffix() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = base.tree().unwrap();
    let sig = repo.signature().unwrap();
    let other = repo
        .find_commit(
            repo.commit(None, &sig, &sig, "other", &tree, &[&base])
                .unwrap(),
        )
        .unwrap();

    create_commit(
        &repo,
        Some(HeadUpdate {
            expected: Some(base.id()),
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "merge it",
        &tree,
        &[&base, &other],
    )
    .unwrap();

    assert_eq!(
        last_reflog_message(&repo, "HEAD"),
        "commit (merge): merge it"
    );
}

#[test]
fn refuses_when_head_moved_and_moves_nothing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "moved on");
    let tip = repo.head().unwrap().target().unwrap();
    let sig = repo.signature().unwrap();
    let tree = base.tree().unwrap();

    let result = create_commit(
        &repo,
        Some(HeadUpdate {
            expected: Some(base.id()),
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "stale",
        &tree,
        &[&base],
    );

    assert!(result.is_err());
    assert_eq!(repo.head().unwrap().target(), Some(tip));
}

#[test]
fn without_head_update_writes_the_object_only() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();

    let oid = create_commit(
        &repo,
        None,
        &sig,
        &sig,
        "loose",
        &base.tree().unwrap(),
        &[&base],
    )
    .unwrap();

    assert_eq!(repo.head().unwrap().target(), Some(base.id()));
    assert_eq!(
        repo.find_commit(oid).unwrap().summary().unwrap(),
        Some("loose")
    );
}

#[test]
fn unsigned_when_commit_gpgsign_is_unset() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    let oid = create_commit(
        &repo,
        None,
        &sig,
        &sig,
        "x",
        &base.tree().unwrap(),
        &[&base],
    )
    .unwrap();
    assert_eq!(
        repo.extract_signature(&oid, None).err().unwrap().code(),
        git2::ErrorCode::NotFound
    );
    assert!(git_core::signing::signing_config(&repo).unwrap().is_none());
}

#[test]
fn x509_format_is_refused() {
    let (_dir, repo) = init_repo();
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "x509").unwrap();
    let err = git_core::signing::signing_config(&repo).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Signing is configured with gpg.format=x509, which Browsitory does not support."
    );
}

#[test]
fn ssh_without_signing_key_is_refused() {
    let (_dir, repo) = init_repo();
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "ssh").unwrap();
    let err = git_core::signing::signing_config(&repo).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Signing with SSH needs user.signingKey to be set."
    );
}
