mod common;

use git_core::remote::{
    add_remote, clear_current_upstream, clear_remote_auth_profile, create_tag, current_upstream,
    delete_tag, fetch_remote, list_remote_branches, list_remotes, list_tags, pull_after_fetch, push_current_branch,
    push_tags, remote_upstreams, remove_remote, remove_remote_and_clear_upstreams, rename_remote,
    set_current_upstream, set_remote_auth_profile, update_remote_urls, CredentialProvider,
    PullOutcome, RemoteAuthMode, RemoteError, TransferErrorKind, TransferOperation, TransferPhase,
    TransferProgress, TransferReporter,
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
fn classifies_the_known_missing_credential_callback_error_without_exposing_it() {
    let error = RemoteError::from(git2::Error::from_str("missing credential for remote"));

    assert_eq!(
        error.transfer_error_kind(),
        TransferErrorKind::MissingCredential
    );
}

#[test]
fn does_not_classify_a_wrapped_remote_diagnostic_as_missing_credential() {
    let error = RemoteError::from(git2::Error::from_str(
        "remote reported missing credential for remote at https://alice:secret@example.test/repo.git",
    ));

    assert_eq!(
        error.transfer_error_kind(),
        TransferErrorKind::TransferFailed
    );
}

struct RemoteFixture {
    source_dir: tempfile::TempDir,
    remote_dir: tempfile::TempDir,
    local_dir: tempfile::TempDir,
    source: git2::Repository,
    local: git2::Repository,
}

impl RemoteFixture {
    fn remote_commit(&self, message: &str) {
        common::write_file(self.source_dir.path(), "remote.txt", message);
        common::commit_all(&self.source, message);

        let branch = self.source.head().unwrap().shorthand().unwrap().to_string();
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        self.source
            .find_remote("origin")
            .unwrap()
            .push(&[refspec], None)
            .unwrap();
        self.local
            .find_remote("origin")
            .unwrap()
            .fetch(&[] as &[&str], None, None)
            .unwrap();
    }

    fn local_commit(&self, message: &str) {
        common::write_file(self.local_dir.path(), "local.txt", message);
        common::commit_all(&self.local, message);
    }

    fn remote_tip(&self) -> git2::Oid {
        self.source.head().unwrap().target().unwrap()
    }

    fn write_local(&self, path: &str, contents: &str) {
        common::write_file(self.local_dir.path(), path, contents);
    }
}

fn local_and_bare_remote() -> RemoteFixture {
    let (source_dir, source) = common::init_repo();
    source.set_head("refs/heads/main").unwrap();
    common::write_file(source_dir.path(), "README.md", "initial commit\n");
    common::commit_all(&source, "initial commit");

    let remote_dir = tempfile::TempDir::new().unwrap();
    let remote = git2::Repository::init_bare(remote_dir.path()).unwrap();
    let branch = source.head().unwrap().shorthand().unwrap().to_string();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    source
        .remote("origin", remote_dir.path().to_str().unwrap())
        .unwrap();
    source
        .find_remote("origin")
        .unwrap()
        .push(&[refspec], None)
        .unwrap();
    remote.set_head("refs/heads/main").unwrap();
    drop(remote);

    let local_dir = tempfile::TempDir::new().unwrap();
    let local =
        git2::Repository::clone(remote_dir.path().to_str().unwrap(), local_dir.path()).unwrap();
    {
        let mut config = local.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }

    RemoteFixture {
        source_dir,
        remote_dir,
        local_dir,
        source,
        local,
    }
}

fn diverged_local_and_bare_remote() -> RemoteFixture {
    local_and_bare_remote()
}

#[test]
fn lists_a_remote_tracking_branches_without_its_symbolic_head() {
    let fixture = local_and_bare_remote();

    let branches = list_remote_branches(&fixture.local, "origin").unwrap();

    assert_eq!(branches, vec!["main".to_string()]);
}

#[test]
fn tag_crud_preserves_lightweight_and_annotated_tag_metadata() {
    // Removing tag creation, annotation selection, or ref deletion must fail this test.
    let (_dir, repo) = common::init_repo();
    common::write_file(_dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    let target_id = repo.head().unwrap().target().unwrap().to_string();

    create_tag(&repo, "v1.0.0", None).unwrap();
    create_tag(&repo, "release-note", Some("ship it")).unwrap();

    let tags = list_tags(&repo).unwrap();
    assert!(tags.iter().any(|tag| {
        tag.name == "v1.0.0"
            && tag.target_id == target_id
            && !tag.annotated
            && tag.message.is_none()
            && tag.tagger_name.is_none()
            && tag.timestamp.is_none()
    }));
    assert!(tags.iter().any(|tag| {
        tag.name == "release-note"
            && tag.target_id == target_id
            && tag.annotated
            && tag.message.as_deref() == Some("ship it")
            && tag.tagger_name.as_deref() == Some("Test User")
            && tag.timestamp.is_some()
    }));

    delete_tag(&repo, "v1.0.0").unwrap();
    assert!(list_tags(&repo)
        .unwrap()
        .iter()
        .all(|tag| tag.name != "v1.0.0"));
}

#[test]
fn pushes_current_branch_and_only_selected_tag_to_bare_remote() {
    // Omitting either explicit refspec or pushing every local tag must fail this test.
    let fixture = local_and_bare_remote();
    fixture.local_commit("local change");
    create_tag(&fixture.local, "v1.0.0", None).unwrap();
    create_tag(&fixture.local, "not-selected", None).unwrap();
    let mut reporter = VecReporter::default();

    push_current_branch(&fixture.local, "origin", &mut NoCredentials, &mut reporter).unwrap();
    push_tags(
        &fixture.local,
        "origin",
        &["v1.0.0".to_string()],
        &mut NoCredentials,
        &mut reporter,
    )
    .unwrap();

    let remote_repo = git2::Repository::open_bare(fixture.remote_dir.path()).unwrap();
    let local_head = fixture.local.head().unwrap();
    let branch = local_head.shorthand().unwrap();
    assert_eq!(
        remote_repo
            .find_reference(&format!("refs/heads/{branch}"))
            .unwrap()
            .target(),
        fixture.local.head().unwrap().target()
    );
    assert!(remote_repo.find_reference("refs/tags/v1.0.0").is_ok());
    assert!(remote_repo
        .find_reference("refs/tags/not-selected")
        .is_err());
    assert!(reporter.events.iter().any(|event| {
        event.operation == TransferOperation::PushBranch && event.phase == TransferPhase::Receiving
    }));
    assert!(reporter.events.iter().any(|event| {
        event.operation == TransferOperation::PushTags && event.phase == TransferPhase::Receiving
    }));
}

#[test]
fn pushes_all_local_tags_without_using_the_configured_push_refspec() {
    // Passing no explicit refspecs lets libgit2 honor remote.origin.push instead of pushing tags.
    let fixture = local_and_bare_remote();
    create_tag(&fixture.local, "v1.0.0", None).unwrap();
    create_tag(&fixture.local, "v2.0.0", Some("second release")).unwrap();
    fixture
        .local
        .config()
        .unwrap()
        .set_str(
            "remote.origin.push",
            "+refs/heads/main:refs/heads/configured-force",
        )
        .unwrap();
    let mut reporter = VecReporter::default();

    push_tags(
        &fixture.local,
        "origin",
        &[],
        &mut NoCredentials,
        &mut reporter,
    )
    .unwrap();

    let remote_repo = git2::Repository::open_bare(fixture.remote_dir.path()).unwrap();
    assert!(remote_repo.find_reference("refs/tags/v1.0.0").is_ok());
    assert!(remote_repo.find_reference("refs/tags/v2.0.0").is_ok());
    assert!(remote_repo
        .find_reference("refs/heads/configured-force")
        .is_err());
}

#[test]
fn pushing_all_tags_is_a_no_op_when_there_are_no_local_tags() {
    // Calling remote.push with an empty list would execute this configured force refspec.
    let fixture = local_and_bare_remote();
    fixture
        .local
        .config()
        .unwrap()
        .set_str(
            "remote.origin.push",
            "+refs/heads/main:refs/heads/configured-force",
        )
        .unwrap();
    let mut reporter = VecReporter::default();

    push_tags(
        &fixture.local,
        "origin",
        &[],
        &mut NoCredentials,
        &mut reporter,
    )
    .unwrap();

    let remote_repo = git2::Repository::open_bare(fixture.remote_dir.path()).unwrap();
    assert!(remote_repo
        .find_reference("refs/heads/configured-force")
        .is_err());
    assert!(reporter.events.is_empty());
}

#[test]
fn branch_push_rejects_non_fast_forward_updates() {
    // Enabling force on the generated refspec would make this unsafe push succeed.
    let fixture = local_and_bare_remote();
    fixture.remote_commit("remote change");
    fixture.local_commit("local change");
    let mut reporter = VecReporter::default();

    assert!(matches!(
        push_current_branch(&fixture.local, "origin", &mut NoCredentials, &mut reporter),
        Err(RemoteError::NonFastForward)
    ));
    assert_eq!(
        fixture.remote_tip(),
        fixture.source.head().unwrap().target().unwrap()
    );
}

#[test]
fn pull_fast_forwards_a_clean_branch_after_fetch() {
    let fixture = diverged_local_and_bare_remote();
    fixture.remote_commit("remote change");

    let outcome = pull_after_fetch(&fixture.local, "origin", "main").unwrap();

    assert!(matches!(outcome, PullOutcome::FastForwarded { .. }));
    assert_eq!(
        fixture.local.head().unwrap().target(),
        Some(fixture.remote_tip())
    );
}

#[test]
fn pull_rejects_a_dirty_worktree_before_changing_head() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit("remote change");
    fixture.write_local("dirty.txt", "dirty");
    let local_head = fixture.local.head().unwrap().target();

    assert!(matches!(
        pull_after_fetch(&fixture.local, "origin", "main"),
        Err(RemoteError::DirtyWorktree)
    ));
    assert_eq!(fixture.local.head().unwrap().target(), local_head);
    assert_ne!(local_head, Some(fixture.remote_tip()));
}

#[test]
fn pull_rejects_detached_head_without_creating_a_head_branch() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit("remote change");
    let local_head = fixture.local.head().unwrap().target().unwrap();
    fixture.local.set_head_detached(local_head).unwrap();

    assert!(matches!(
        pull_after_fetch(&fixture.local, "origin", "main"),
        Err(RemoteError::DetachedHead)
    ));
    assert!(fixture.local.find_reference("refs/heads/HEAD").is_err());
}

#[test]
fn pull_reports_up_to_date_when_local_matches_tracking_ref() {
    let fixture = local_and_bare_remote();

    assert!(matches!(
        pull_after_fetch(&fixture.local, "origin", "main"),
        Ok(PullOutcome::UpToDate)
    ));
}

#[test]
fn pull_reports_up_to_date_when_local_is_ahead_of_tracking_ref() {
    let fixture = local_and_bare_remote();
    fixture.local_commit("local change");
    let local_head = fixture.local.head().unwrap().target();

    assert!(matches!(
        pull_after_fetch(&fixture.local, "origin", "main"),
        Ok(PullOutcome::UpToDate)
    ));
    assert_eq!(fixture.local.head().unwrap().target(), local_head);
}

#[test]
fn pull_reports_diverged_without_moving_head() {
    let fixture = diverged_local_and_bare_remote();
    fixture.remote_commit("remote change");
    fixture.local_commit("local change");
    let local_head = fixture.local.head().unwrap().target();

    assert!(matches!(
        pull_after_fetch(&fixture.local, "origin", "main"),
        Ok(PullOutcome::Diverged { .. })
    ));
    assert_eq!(fixture.local.head().unwrap().target(), local_head);
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
fn remote_auth_metadata_is_local_non_secret_and_follows_remote_lifecycle() {
    // Removing profile persistence or failing to move/remove it with the remote must fail this.
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "origin", "https://example.com/owner/repo.git", None).unwrap();

    set_remote_auth_profile(
        &repo,
        "origin",
        RemoteAuthMode::HttpsToken {
            username: "rene".to_string(),
        },
    )
    .unwrap();
    let config = repo.config().unwrap();
    assert_eq!(
        config
            .get_string("browsitory.remote.origin.auth-mode")
            .unwrap(),
        "https-token"
    );
    assert_eq!(
        config
            .get_string("browsitory.remote.origin.username")
            .unwrap(),
        "rene"
    );
    assert!(config.get_string("browsitory.remote.origin.token").is_err());
    drop(config);

    rename_remote(&repo, "origin", "upstream").unwrap();
    let config = repo.config().unwrap();
    assert!(config
        .get_string("browsitory.remote.origin.auth-mode")
        .is_err());
    assert_eq!(
        config
            .get_string("browsitory.remote.upstream.auth-mode")
            .unwrap(),
        "https-token"
    );
    assert_eq!(
        config
            .get_string("browsitory.remote.upstream.username")
            .unwrap(),
        "rene"
    );
    drop(config);

    remove_remote(&repo, "upstream").unwrap();
    let config = repo.config().unwrap();
    assert!(config
        .get_string("browsitory.remote.upstream.auth-mode")
        .is_err());
    assert!(config
        .get_string("browsitory.remote.upstream.username")
        .is_err());
    drop(config);

    add_remote(&repo, "origin", "https://example.com/owner/repo.git", None).unwrap();
    set_remote_auth_profile(&repo, "origin", RemoteAuthMode::SshAgent).unwrap();
    clear_remote_auth_profile(&repo, "origin").unwrap();
    let config = repo.config().unwrap();
    assert!(config
        .get_string("browsitory.remote.origin.auth-mode")
        .is_err());
    assert!(config
        .get_string("browsitory.remote.origin.username")
        .is_err());
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
