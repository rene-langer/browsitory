mod common;

use git_core::remote::{
    add_remote, clear_current_upstream, current_upstream, list_remotes, remove_remote,
    rename_remote, set_current_upstream, update_remote_urls, RemoteError,
};

#[test]
fn remote_crud_and_upstream_round_trip() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");

    add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();
    set_current_upstream(&repo, "origin", "main").unwrap();

    assert_eq!(list_remotes(&repo).unwrap()[0].name, "origin");
    assert_eq!(
        current_upstream(&repo).unwrap().unwrap().remote_name,
        "origin"
    );
    assert!(matches!(
        remove_remote(&repo, "origin"),
        Err(RemoteError::RemoteInUse { .. })
    ));

    clear_current_upstream(&repo).unwrap();
    remove_remote(&repo, "origin").unwrap();
}

#[test]
fn updating_a_remote_without_a_push_url_keeps_push_url_unset() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();

    update_remote_urls(&repo, "origin", "file:///tmp/updated-origin.git", None).unwrap();

    let remote = list_remotes(&repo).unwrap().pop().unwrap();
    assert_eq!(remote.fetch_url, "file:///tmp/updated-origin.git");
    assert_eq!(remote.push_url, None);
}

#[test]
fn credential_bearing_urls_are_rejected_and_never_returned() {
    let (_dir, repo) = common::init_repo();
    let credential_url = "https://user:secret@example.com/repo.git";
    let mixed_case_credential_url = "HtTpS://user:secret@example.com/repo.git";

    assert!(add_remote(&repo, "origin", credential_url, None).is_err());
    assert!(list_remotes(&repo).unwrap().is_empty());
    assert!(add_remote(&repo, "origin", mixed_case_credential_url, None).is_err());
    assert!(list_remotes(&repo).unwrap().is_empty());
    assert!(add_remote(
        &repo,
        "origin",
        "https://example.com/repo.git",
        Some(credential_url)
    )
    .is_err());
    assert!(list_remotes(&repo).unwrap().is_empty());

    add_remote(&repo, "origin", "https://example.com/repo.git", None).unwrap();
    assert!(update_remote_urls(&repo, "origin", credential_url, None).is_err());
    assert!(update_remote_urls(
        &repo,
        "origin",
        "https://example.com/repo.git",
        Some(credential_url)
    )
    .is_err());
    assert_eq!(
        list_remotes(&repo).unwrap()[0].fetch_url,
        "https://example.com/repo.git"
    );

    repo.remote_set_pushurl("origin", Some(credential_url))
        .unwrap();
    assert!(list_remotes(&repo).is_err());
}

#[test]
fn normal_ssh_urls_are_accepted() {
    let (_dir, repo) = common::init_repo();

    add_remote(
        &repo,
        "origin",
        "git@github.com:browsitory/browsitory.git",
        None,
    )
    .unwrap();

    assert_eq!(
        list_remotes(&repo).unwrap()[0].fetch_url,
        "git@github.com:browsitory/browsitory.git"
    );
}

#[test]
fn setting_an_upstream_for_a_missing_remote_is_rejected() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");

    assert!(set_current_upstream(&repo, "missing", "main").is_err());
    assert!(current_upstream(&repo).unwrap().is_none());
}

#[test]
fn remote_url_lifecycle_supports_distinct_push_urls_and_clearing_them() {
    let (_dir, repo) = common::init_repo();
    add_remote(
        &repo,
        "origin",
        "file:///tmp/origin.git",
        Some("file:///tmp/origin-push.git"),
    )
    .unwrap();

    rename_remote(&repo, "origin", "upstream").unwrap();
    update_remote_urls(&repo, "upstream", "file:///tmp/upstream.git", None).unwrap();

    let remote = list_remotes(&repo).unwrap().pop().unwrap();
    assert_eq!(remote.name, "upstream");
    assert_eq!(remote.fetch_url, "file:///tmp/upstream.git");
    assert_eq!(remote.push_url, None);
}

#[test]
fn removal_is_blocked_by_a_non_current_branchs_upstream() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("topic", &head, false).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("branch.topic.remote", "origin").unwrap();
    config
        .set_str("branch.topic.merge", "refs/heads/main")
        .unwrap();

    assert!(matches!(
        remove_remote(&repo, "origin"),
        Err(RemoteError::RemoteInUse { branches, .. }) if branches == ["topic"]
    ));
}
