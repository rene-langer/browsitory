mod common;

use git_core::remote::{
    add_remote, clear_current_upstream, clear_remote_auth_profile, create_tag, current_upstream,
    delete_remote_branch, delete_tag, fetch_all, fetch_remote, force_push_current_branch,
    list_remote_branches, list_remotes, list_tags, pull_after_fetch, push_branch,
    push_current_branch, push_tags, push_target, remote_upstreams, remove_remote,
    remove_remote_and_clear_upstreams, rename_remote, resolve_push_target, set_branch_upstream,
    set_current_upstream, set_remote_auth_profile, update_remote_urls, CredentialProvider,
    FetchAllOutcome, PullOutcome, PushTarget, RemoteAuthMode, RemoteError, TransferErrorKind,
    TransferOperation, TransferPhase, TransferProgress, TransferReporter,
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
        self.remote_commit_file("remote.txt", message);
    }

    fn remote_commit_file(&self, path: &str, message: &str) {
        common::write_file(self.source_dir.path(), path, message);
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

    /// Like `remote_commit`, but does NOT fetch into `local` — the local tracking ref goes stale.
    fn remote_commit_without_fetch(&self, message: &str) {
        common::write_file(self.source_dir.path(), "remote.txt", message);
        common::commit_all(&self.source, message);
        let branch = self.source.head().unwrap().shorthand().unwrap().to_string();
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        self.source
            .find_remote("origin")
            .unwrap()
            .push(&[refspec], None)
            .unwrap();
    }

    fn bare_tip(&self, branch: &str) -> Option<git2::Oid> {
        let bare = git2::Repository::open_bare(self.remote_dir.path()).unwrap();
        bare.find_reference(&format!("refs/heads/{branch}"))
            .ok()
            .and_then(|r| r.target())
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

    push_current_branch(
        &fixture.local,
        "origin",
        &mut NoCredentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    push_tags(
        &fixture.local,
        "origin",
        &["v1.0.0".to_string()],
        &mut NoCredentials,
        &mut reporter,
        &|| false,
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
        &|| false,
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
        &|| false,
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
        push_current_branch(
            &fixture.local,
            "origin",
            &mut NoCredentials,
            &mut reporter,
            &|| false
        ),
        Err(RemoteError::NonFastForward)
    ));
    assert_eq!(
        fixture.remote_tip(),
        fixture.source.head().unwrap().target().unwrap()
    );
}

#[test]
fn pull_aborts_untouched_when_an_ignored_file_blocks_the_checkout() {
    let fixture = diverged_local_and_bare_remote();
    std::fs::create_dir_all(fixture.local.path().join("info")).unwrap();
    std::fs::write(fixture.local.path().join("info/exclude"), "blocker\n").unwrap();
    fixture.write_local("blocker", "precious");
    fixture.remote_commit_file("blocker/inner.txt", "remote change");
    let head_before = fixture.local.head().unwrap().target().unwrap();

    let result = pull_after_fetch(&fixture.local, "origin", "main");

    assert!(matches!(result, Err(RemoteError::CheckoutConflict)));
    assert_eq!(fixture.local.head().unwrap().target().unwrap(), head_before);
    assert_eq!(
        std::fs::read_to_string(fixture.local_dir.path().join("blocker")).unwrap(),
        "precious"
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
        None,
        "fetch-42".to_string(),
        &mut NoCredentials,
        &mut events,
        &|| false,
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
fn a_fetch_aborts_when_the_cancel_flag_is_set_before_the_first_progress_tick() {
    // The cancel flag is the side channel a cancel request writes to while the transfer is
    // already blocked inside git2's network I/O: the transfer-progress callback polls it on
    // every tick and returns `false`, which aborts the transfer from inside libgit2.
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

    let cancelled = std::sync::atomic::AtomicBool::new(true);
    let mut reporter = VecReporter::default();

    let result = fetch_remote(
        &local,
        "origin",
        None,
        "fetch-cancelled".to_string(),
        &mut NoCredentials,
        &mut reporter,
        &|| cancelled.load(std::sync::atomic::Ordering::SeqCst),
    );

    assert_eq!(
        result.unwrap_err().transfer_error_kind(),
        TransferErrorKind::Cancelled
    );
    assert!(local
        .find_reference(&format!("refs/remotes/origin/{branch_name}"))
        .is_err());
}

#[test]
fn a_push_cancelled_before_negotiation_is_reported_cancelled_and_leaves_the_remote_untouched() {
    // `push_negotiation` fires before any pack data is sent; returning an error there is
    // libgit2's documented way to cancel a push without the remote applying anything.
    let fixture = local_and_bare_remote();
    fixture.local_commit("local change");
    let remote_tip_before = fixture.remote_tip();
    let mut reporter = VecReporter::default();

    let result = push_current_branch(
        &fixture.local,
        "origin",
        &mut NoCredentials,
        &mut reporter,
        &|| true,
    );

    assert_eq!(
        result.unwrap_err().transfer_error_kind(),
        TransferErrorKind::Cancelled
    );
    let remote_repo = git2::Repository::open_bare(fixture.remote_dir.path()).unwrap();
    assert_eq!(
        remote_repo
            .find_reference("refs/heads/main")
            .unwrap()
            .target()
            .unwrap(),
        remote_tip_before
    );
}

#[test]
fn a_push_cancelled_after_negotiation_completes_and_reports_its_real_outcome() {
    // Once negotiation has passed, the pack goes out and the remote applies the update before
    // any later callback runs. Aborting there would report "cancelled" for a push that actually
    // landed (and skip the local tracking-ref update), so a late cancel must be ignored.
    let fixture = local_and_bare_remote();
    fixture.local_commit("local change");
    let local_tip = fixture.local.head().unwrap().target().unwrap();
    let polls = std::sync::atomic::AtomicUsize::new(0);
    let mut reporter = VecReporter::default();

    // Not cancelled on the first poll (negotiation), cancelled on every poll after it.
    let result = push_current_branch(
        &fixture.local,
        "origin",
        &mut NoCredentials,
        &mut reporter,
        &|| polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1,
    );

    assert!(result.is_ok(), "late cancel must not fail a landed push");
    assert!(polls.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    let remote_repo = git2::Repository::open_bare(fixture.remote_dir.path()).unwrap();
    assert_eq!(
        remote_repo
            .find_reference("refs/heads/main")
            .unwrap()
            .target()
            .unwrap(),
        local_tip
    );
    assert_eq!(
        fixture
            .local
            .find_reference("refs/remotes/origin/main")
            .unwrap()
            .target()
            .unwrap(),
        local_tip
    );
}

#[test]
fn a_cancelled_fetch_is_not_reported_as_a_generic_transfer_failure() {
    // Guards the classification: an uncancelled failure against the same fixture must still be
    // `TransferFailed`, so `Cancelled` can't be produced by simply mapping every fetch error.
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    let missing_dir = tempfile::TempDir::new().unwrap();
    let missing_path = missing_dir.path().join("does-not-exist");
    repo.remote("origin", missing_path.to_str().unwrap())
        .unwrap();
    let mut reporter = VecReporter::default();

    let result = fetch_remote(
        &repo,
        "origin",
        None,
        "fetch-failed".to_string(),
        &mut NoCredentials,
        &mut reporter,
        &|| false,
    );

    assert_eq!(
        result.unwrap_err().transfer_error_kind(),
        TransferErrorKind::TransferFailed
    );
}

#[test]
fn fetch_from_an_unreachable_remote_fails_without_creating_a_tracking_ref() {
    // A bad transport must surface as an error, not panic or silently create refs.
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    let missing_dir = tempfile::TempDir::new().unwrap();
    let missing_path = missing_dir.path().join("does-not-exist");
    repo.remote("origin", missing_path.to_str().unwrap())
        .unwrap();
    let mut reporter = VecReporter::default();

    let result = fetch_remote(
        &repo,
        "origin",
        None,
        "fetch-unreachable".to_string(),
        &mut NoCredentials,
        &mut reporter,
        &|| false,
    );

    assert!(matches!(result, Err(RemoteError::Git(_))));
    assert!(repo.find_reference("refs/remotes/origin/main").is_err());
}

#[test]
fn push_to_an_unreachable_remote_fails_without_moving_local_head() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    let missing_dir = tempfile::TempDir::new().unwrap();
    let missing_path = missing_dir.path().join("does-not-exist");
    repo.remote("origin", missing_path.to_str().unwrap())
        .unwrap();
    let local_head = repo.head().unwrap().target();
    let mut reporter = VecReporter::default();

    let result = push_current_branch(&repo, "origin", &mut NoCredentials, &mut reporter, &|| {
        false
    });

    assert!(matches!(result, Err(RemoteError::Git(_))));
    assert_eq!(repo.head().unwrap().target(), local_head);
}

#[test]
fn pull_fails_when_the_remote_branch_was_never_fetched() {
    // No refs/remotes/origin/main exists yet, so there is nothing to compare against.
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    add_remote(&repo, "origin", "file:///tmp/does-not-matter.git", None).unwrap();
    let local_head = repo.head().unwrap().target();

    let result = pull_after_fetch(&repo, "origin", "main");

    assert!(matches!(result, Err(RemoteError::Git(_))));
    assert_eq!(repo.head().unwrap().target(), local_head);
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
fn current_upstream_returns_none_on_a_detached_head_instead_of_erroring() {
    // A detached HEAD is normal mid-rebase/mid-checkout-of-a-commit state, not a user error —
    // `refresh()`'s status poll (frontend `useAppState.ts`) calls this on every mutation via
    // `Promise.all`, and a detached HEAD literally has no upstream to report, so this must
    // succeed with `None` rather than reuse `pull`'s "cannot pull while HEAD is detached" error.
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    let head_id = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(head_id).unwrap();

    assert_eq!(current_upstream(&repo).unwrap(), None);
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

#[test]
fn current_upstream_reports_ahead_and_behind_counts_against_the_tracking_ref() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "1");
    common::commit_all(&repo, "first");
    let first = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "2");
    common::commit_all(&repo, "second");
    let second = repo.head().unwrap().target().unwrap();
    let head_ref = repo.head().unwrap().name().unwrap().to_string();

    add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();
    set_current_upstream(&repo, "origin", "main").unwrap();

    // No tracking ref fetched yet: counts are unknown, not zero.
    let upstream = current_upstream(&repo).unwrap().unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (None, None));

    // Remote at `first`, local at `second`: one commit ahead.
    repo.reference("refs/remotes/origin/main", first, true, "test")
        .unwrap();
    let upstream = current_upstream(&repo).unwrap().unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (Some(1), Some(0)));

    // Remote at `second`, local rewound to `first`: one commit behind.
    repo.reference("refs/remotes/origin/main", second, true, "test")
        .unwrap();
    repo.reference(&head_ref, first, true, "rewind").unwrap();
    let upstream = current_upstream(&repo).unwrap().unwrap();
    assert_eq!((upstream.ahead, upstream.behind), (Some(0), Some(1)));
}

#[test]
fn force_push_overwrites_the_remote_when_the_lease_matches() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit("remote change"); // fetched: tracking ref up to date
    fixture.local_commit("local change"); // diverged
    let mut reporter = VecReporter::default();

    force_push_current_branch(
        &fixture.local,
        "origin",
        &mut NoCredentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    let local_tip = fixture.local.head().unwrap().target().unwrap();
    assert_eq!(fixture.bare_tip("main"), Some(local_tip));
    assert_eq!(
        fixture
            .local
            .find_reference("refs/remotes/origin/main")
            .unwrap()
            .target(),
        Some(local_tip)
    );
    assert!(reporter
        .events
        .iter()
        .all(|e| e.operation == TransferOperation::ForcePushBranch));
}

#[test]
fn force_push_refuses_a_stale_lease_and_leaves_the_remote_untouched() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit_without_fetch("someone else's work");
    fixture.local_commit("local change");
    let remote_before = fixture.bare_tip("main");
    let mut reporter = VecReporter::default();

    let result = force_push_current_branch(
        &fixture.local,
        "origin",
        &mut NoCredentials,
        &mut reporter,
        &|| false,
    );

    assert!(matches!(result, Err(RemoteError::StaleLease)));
    assert_eq!(
        result.unwrap_err().transfer_error_kind(),
        TransferErrorKind::StaleLease
    );
    assert_eq!(fixture.bare_tip("main"), remote_before);
}

#[test]
fn force_push_of_a_branch_new_to_the_remote_succeeds() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("brand-new", &head, false).unwrap();
    fixture.local.set_head("refs/heads/brand-new").unwrap();
    fixture.local_commit("on new branch");
    let mut reporter = VecReporter::default();

    force_push_current_branch(
        &fixture.local,
        "origin",
        &mut NoCredentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    assert_eq!(
        fixture.bare_tip("brand-new"),
        Some(fixture.local.head().unwrap().target().unwrap())
    );
}

#[test]
fn force_push_refuses_when_remote_has_the_branch_but_it_was_never_fetched() {
    let fixture = local_and_bare_remote();
    // Remote gains `other`; local never fetches it, then creates its own `other`.
    let src_head = fixture.source.head().unwrap().peel_to_commit().unwrap();
    fixture.source.branch("other", &src_head, false).unwrap();
    fixture
        .source
        .find_remote("origin")
        .unwrap()
        .push(&["refs/heads/other:refs/heads/other"], None)
        .unwrap();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("other", &head, false).unwrap();
    fixture.local.set_head("refs/heads/other").unwrap();
    fixture.local_commit("local other");
    let mut reporter = VecReporter::default();

    assert!(matches!(
        force_push_current_branch(
            &fixture.local,
            "origin",
            &mut NoCredentials,
            &mut reporter,
            &|| false
        ),
        Err(RemoteError::StaleLease)
    ));
}

#[test]
fn force_push_refuses_a_detached_head() {
    let fixture = local_and_bare_remote();
    let oid = fixture.local.head().unwrap().target().unwrap();
    fixture.local.set_head_detached(oid).unwrap();
    let mut reporter = VecReporter::default();
    assert!(matches!(
        force_push_current_branch(
            &fixture.local,
            "origin",
            &mut NoCredentials,
            &mut reporter,
            &|| false
        ),
        Err(RemoteError::DetachedHead)
    ));
}

fn quiet() -> (NoCredentials, VecReporter) {
    (NoCredentials, VecReporter::default())
}

#[test]
fn pushing_a_non_current_branch_without_upstream_creates_only_the_remote_branch() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("topic", &head, false).unwrap();
    let main_before = fixture.bare_tip("main");
    let (mut credentials, mut reporter) = quiet();

    push_branch(
        &fixture.local,
        "origin",
        "topic",
        "topic",
        false,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    assert_eq!(fixture.bare_tip("topic"), Some(head.id()));
    assert_eq!(fixture.bare_tip("main"), main_before);
    assert_eq!(fixture.local.head().unwrap().shorthand().unwrap(), "main");
    assert!(fixture
        .local
        .config()
        .unwrap()
        .get_string("branch.topic.remote")
        .is_err());
}

#[test]
fn pushing_with_set_upstream_records_the_upstream_under_the_chosen_name() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("topic", &head, false).unwrap();
    let (mut credentials, mut reporter) = quiet();

    push_branch(
        &fixture.local,
        "origin",
        "topic",
        "published",
        true,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    assert_eq!(fixture.bare_tip("published"), Some(head.id()));
    assert_eq!(fixture.bare_tip("topic"), None);
    let config = fixture.local.config().unwrap();
    assert_eq!(config.get_string("branch.topic.remote").unwrap(), "origin");
    assert_eq!(
        config.get_string("branch.topic.merge").unwrap(),
        "refs/heads/published"
    );
    let topic = git_core::branch::list_branches(&fixture.local)
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == "topic")
        .unwrap();
    assert_eq!(topic.upstream.as_deref(), Some("origin/published"));
    assert_eq!((topic.ahead, topic.behind), (Some(0), Some(0)));
}

#[test]
fn a_rejected_push_with_set_upstream_leaves_the_upstream_unset() {
    let fixture = local_and_bare_remote();
    let old = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("old", &old, false).unwrap();
    fixture.remote_commit("remote moves on");
    let main_before = fixture.bare_tip("main");
    // `old` lacks the remote's new commit, so pushing it onto main is not a fast-forward.
    let (mut credentials, mut reporter) = quiet();

    let result = push_branch(
        &fixture.local,
        "origin",
        "old",
        "main",
        true,
        &mut credentials,
        &mut reporter,
        &|| false,
    );

    assert!(matches!(result, Err(RemoteError::NonFastForward)));
    assert_eq!(fixture.bare_tip("main"), main_before);
    assert!(fixture
        .local
        .config()
        .unwrap()
        .get_string("branch.old.remote")
        .is_err());
}

#[test]
fn an_invalid_remote_branch_name_is_rejected_before_connecting() {
    let fixture = local_and_bare_remote();
    let (mut credentials, mut reporter) = quiet();

    for name in ["bad name", "", "HEAD", "-x", "a..b"] {
        let result = push_branch(
            &fixture.local,
            "origin",
            "main",
            name,
            false,
            &mut credentials,
            &mut reporter,
            &|| false,
        );
        assert!(matches!(result, Err(RemoteError::Git(_))), "{name:?}");
    }
    assert!(reporter.events.is_empty());
    assert_eq!(fixture.bare_tip("bad name"), None);
}

#[test]
fn pushing_a_missing_local_branch_is_rejected_before_connecting() {
    let fixture = local_and_bare_remote();
    let (mut credentials, mut reporter) = quiet();

    let result = push_branch(
        &fixture.local,
        "origin",
        "nope",
        "nope",
        false,
        &mut credentials,
        &mut reporter,
        &|| false,
    );

    assert!(matches!(result, Err(RemoteError::Git(_))));
    assert!(reporter.events.is_empty());
    assert_eq!(fixture.bare_tip("nope"), None);
}

#[test]
fn push_current_branch_under_push_default_upstream_targets_the_upstream_branch_name() {
    let fixture = local_and_bare_remote();
    fixture.local_commit("local work");
    let local_tip = fixture.local.head().unwrap().target().unwrap();
    let main_before = fixture.bare_tip("main");
    set_current_upstream(&fixture.local, "origin", "renamed").unwrap();
    fixture
        .local
        .config()
        .unwrap()
        .set_str("push.default", "upstream")
        .unwrap();
    let (mut credentials, mut reporter) = quiet();

    push_current_branch(
        &fixture.local,
        "origin",
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    assert_eq!(fixture.bare_tip("renamed"), Some(local_tip));
    assert_eq!(fixture.bare_tip("main"), main_before);
}

#[test]
fn push_current_branch_to_another_remote_keeps_the_local_name() {
    let fixture = local_and_bare_remote();
    let other_dir = tempfile::TempDir::new().unwrap();
    git2::Repository::init_bare(other_dir.path()).unwrap();
    fixture
        .local
        .remote("other", other_dir.path().to_str().unwrap())
        .unwrap();
    set_current_upstream(&fixture.local, "origin", "renamed").unwrap();
    let local_tip = fixture.local.head().unwrap().target().unwrap();
    let (mut credentials, mut reporter) = quiet();

    push_current_branch(
        &fixture.local,
        "other",
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    let other = git2::Repository::open_bare(other_dir.path()).unwrap();
    assert_eq!(
        other.find_reference("refs/heads/main").unwrap().target(),
        Some(local_tip)
    );
    assert!(other.find_reference("refs/heads/renamed").is_err());
}

#[test]
fn force_push_under_push_default_upstream_overwrites_the_differently_named_upstream() {
    let fixture = local_and_bare_remote();
    fixture
        .local
        .config()
        .unwrap()
        .set_str("push.default", "upstream")
        .unwrap();
    fixture.local_commit("a");
    let (mut credentials, mut reporter) = quiet();
    push_branch(
        &fixture.local,
        "origin",
        "main",
        "renamed",
        true,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    let base = fixture
        .local
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent(0)
        .unwrap();
    fixture
        .local
        .reset(base.as_object(), git2::ResetType::Hard, None)
        .unwrap();
    fixture.write_local("local.txt", "b");
    common::commit_all(&fixture.local, "b");
    let rewritten = fixture.local.head().unwrap().target().unwrap();
    let main_before = fixture.bare_tip("main");

    force_push_current_branch(
        &fixture.local,
        "origin",
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    assert_eq!(fixture.bare_tip("renamed"), Some(rewritten));
    assert_eq!(fixture.bare_tip("main"), main_before);
}

#[test]
fn force_push_under_push_default_upstream_leases_the_differently_named_upstream() {
    let fixture = local_and_bare_remote();
    fixture
        .local
        .config()
        .unwrap()
        .set_str("push.default", "upstream")
        .unwrap();
    fixture.local_commit("a");
    let (mut credentials, mut reporter) = quiet();
    push_branch(
        &fixture.local,
        "origin",
        "main",
        "renamed",
        true,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    let published = fixture.bare_tip("renamed");
    // Someone else moves `renamed` on the remote; our tracking ref for it is now stale. `main`
    // on the remote is untouched, so a lease against `origin/main` would wrongly pass.
    {
        let bare = git2::Repository::open_bare(fixture.remote_dir.path()).unwrap();
        let main = bare
            .find_reference("refs/heads/main")
            .unwrap()
            .target()
            .unwrap();
        bare.reference("refs/heads/renamed", main, true, "someone else")
            .unwrap();
    }
    let moved = fixture.bare_tip("renamed");
    assert_ne!(moved, published);
    fixture.local_commit("b");

    let result = force_push_current_branch(
        &fixture.local,
        "origin",
        &mut credentials,
        &mut reporter,
        &|| false,
    );

    assert!(matches!(result, Err(RemoteError::StaleLease)));
    assert_eq!(fixture.bare_tip("renamed"), moved);
}

#[test]
fn deleting_a_remote_branch_removes_the_remote_ref_and_the_tracking_ref() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("doomed", &head, false).unwrap();
    fixture.local.branch("doomed-too", &head, false).unwrap();
    let (mut credentials, mut reporter) = quiet();
    push_branch(
        &fixture.local,
        "origin",
        "doomed",
        "doomed",
        false,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    push_branch(
        &fixture.local,
        "origin",
        "doomed-too",
        "doomed-too",
        false,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    assert!(fixture.bare_tip("doomed").is_some());
    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/doomed")
        .is_ok());
    let main_before = fixture.bare_tip("main");
    let mut events = VecReporter::default();

    delete_remote_branch(
        &fixture.local,
        "origin",
        "doomed",
        &mut credentials,
        &mut events,
        &|| false,
    )
    .unwrap();

    assert_eq!(fixture.bare_tip("doomed"), None);
    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/doomed")
        .is_err());
    assert!(fixture
        .local
        .find_branch("doomed", git2::BranchType::Local)
        .is_ok());
    // Exactly that ref: siblings (including a name it prefixes) and main are untouched.
    assert_eq!(fixture.bare_tip("doomed-too"), Some(head.id()));
    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/doomed-too")
        .is_ok());
    assert_eq!(fixture.bare_tip("main"), main_before);
    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/main")
        .is_ok());
    assert!(events
        .events
        .iter()
        .all(|event| event.operation == TransferOperation::DeleteRemoteBranch));
}

#[test]
fn deleting_a_remote_branch_keeps_the_upstream_config_of_branches_tracking_it() {
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("topic", &head, false).unwrap();
    let (mut credentials, mut reporter) = quiet();
    push_branch(
        &fixture.local,
        "origin",
        "topic",
        "topic",
        true,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    delete_remote_branch(
        &fixture.local,
        "origin",
        "topic",
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    let topic = git_core::branch::list_branches(&fixture.local)
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == "topic")
        .unwrap();
    assert_eq!(topic.upstream.as_deref(), Some("origin/topic"));
    assert_eq!((topic.ahead, topic.behind), (None, None));
}

#[test]
fn a_remote_branch_delete_cancelled_at_negotiation_leaves_both_refs() {
    let fixture = local_and_bare_remote();
    let (mut credentials, mut reporter) = quiet();

    let result = delete_remote_branch(
        &fixture.local,
        "origin",
        "main",
        &mut credentials,
        &mut reporter,
        &|| true,
    );

    assert!(matches!(result, Err(RemoteError::Cancelled)));
    assert!(fixture.bare_tip("main").is_some());
    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/main")
        .is_ok());
}

#[test]
fn deleting_head_or_an_invalid_name_is_rejected_before_connecting() {
    let fixture = local_and_bare_remote();
    let (mut credentials, mut reporter) = quiet();

    for name in ["HEAD", "", "bad name", "-x", "main:other", "+main"] {
        let result = delete_remote_branch(
            &fixture.local,
            "origin",
            name,
            &mut credentials,
            &mut reporter,
            &|| false,
        );
        assert!(matches!(result, Err(RemoteError::Git(_))), "{name:?}");
    }
    assert!(reporter.events.is_empty());
    assert!(fixture.bare_tip("main").is_some());
}

fn no_credentials_for(_remote: &str) -> Result<Box<dyn CredentialProvider>, RemoteError> {
    Ok(Box::new(NoCredentials))
}

/// Publishes `branch` from the source repo, fetches it into `local`, then deletes it on the
/// remote, leaving `refs/remotes/origin/<branch>` stale in `local`.
fn stale_tracking_ref(fixture: &RemoteFixture, branch: &str) {
    let head = fixture.source.head().unwrap().peel_to_commit().unwrap();
    fixture.source.branch(branch, &head, false).unwrap();
    let mut origin = fixture.source.find_remote("origin").unwrap();
    origin
        .push(&[format!("refs/heads/{branch}:refs/heads/{branch}")], None)
        .unwrap();
    fixture
        .local
        .find_remote("origin")
        .unwrap()
        .fetch(&[] as &[&str], None, None)
        .unwrap();
    assert!(fixture
        .local
        .find_reference(&format!("refs/remotes/origin/{branch}"))
        .is_ok());
    origin
        .push(&[format!(":refs/heads/{branch}")], None)
        .unwrap();
}

#[test]
fn fetch_with_prune_removes_a_remote_deleted_branch_and_without_keeps_it() {
    let fixture = local_and_bare_remote();
    stale_tracking_ref(&fixture, "gone");
    let mut reporter = VecReporter::default();

    fetch_remote(
        &fixture.local,
        "origin",
        Some(false),
        "f-1".into(),
        &mut NoCredentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/gone")
        .is_ok());

    fetch_remote(
        &fixture.local,
        "origin",
        Some(true),
        "f-2".into(),
        &mut NoCredentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/gone")
        .is_err());
    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/main")
        .is_ok());
}

#[test]
fn fetch_with_unspecified_prune_follows_fetch_prune_config() {
    let fixture = local_and_bare_remote();
    stale_tracking_ref(&fixture, "gone");
    fixture
        .local
        .config()
        .unwrap()
        .set_bool("fetch.prune", true)
        .unwrap();

    fetch_remote(
        &fixture.local,
        "origin",
        None,
        "f-1".into(),
        &mut NoCredentials,
        &mut VecReporter::default(),
        &|| false,
    )
    .unwrap();

    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/gone")
        .is_err());
}

#[test]
fn fetch_with_explicit_no_prune_overrides_fetch_prune_config() {
    let fixture = local_and_bare_remote();
    stale_tracking_ref(&fixture, "gone");
    fixture
        .local
        .config()
        .unwrap()
        .set_bool("fetch.prune", true)
        .unwrap();

    fetch_remote(
        &fixture.local,
        "origin",
        Some(false),
        "f-1".into(),
        &mut NoCredentials,
        &mut VecReporter::default(),
        &|| false,
    )
    .unwrap();

    assert!(fixture
        .local
        .find_reference("refs/remotes/origin/gone")
        .is_ok());
}

#[test]
fn remote_info_prune_prefers_the_remote_setting_over_fetch_prune() {
    let (_dir, repo) = common::init_repo();
    add_remote(&repo, "a", "file:///tmp/a.git", None).unwrap();
    add_remote(&repo, "b", "file:///tmp/b.git", None).unwrap();
    // Repo-local, so a developer's global `fetch.prune` cannot leak in.
    repo.config()
        .unwrap()
        .set_bool("fetch.prune", false)
        .unwrap();
    assert!(list_remotes(&repo)
        .unwrap()
        .iter()
        .all(|remote| !remote.prune));
    let mut config = repo.config().unwrap();
    config.set_bool("fetch.prune", true).unwrap();
    config.set_bool("remote.b.prune", false).unwrap();

    let remotes = list_remotes(&repo).unwrap();

    assert!(
        remotes
            .iter()
            .find(|remote| remote.name == "a")
            .unwrap()
            .prune
    );
    assert!(
        !remotes
            .iter()
            .find(|remote| remote.name == "b")
            .unwrap()
            .prune
    );
}

#[test]
fn fetch_all_records_an_unreachable_remote_and_fetches_the_rest() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit_without_fetch("only on the remote");
    let missing = tempfile::TempDir::new().unwrap();
    fixture
        .local
        .remote("broken", missing.path().join("nope").to_str().unwrap())
        .unwrap();
    let mut reporter = VecReporter::default();

    let outcome = fetch_all(
        &fixture.local,
        None,
        "all-1".into(),
        &mut no_credentials_for,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    assert_eq!(outcome.fetched, vec!["origin".to_string()]);
    assert_eq!(
        outcome.failed,
        vec![("broken".to_string(), TransferErrorKind::TransferFailed)]
    );
    assert_eq!(
        fixture
            .local
            .find_reference("refs/remotes/origin/main")
            .unwrap()
            .target(),
        Some(fixture.remote_tip())
    );
    assert!(!reporter.events.is_empty());
    assert!(reporter.events.iter().all(|event| {
        event.operation_id == "all-1" && event.operation == TransferOperation::FetchAll
    }));
}

#[test]
fn fetch_all_records_a_remote_whose_credentials_cannot_be_built_and_fetches_the_rest() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit_without_fetch("only on the remote");
    let other_dir = tempfile::TempDir::new().unwrap();
    git2::Repository::init_bare(other_dir.path()).unwrap();
    // "aaa" sorts first, so a failure there must not stop origin from being fetched.
    fixture
        .local
        .remote("aaa", other_dir.path().to_str().unwrap())
        .unwrap();
    let mut credentials_for = |remote: &str| -> Result<Box<dyn CredentialProvider>, RemoteError> {
        if remote == "aaa" {
            Err(git2::Error::from_str("missing credential for remote").into())
        } else {
            Ok(Box::new(NoCredentials))
        }
    };

    let outcome = fetch_all(
        &fixture.local,
        None,
        "all-1".into(),
        &mut credentials_for,
        &mut VecReporter::default(),
        &|| false,
    )
    .unwrap();

    assert_eq!(outcome.fetched, vec!["origin".to_string()]);
    assert_eq!(
        outcome.failed,
        vec![("aaa".to_string(), TransferErrorKind::MissingCredential)]
    );
}

#[test]
fn fetch_all_without_remotes_is_an_empty_success() {
    let (_dir, repo) = common::init_repo();

    let outcome = fetch_all(
        &repo,
        None,
        "all-1".into(),
        &mut no_credentials_for,
        &mut VecReporter::default(),
        &|| false,
    )
    .unwrap();

    assert_eq!(outcome, FetchAllOutcome::default());
}

#[test]
fn fetch_all_cancelled_before_the_first_remote_fetches_nothing() {
    let fixture = local_and_bare_remote();
    fixture.remote_commit_without_fetch("only on the remote");
    let before = fixture
        .local
        .find_reference("refs/remotes/origin/main")
        .unwrap()
        .target();

    let result = fetch_all(
        &fixture.local,
        None,
        "all-1".into(),
        &mut no_credentials_for,
        &mut VecReporter::default(),
        &|| true,
    );

    assert!(matches!(result, Err(RemoteError::Cancelled)));
    assert_eq!(
        fixture
            .local
            .find_reference("refs/remotes/origin/main")
            .unwrap()
            .target(),
        before
    );
}

fn target(remote: &str, branch: &str) -> PushTarget {
    PushTarget::Branch {
        remote: remote.into(),
        branch: branch.into(),
    }
}

fn choice(remote: &str, local: &str) -> PushTarget {
    PushTarget::NeedsChoice {
        remote: remote.into(),
        local_branch: local.into(),
    }
}

#[test]
fn push_default_decides_the_target_of_a_differently_named_upstream() {
    let renamed = Some(("origin", "feature-x"));
    // Unset push.default is git's default, `simple`.
    for mode in [None, Some("simple"), Some("matching")] {
        assert_eq!(
            resolve_push_target(mode, "fx", "origin", renamed),
            choice("origin", "fx"),
            "{mode:?}"
        );
    }
    assert_eq!(
        resolve_push_target(Some("nothing"), "fx", "origin", renamed),
        choice("origin", "fx")
    );
    assert_eq!(
        resolve_push_target(Some("upstream"), "fx", "origin", renamed),
        target("origin", "feature-x")
    );
    assert_eq!(
        resolve_push_target(Some("tracking"), "fx", "origin", renamed),
        target("origin", "feature-x")
    );
    assert_eq!(
        resolve_push_target(Some("current"), "fx", "origin", renamed),
        target("origin", "fx")
    );
}

#[test]
fn push_default_pushes_a_same_named_upstream_and_other_remotes_under_the_local_name() {
    let same = Some(("origin", "main"));
    for mode in [
        None,
        Some("simple"),
        Some("matching"),
        Some("upstream"),
        Some("current"),
    ] {
        assert_eq!(
            resolve_push_target(mode, "main", "origin", same),
            target("origin", "main"),
            "{mode:?}"
        );
        // An explicitly chosen remote that is not the upstream's (or no upstream at all).
        assert_eq!(
            resolve_push_target(mode, "main", "backup", same),
            target("backup", "main"),
            "{mode:?}"
        );
        assert_eq!(
            resolve_push_target(mode, "main", "origin", None),
            target("origin", "main"),
            "{mode:?}"
        );
    }
    assert_eq!(
        resolve_push_target(Some("nothing"), "main", "origin", same),
        choice("origin", "main")
    );
    assert_eq!(
        resolve_push_target(Some("nothing"), "main", "backup", None),
        choice("backup", "main")
    );
}

#[test]
fn push_target_reads_push_default_and_the_upstream_config() {
    let fixture = local_and_bare_remote();
    set_current_upstream(&fixture.local, "origin", "feature-x").unwrap();
    let mut config = fixture.local.config().unwrap();
    config.set_str("push.default", "simple").unwrap();
    assert_eq!(
        push_target(&fixture.local, "main", "origin").unwrap(),
        choice("origin", "main")
    );
    assert_eq!(
        current_upstream(&fixture.local)
            .unwrap()
            .unwrap()
            .push_branch,
        None
    );
    config.set_str("push.default", "upstream").unwrap();
    assert_eq!(
        push_target(&fixture.local, "main", "origin").unwrap(),
        target("origin", "feature-x")
    );
    let upstream = current_upstream(&fixture.local).unwrap().unwrap();
    assert_eq!(upstream.push_branch.as_deref(), Some("feature-x"));
}

#[test]
fn push_current_branch_with_a_differently_named_upstream_needs_a_choice_by_default() {
    // A branch created from origin/main tracks origin/main; `simple` must never push it there.
    let fixture = local_and_bare_remote();
    fixture
        .local
        .config()
        .unwrap()
        .set_str("push.default", "simple")
        .unwrap();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("feat", &head, false).unwrap();
    fixture.local.set_head("refs/heads/feat").unwrap();
    set_current_upstream(&fixture.local, "origin", "main").unwrap();
    fixture.local_commit("feature work");
    let main_before = fixture.bare_tip("main");
    let (mut credentials, mut reporter) = quiet();

    let pushed = push_current_branch(
        &fixture.local,
        "origin",
        &mut credentials,
        &mut reporter,
        &|| false,
    );
    let forced = force_push_current_branch(
        &fixture.local,
        "origin",
        &mut credentials,
        &mut reporter,
        &|| false,
    );

    for result in [pushed, forced] {
        assert!(matches!(
            result,
            Err(RemoteError::PushTargetNeedsChoice { .. })
        ));
    }
    assert!(reporter.events.is_empty());
    assert_eq!(fixture.bare_tip("main"), main_before);
    assert_eq!(fixture.bare_tip("feat"), None);
}

#[test]
fn push_current_branch_under_push_default_current_pushes_the_same_name() {
    let fixture = local_and_bare_remote();
    fixture
        .local
        .config()
        .unwrap()
        .set_str("push.default", "current")
        .unwrap();
    set_current_upstream(&fixture.local, "origin", "renamed").unwrap();
    fixture.local_commit("local work");
    let local_tip = fixture.local.head().unwrap().target().unwrap();
    let (mut credentials, mut reporter) = quiet();

    push_current_branch(
        &fixture.local,
        "origin",
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    assert_eq!(fixture.bare_tip("main"), Some(local_tip));
    assert_eq!(fixture.bare_tip("renamed"), None);
}

#[test]
fn push_current_branch_under_push_default_nothing_needs_a_choice() {
    let fixture = local_and_bare_remote();
    fixture
        .local
        .config()
        .unwrap()
        .set_str("push.default", "nothing")
        .unwrap();
    let main_before = fixture.bare_tip("main");
    fixture.local_commit("local work");
    let (mut credentials, mut reporter) = quiet();

    let result = push_current_branch(
        &fixture.local,
        "origin",
        &mut credentials,
        &mut reporter,
        &|| false,
    );

    assert!(matches!(
        result,
        Err(RemoteError::PushTargetNeedsChoice { .. })
    ));
    assert_eq!(fixture.bare_tip("main"), main_before);
}

#[test]
fn force_push_validates_the_resolved_target_before_connecting() {
    let fixture = local_and_bare_remote();
    let mut config = fixture.local.config().unwrap();
    config.set_str("push.default", "upstream").unwrap();
    config
        .set_str("branch.main.merge", "refs/heads/bad name")
        .unwrap();
    let (mut credentials, mut reporter) = quiet();

    let result = force_push_current_branch(
        &fixture.local,
        "origin",
        &mut credentials,
        &mut reporter,
        &|| false,
    );

    assert!(matches!(result, Err(RemoteError::Git(_))));
    assert!(reporter.events.is_empty());
}

#[test]
fn setting_an_upstream_is_not_fooled_by_a_local_branch_named_like_the_remote_branch() {
    // libgit2's Branch::set_upstream resolves "origin/topic" as a LOCAL branch first, which wrote
    // `remote = .` and `merge = refs/heads/origin/topic`.
    let fixture = local_and_bare_remote();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("origin/topic", &head, false).unwrap();
    fixture.local.branch("other", &head, false).unwrap();
    fixture
        .local
        .reference("refs/remotes/origin/topic", head.id(), true, "test")
        .unwrap();

    set_branch_upstream(&fixture.local, "other", "origin", "topic").unwrap();
    set_current_upstream(&fixture.local, "origin", "topic").unwrap();

    let config = fixture.local.config().unwrap();
    for branch in ["other", "main"] {
        assert_eq!(
            config
                .get_string(&format!("branch.{branch}.remote"))
                .unwrap(),
            "origin"
        );
        assert_eq!(
            config
                .get_string(&format!("branch.{branch}.merge"))
                .unwrap(),
            "refs/heads/topic"
        );
    }
}

#[test]
fn setting_an_upstream_rejects_a_missing_local_branch_or_remote() {
    let fixture = local_and_bare_remote();
    assert!(set_branch_upstream(&fixture.local, "nope", "origin", "topic").is_err());
    assert!(set_branch_upstream(&fixture.local, "main", "nope", "topic").is_err());
}

#[test]
fn deleting_a_remote_branch_removes_the_tracking_ref_its_fetch_refspec_maps_to() {
    let fixture = local_and_bare_remote();
    let url = fixture.remote_dir.path().to_str().unwrap().to_string();
    fixture
        .local
        .remote_with_fetch("mirror", &url, "+refs/heads/*:refs/remotes/mirror-ns/*")
        .unwrap();
    fixture
        .local
        .remote_with_fetch("narrow", &url, "+refs/heads/main:refs/remotes/narrow/main")
        .unwrap();
    let head = fixture.local.head().unwrap().peel_to_commit().unwrap();
    fixture.local.branch("doomed", &head, false).unwrap();
    let (mut credentials, mut reporter) = quiet();
    push_branch(
        &fixture.local,
        "origin",
        "doomed",
        "doomed",
        false,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    fixture
        .local
        .find_remote("mirror")
        .unwrap()
        .fetch(&[] as &[&str], None, None)
        .unwrap();
    assert!(fixture
        .local
        .find_reference("refs/remotes/mirror-ns/doomed")
        .is_ok());
    // Decoys at the conventional `refs/remotes/<remote>/<branch>` path, which no refspec maps to.
    for decoy in ["refs/remotes/mirror/doomed", "refs/remotes/narrow/doomed"] {
        fixture
            .local
            .reference(decoy, head.id(), true, "decoy")
            .unwrap();
    }

    delete_remote_branch(
        &fixture.local,
        "mirror",
        "doomed",
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();

    assert_eq!(fixture.bare_tip("doomed"), None);
    assert!(fixture
        .local
        .find_reference("refs/remotes/mirror-ns/doomed")
        .is_err());
    assert!(fixture
        .local
        .find_reference("refs/remotes/mirror/doomed")
        .is_ok());

    // A remote whose fetch refspec does not cover the branch: no local tracking ref is touched.
    push_branch(
        &fixture.local,
        "origin",
        "doomed",
        "doomed",
        false,
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    delete_remote_branch(
        &fixture.local,
        "narrow",
        "doomed",
        &mut credentials,
        &mut reporter,
        &|| false,
    )
    .unwrap();
    assert_eq!(fixture.bare_tip("doomed"), None);
    assert!(fixture
        .local
        .find_reference("refs/remotes/narrow/doomed")
        .is_ok());
}
