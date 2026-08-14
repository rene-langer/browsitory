mod common;

use git_core::remote::{
    add_remote, clear_current_upstream, current_upstream, fetch_remote, list_remotes,
    remote_upstreams, remove_remote, remove_remote_and_clear_upstreams, rename_remote,
    set_current_upstream, update_remote_urls, CredentialProvider, RemoteError, TransferOperation,
    TransferPhase, TransferProgress, TransferReporter,
};

#[derive(Default)]
struct VecReporter {
    events: Vec<TransferProgress>,
}

impl TransferReporter for VecReporter {
    fn report(&mut self, event: TransferProgress) {
        self.events.push(event);
    }
}

struct NoCredentials;

impl CredentialProvider for NoCredentials {
    fn credential(
        &mut self,
        _url: &str,
        _username: Option<&str>,
        _allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        Err(git2::Error::from_str(
            "credentials were not expected for a local remote",
        ))
    }
}

#[test]
fn fetch_updates_tracking_ref_and_reports_owned_progress() {
    let (source_dir, source) = common::init_repo();
    common::write_file(source_dir.path(), "README.md", "initial commit\n");
    common::commit_all(&source, "initial commit");
    let remote_dir = tempfile::TempDir::new().unwrap();
    let remote_repo = git2::Repository::init_bare(remote_dir.path()).unwrap();
    let branch_name = source.head().unwrap().shorthand().unwrap().to_string();
    let branch_ref = format!("refs/heads/{branch_name}");
    source
        .remote("origin", remote_dir.path().to_str().unwrap())
        .unwrap();
    source
        .find_remote("origin")
        .unwrap()
        .push(&[format!("{branch_ref}:{branch_ref}")], None)
        .unwrap();
    let (_local_dir, local) = common::init_repo();
    local
        .remote("origin", remote_dir.path().to_str().unwrap())
        .unwrap();
    drop(remote_repo);

    let mut events = VecReporter::default();
    fetch_remote(
        &local,
        "origin",
        "fetch-42".to_string(),
        &mut NoCredentials,
        &mut events,
    )
    .unwrap();

    assert!(local
        .find_reference(&format!("refs/remotes/origin/{branch_name}"))
        .is_ok());
    assert!(events
        .events
        .iter()
        .any(|event| event.phase == TransferPhase::Receiving));
    assert!(events.events.iter().all(|event| {
        event.operation_id == "fetch-42" && event.operation == TransferOperation::Fetch
    }));
}

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

#[test]
fn explicit_removal_clears_every_upstream_for_only_the_selected_remote() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();
    add_remote(&repo, "backup", "file:///tmp/backup.git", None).unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let current_branch = repo.head().unwrap().shorthand().unwrap().to_string();
    repo.branch("topic", &head, false).unwrap();
    repo.branch("backup-topic", &head, false).unwrap();
    let mut config = repo.config().unwrap();
    for branch in [&current_branch, "topic"] {
        config
            .set_str(&format!("branch.{branch}.remote"), "origin")
            .unwrap();
        config
            .set_str(&format!("branch.{branch}.merge"), "refs/heads/main")
            .unwrap();
    }
    config
        .set_str("branch.backup-topic.remote", "backup")
        .unwrap();
    config
        .set_str("branch.backup-topic.merge", "refs/heads/main")
        .unwrap();
    drop(config);

    let mut affected_branches: Vec<_> = remote_upstreams(&repo, "origin")
        .unwrap()
        .into_iter()
        .map(|upstream| upstream.local_branch)
        .collect();
    affected_branches.sort();
    let mut expected_branches = vec![current_branch.clone(), "topic".to_string()];
    expected_branches.sort();
    assert_eq!(affected_branches, expected_branches);

    remove_remote_and_clear_upstreams(&repo, "origin").unwrap();

    assert!(repo.find_remote("origin").is_err());
    assert!(remote_upstreams(&repo, "origin").unwrap().is_empty());
    assert_eq!(
        remote_upstreams(&repo, "backup").unwrap()[0].local_branch,
        "backup-topic"
    );
}
