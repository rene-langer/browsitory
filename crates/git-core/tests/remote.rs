mod common;

use git_core::remote::{
    add_remote, clear_current_upstream, current_upstream, fetch_remote, list_remotes,
    pull_after_fetch, remote_upstreams, remove_remote, remove_remote_and_clear_upstreams,
    rename_remote, set_current_upstream, update_remote_urls, CredentialProvider, PullOutcome,
    RemoteError, TransferOperation, TransferPhase, TransferProgress, TransferReporter,
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

struct RemoteFixture {
    source_dir: tempfile::TempDir,
    _remote_dir: tempfile::TempDir,
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
        _remote_dir: remote_dir,
        local_dir,
        source,
        local,
    }
}

fn diverged_local_and_bare_remote() -> RemoteFixture {
    local_and_bare_remote()
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
    fixture.write_local("dirty.txt", "dirty");

    assert!(matches!(
        pull_after_fetch(&fixture.local, "origin", "main"),
        Err(RemoteError::DirtyWorktree)
    ));
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
