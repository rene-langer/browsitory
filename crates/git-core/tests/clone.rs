#[allow(dead_code)]
mod common;

use std::path::{Path, PathBuf};

use git_core::clone::{clone_repo, CloneError, CloneOptions};
use git_core::remote::{
    CredentialProvider, TransferOperation, TransferPhase, TransferProgress, TransferReporter,
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

/// A bare repo with two commits on `main` and one on `feature`, addressed by `file://` URL.
struct Source {
    _dir: tempfile::TempDir,
    url: String,
}

fn source() -> Source {
    let dir = tempfile::TempDir::new().unwrap();
    let work_path = dir.path().join("work");
    let work = git2::Repository::init_opts(
        &work_path,
        git2::RepositoryInitOptions::new().initial_head("main"),
    )
    .unwrap();
    {
        let mut config = work.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    common::write_file(&work_path, "a.txt", "one\n");
    common::commit_all(&work, "first");
    common::write_file(&work_path, "a.txt", "two\n");
    common::commit_all(&work, "second");
    let head = work.head().unwrap().peel_to_commit().unwrap();
    work.branch("feature", &head, false).unwrap();
    common::write_file(&work_path, "a.txt", "main only\n");
    common::commit_all(&work, "third on main");

    let bare_path = dir.path().join("source.git");
    git2::Repository::init_bare(&bare_path).unwrap();
    let mut remote = work.remote("origin", bare_path.to_str().unwrap()).unwrap();
    remote
        .push(
            &[
                "refs/heads/main:refs/heads/main",
                "refs/heads/feature:refs/heads/feature",
            ],
            None,
        )
        .unwrap();
    let bare = git2::Repository::open_bare(&bare_path).unwrap();
    bare.set_head("refs/heads/main").unwrap();

    let url = url_for(&bare_path);
    Source { _dir: dir, url }
}

fn url_for(path: &Path) -> String {
    format!("file://{}", path.to_str().unwrap().replace('\\', "/"))
}

fn opts(url: &str, dest: PathBuf) -> CloneOptions {
    CloneOptions {
        url: url.to_string(),
        dest,
        branch: None,
        recurse_submodules: false,
    }
}

fn run(
    opts: &CloneOptions,
) -> (
    Result<git_core::clone::CloneOutcome, CloneError>,
    VecReporter,
) {
    let mut reporter = VecReporter::default();
    let result = clone_repo(opts, &mut NoCredentials, &mut reporter, &|| false);
    (result, reporter)
}

#[test]
fn clones_the_default_branch_with_origin_and_tracking() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");

    let (result, _) = run(&opts(&source.url, dest.clone()));

    let outcome = result.unwrap();
    assert_eq!(outcome.path, dest);
    assert!(outcome.submodule_failures.is_empty());
    let repo = git2::Repository::open(&dest).unwrap();
    assert_eq!(repo.head().unwrap().shorthand(), Ok("main"));
    assert_eq!(
        std::fs::read_to_string(dest.join("a.txt")).unwrap(),
        "main only\n"
    );
    assert_eq!(
        repo.find_remote("origin").unwrap().url(),
        Ok(source.url.as_str())
    );
    let upstream = repo
        .find_branch("main", git2::BranchType::Local)
        .unwrap()
        .upstream()
        .unwrap();
    assert_eq!(upstream.name().unwrap(), Some("origin/main"));
}

#[test]
fn checks_out_the_requested_branch() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut options = opts(&source.url, dest.clone());
    options.branch = Some("feature".into());

    let (result, _) = run(&options);

    result.unwrap();
    let repo = git2::Repository::open(&dest).unwrap();
    assert_eq!(repo.head().unwrap().shorthand(), Ok("feature"));
    assert_eq!(
        std::fs::read_to_string(dest.join("a.txt")).unwrap(),
        "two\n"
    );
}

#[test]
fn unknown_branch_is_reported_and_the_created_destination_removed() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut options = opts(&source.url, dest.clone());
    options.branch = Some("nope".into());

    let (result, _) = run(&options);

    assert!(
        matches!(result, Err(CloneError::BranchNotFound)),
        "{result:?}"
    );
    assert!(!dest.exists());
}

#[test]
fn trims_the_url_and_ignores_a_blank_branch() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut options = opts(&format!("  {}\n", source.url), dest.clone());
    options.branch = Some("   ".into());

    let (result, _) = run(&options);

    result.unwrap();
    let repo = git2::Repository::open(&dest).unwrap();
    assert_eq!(repo.head().unwrap().shorthand(), Ok("main"));
}

#[test]
fn refuses_a_non_empty_destination_and_leaves_it_untouched() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    std::fs::write(target.path().join("keep.txt"), "mine").unwrap();

    let (result, _) = run(&opts(&source.url, target.path().to_path_buf()));

    assert!(
        matches!(result, Err(CloneError::DestNotEmpty)),
        "{result:?}"
    );
    assert_eq!(
        std::fs::read_to_string(target.path().join("keep.txt")).unwrap(),
        "mine"
    );
    assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 1);
}

#[test]
fn refuses_a_destination_that_is_a_file() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let file = target.path().join("clone");
    std::fs::write(&file, "mine").unwrap();

    let (result, _) = run(&opts(&source.url, file.clone()));

    assert!(
        matches!(result, Err(CloneError::DestNotEmpty)),
        "{result:?}"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
}

#[test]
fn clones_into_an_existing_empty_directory() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();

    let (result, _) = run(&opts(&source.url, target.path().to_path_buf()));

    result.unwrap();
    assert!(target.path().join("a.txt").exists());
}

#[test]
fn failed_clone_into_existing_empty_dir_keeps_the_dir_empty() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let mut options = opts(&source.url, target.path().to_path_buf());
    options.branch = Some("nope".into());

    let (result, _) = run(&options);

    assert!(result.is_err());
    assert!(target.path().is_dir());
    assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 0);
}

#[test]
fn cancel_before_start_creates_nothing() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut reporter = VecReporter::default();

    let result = clone_repo(
        &opts(&source.url, dest.clone()),
        &mut NoCredentials,
        &mut reporter,
        &|| true,
    );

    assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    assert!(!dest.exists());
}

#[test]
fn reports_receiving_then_updating_progress_as_clone() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();

    let (result, reporter) = run(&opts(&source.url, target.path().join("clone")));

    result.unwrap();
    assert!(reporter
        .events
        .iter()
        .all(|e| e.operation == TransferOperation::Clone));
    let first_updating = reporter
        .events
        .iter()
        .position(|e| e.phase == TransferPhase::Updating);
    let last_receiving = reporter
        .events
        .iter()
        .rposition(|e| e.phase == TransferPhase::Receiving);
    assert!(last_receiving.is_some(), "no Receiving events");
    assert!(first_updating.is_some(), "no Updating events");
    assert!(last_receiving < first_updating);
}

#[test]
fn rejects_remote_helper_and_empty_urls() {
    let target = tempfile::TempDir::new().unwrap();
    for url in ["ext::sh -c touch% /tmp/pwned", "", "   "] {
        let dest = target.path().join("clone");
        let (result, _) = run(&opts(url, dest.clone()));
        assert!(
            matches!(result, Err(CloneError::InvalidUrl)),
            "{url}: {result:?}"
        );
        assert!(!dest.exists(), "{url}: destination must not be created");
    }
}

#[test]
fn rejects_a_url_with_an_embedded_password() {
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");

    let (result, _) = run(&opts(
        "https://alice:secret@example.test/repo.git",
        dest.clone(),
    ));

    assert!(
        matches!(result, Err(CloneError::CredentialBearingUrl)),
        "{result:?}"
    );
    assert!(!dest.exists());
}

#[test]
fn accepts_scp_style_and_ipv6_ssh_urls_at_validation() {
    for url in [
        "git@example.test:org/repo.git",
        "ssh://git@[::1]/repo.git",
        "/abs/local/repo",
    ] {
        assert!(git_core::clone::validate_clone_url(url).is_ok(), "{url}");
    }
}

/// Cancel closure that allows the pre-start check, then cancels on every later poll.
fn cancel_after_start() -> impl Fn() -> bool {
    let calls = std::cell::Cell::new(0u32);
    move || {
        calls.set(calls.get() + 1);
        calls.get() > 1
    }
}

#[test]
fn cancel_mid_clone_aborts_and_removes_the_created_destination() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut reporter = VecReporter::default();

    let result = clone_repo(
        &opts(&source.url, dest.clone()),
        &mut NoCredentials,
        &mut reporter,
        &cancel_after_start(),
    );

    assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    assert!(!dest.exists());
}

#[test]
fn cancel_mid_clone_into_existing_empty_dir_keeps_the_dir_empty() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let mut reporter = VecReporter::default();

    let result = clone_repo(
        &opts(&source.url, target.path().to_path_buf()),
        &mut NoCredentials,
        &mut reporter,
        &cancel_after_start(),
    );

    assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    assert!(target.path().is_dir());
    assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 0);
}

/// Reporter that raises `flag` on an `Updating` event (the last one when `last_only`), i.e. once
/// the transfer is complete and only checkout / submodule work remains.
struct FlagOnUpdating<'a> {
    flag: &'a std::cell::Cell<bool>,
    last_only: bool,
}

impl TransferReporter for FlagOnUpdating<'_> {
    fn report(&mut self, event: TransferProgress) {
        if event.phase == TransferPhase::Updating
            && (!self.last_only || event.current >= event.total)
        {
            self.flag.set(true);
        }
    }
}

#[test]
fn cancel_raised_during_checkout_cancels_and_removes_the_destination() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let flag = std::cell::Cell::new(false);
    let mut reporter = FlagOnUpdating {
        flag: &flag,
        last_only: false,
    };

    let result = clone_repo(
        &opts(&source.url, dest.clone()),
        &mut NoCredentials,
        &mut reporter,
        &|| flag.get(),
    );

    assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    assert!(!dest.exists());
}

#[test]
fn cancel_raised_once_checkout_finished_still_cancels() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let flag = std::cell::Cell::new(false);
    let mut reporter = FlagOnUpdating {
        flag: &flag,
        last_only: true,
    };

    let result = clone_repo(
        &opts(&source.url, dest.clone()),
        &mut NoCredentials,
        &mut reporter,
        &|| flag.get(),
    );

    assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    assert!(!dest.exists());
}

#[test]
fn cancel_raised_before_the_submodule_phase_cancels() {
    let (_dir, url) = source_with_submodule(|p| p.to_str().unwrap().to_string());
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let flag = std::cell::Cell::new(false);
    let mut reporter = FlagOnUpdating {
        flag: &flag,
        last_only: true,
    };
    let mut options = opts(&url, dest.clone());
    options.recurse_submodules = true;

    let result = clone_repo(&options, &mut NoCredentials, &mut reporter, &|| flag.get());

    assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    assert!(!dest.exists());
}

#[test]
fn a_relative_destination_is_refused_and_nothing_is_created() {
    let source = source();
    let name = format!("relative-clone-dest-{}", std::process::id());
    let dest = PathBuf::from(&name);

    let (result, _) = run(&opts(&source.url, dest.clone()));

    match result {
        Err(CloneError::Io(error)) => {
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
            assert_eq!(error.to_string(), "Destination must be an absolute path.");
        }
        other => panic!("{other:?}"),
    }
    assert!(!dest.exists());
}

#[test]
fn a_missing_local_source_with_a_branch_is_not_branch_not_found() {
    let target = tempfile::TempDir::new().unwrap();
    let missing = target.path().join("no-such-repo");
    let mut options = opts(missing.to_str().unwrap(), target.path().join("clone"));
    options.branch = Some("main".into());

    let (result, _) = run(&options);

    assert!(result.is_err());
    assert!(
        !matches!(result, Err(CloneError::BranchNotFound)),
        "{result:?}"
    );
}

/// Bare parent repo whose `deps/child` submodule points at `child_url`.
fn source_with_submodule(child_url_for: impl Fn(&Path) -> String) -> (tempfile::TempDir, String) {
    let dir = tempfile::TempDir::new().unwrap();
    let child_path = dir.path().join("child");
    let child = git2::Repository::init(&child_path).unwrap();
    {
        let mut config = child.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    common::write_file(&child_path, "child.txt", "child\n");
    common::commit_all(&child, "child commit");

    let parent_path = dir.path().join("parent");
    let parent = git2::Repository::init(&parent_path).unwrap();
    {
        let mut config = parent.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    let mut submodule = parent
        .submodule(child_path.to_str().unwrap(), Path::new("deps/child"), true)
        .unwrap();
    submodule.clone(None).unwrap();
    submodule.add_to_index(true).unwrap();
    submodule.add_finalize().unwrap();
    drop(submodule);
    // Rewrite the recorded URL to the one under test (plain path or file:// URL).
    let gitmodules = parent_path.join(".gitmodules");
    let text = std::fs::read_to_string(&gitmodules).unwrap();
    let text = text.replace(child_path.to_str().unwrap(), &child_url_for(&child_path));
    std::fs::write(&gitmodules, text).unwrap();
    common::commit_all(&parent, "add child submodule");

    let bare_path = dir.path().join("parent.git");
    git2::Repository::init_bare(&bare_path).unwrap();
    let head = parent.head().unwrap().name().unwrap().to_string();
    parent
        .remote("origin", bare_path.to_str().unwrap())
        .unwrap()
        .push(&[format!("{head}:{head}")], None)
        .unwrap();
    git2::Repository::open_bare(&bare_path)
        .unwrap()
        .set_head(&head)
        .unwrap();
    let url = url_for(&bare_path);
    (dir, url)
}

#[test]
fn initializes_and_checks_out_submodules_when_requested() {
    let (_dir, url) = source_with_submodule(|p| p.to_str().unwrap().to_string());
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut options = opts(&url, dest.clone());
    options.recurse_submodules = true;

    let (result, _) = run(&options);

    let outcome = result.unwrap();
    assert!(
        outcome.submodule_failures.is_empty(),
        "{:?}",
        outcome.submodule_failures
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("deps/child/child.txt")).unwrap(),
        "child\n"
    );
}

#[test]
fn leaves_submodules_alone_when_not_requested() {
    let (_dir, url) = source_with_submodule(|p| p.to_str().unwrap().to_string());
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");

    let (result, _) = run(&opts(&url, dest.clone()));

    result.unwrap();
    assert!(!dest.join("deps/child/child.txt").exists());
}

#[test]
fn a_failing_submodule_is_reported_and_the_clone_still_succeeds() {
    let (_dir, url) = source_with_submodule(url_for); // file:// is refused for submodules
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut options = opts(&url, dest.clone());
    options.recurse_submodules = true;

    let (result, _) = run(&options);

    let outcome = result.unwrap();
    assert_eq!(outcome.submodule_failures, vec!["deps/child".to_string()]);
    assert!(git2::Repository::open(&dest).is_ok());
}
