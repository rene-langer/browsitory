//! Signing and verification run with a timeout and a cancel check (#110), against fake signer
//! scripts.
#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

use git_core::signing::{
    create_commit_with, verify_commit_with, HeadUpdate, SignError, SignOptions, SignatureStatus,
};

/// An executable `#!/bin/sh` script `name` in `dir`.
fn script(dir: &std::path::Path, name: &str, body: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.to_str().unwrap().to_string()
}

/// A repo with one commit and `commit.gpgSign=true`, `gpg.program=<program>`.
fn signing_repo(program: &str) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "base");
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.program", program).unwrap();
    (dir, repo)
}

fn try_commit(repo: &git2::Repository, opts: &SignOptions) -> Result<git2::Oid, SignError> {
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = head.tree().unwrap();
    let sig = repo.signature().unwrap();
    create_commit_with(
        repo,
        Some(HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "signed?",
        &tree,
        &[&head],
        opts,
    )
}

const FAKE_SIGNATURE: &str = "cat >/dev/null
echo '-----BEGIN PGP SIGNATURE-----'
echo 'ZmFrZQ=='
echo '-----END PGP SIGNATURE-----'
echo '[GNUPG:] SIG_CREATED D 1 8 00 0 0' >&2";

#[test]
fn a_hung_signer_times_out() {
    let bin = tempfile::TempDir::new().unwrap();
    let (_dir, repo) = signing_repo(&script(bin.path(), "gpg", "sleep 30"));
    let head = repo.head().unwrap().target().unwrap();
    let started = Instant::now();

    let result = try_commit(
        &repo,
        &SignOptions {
            timeout: Duration::from_millis(500),
            ..SignOptions::default()
        },
    );

    assert!(
        matches!(result, Err(SignError::TimedOut { seconds: 0 })),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(repo.head().unwrap().target().unwrap(), head);
}

#[test]
fn the_timeout_message_names_the_seconds_and_the_passphrase() {
    assert_eq!(
        SignError::TimedOut { seconds: 120 }.to_string(),
        "The signing program did not finish within 120 s. If it is waiting for a passphrase, \
         unlock your key and try again."
    );
    assert_eq!(SignError::Cancelled.to_string(), "Signing was cancelled.");
}

#[test]
fn a_cancelled_signer_stops() {
    let bin = tempfile::TempDir::new().unwrap();
    let (_dir, repo) = signing_repo(&script(bin.path(), "gpg", "sleep 30"));
    let head = repo.head().unwrap().target().unwrap();
    let started = Instant::now();
    let cancel = || started.elapsed() > Duration::from_millis(200);

    let result = try_commit(
        &repo,
        &SignOptions {
            cancel: &cancel,
            ..SignOptions::default()
        },
    );

    assert!(matches!(result, Err(SignError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(repo.head().unwrap().target().unwrap(), head);
}

#[test]
fn a_signer_printing_too_much_fails_instead_of_truncating() {
    let bin = tempfile::TempDir::new().unwrap();
    let (_dir, repo) = signing_repo(&script(
        bin.path(),
        "gpg",
        "cat >/dev/null; head -c 2000000 /dev/zero; echo '[GNUPG:] SIG_CREATED ' >&2",
    ));

    let result = try_commit(&repo, &SignOptions::default());

    let error = result.unwrap_err();
    assert!(matches!(error, SignError::SignerFailed(_)), "{error:?}");
    assert_eq!(
        error.to_string(),
        "Signing failed: the signature was larger than 1048576 bytes."
    );
}

#[test]
fn a_missing_signer_is_still_signer_not_found() {
    let (_dir, repo) = signing_repo("browsitory-no-such-gpg");
    let result = try_commit(&repo, &SignOptions::default());
    assert!(
        matches!(result, Err(SignError::SignerNotFound(ref p)) if p == "browsitory-no-such-gpg"),
        "{result:?}"
    );
}

#[test]
fn a_working_fake_signer_signs() {
    let bin = tempfile::TempDir::new().unwrap();
    let (_dir, repo) = signing_repo(&script(bin.path(), "gpg", FAKE_SIGNATURE));
    let oid = try_commit(&repo, &SignOptions::default()).unwrap();
    let (signature, _) = repo.extract_signature(&oid, None).unwrap();
    assert!(signature
        .as_str()
        .unwrap()
        .starts_with("-----BEGIN PGP SIGNATURE-----"));
}

#[test]
fn a_hung_verifier_reports_an_error_status() {
    let bin = tempfile::TempDir::new().unwrap();
    let (_dir, repo) = signing_repo(&script(bin.path(), "gpg", FAKE_SIGNATURE));
    let oid = try_commit(&repo, &SignOptions::default()).unwrap();
    repo.config()
        .unwrap()
        .set_str("gpg.program", &script(bin.path(), "gpg-hung", "sleep 30"))
        .unwrap();
    let started = Instant::now();

    let result = verify_commit_with(&repo, &oid.to_string(), Duration::from_millis(500)).unwrap();

    assert_eq!(result.status, SignatureStatus::Error);
    assert!(
        result
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("timed out"),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_signer_whose_output_never_finishes_fails_instead_of_truncating() {
    // A background grandchild keeps stdout open past the drain grace: what was read so far
    // may be incomplete, so it is not used as a signature.
    let bin = tempfile::TempDir::new().unwrap();
    let (_dir, repo) = signing_repo(&script(
        bin.path(),
        "gpg",
        &format!("{FAKE_SIGNATURE}\nsleep 4 &"),
    ));

    let result = try_commit(&repo, &SignOptions::default());

    let error = result.unwrap_err();
    assert_eq!(
        error.to_string(),
        "Signing failed: the signing program's output did not finish."
    );
}

/// Records the hooks a command reports, as `started:<hook>` / `finished:<hook>`.
#[derive(Default)]
struct Recorder(Vec<String>);

impl git_core::hooks::HookReporter for Recorder {
    fn started(&mut self, hook: &str) {
        self.0.push(format!("started:{hook}"));
    }
    fn finished(
        &mut self,
        hook: &str,
        _result: &Result<git_core::hooks::HookOutcome, git_core::hooks::HookError>,
    ) {
        self.0.push(format!("finished:{hook}"));
    }
}

/// `signing_repo`'s base, then `a.txt` changed on the checked-out branch. Returns the base and
/// the branch's tip.
fn rebase_repo(program: &str) -> (tempfile::TempDir, git2::Repository, git2::Oid, git2::Oid) {
    let (dir, repo) = signing_repo(program);
    // The base and the step's commit are made unsigned (`commit_all`), the rebase signs.
    let base = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "a.txt", "changed\n");
    common::commit_all(&repo, "change a");
    let tip = repo.head().unwrap().target().unwrap();
    (dir, repo, base, tip)
}

fn reword(commit: git2::Oid, message: &str) -> git_core::rebase::RebasePlanEntry {
    git_core::rebase::RebasePlanEntry {
        commit_id: commit.to_string(),
        action: git_core::rebase::RebaseAction::Reword {
            message: message.to_string(),
        },
        combined_message: None,
    }
}

#[test]
fn cancelling_the_signer_of_a_rebase_step_rolls_the_rebase_back() {
    // #151: a rebase step's signer is cancellable like a commit's, and reported as the
    // `signing` pseudo-hook so the hook strip offers Cancel.
    let bin = tempfile::TempDir::new().unwrap();
    let (_dir, repo, base, tip) = rebase_repo(&script(bin.path(), "gpg", "sleep 30"));
    let branch = repo.head().unwrap().name().unwrap().to_string();
    let started = Instant::now();
    let cancel = || started.elapsed() > Duration::from_millis(200);
    let mut recorder = Recorder::default();

    let result = git_core::rebase::start_rebase_with(
        &repo,
        &base.to_string(),
        vec![reword(tip, "reworded\n")],
        git_core::hooks::HookOptions {
            cancel: Some(&cancel),
            reporter: Some(&mut recorder),
            ..Default::default()
        },
    );

    assert!(
        matches!(
            result,
            Err(git_core::rebase::RebaseError::Sign(SignError::Cancelled))
        ),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(recorder.0, vec!["started:signing", "finished:signing"]);
    let head = repo.head().unwrap();
    assert_eq!(head.name().ok(), Some(branch.as_str()));
    assert_eq!(head.target(), Some(tip));
}

#[test]
fn cancelling_the_signer_of_a_continued_rebase_step_keeps_the_step_paused() {
    let bin = tempfile::TempDir::new().unwrap();
    let (_dir, repo, base, tip) = rebase_repo(&script(bin.path(), "gpg", FAKE_SIGNATURE));
    let edit = git_core::rebase::RebasePlanEntry {
        commit_id: tip.to_string(),
        action: git_core::rebase::RebaseAction::Edit,
        combined_message: None,
    };
    let (mut state, step) =
        git_core::rebase::start_rebase(&repo, &base.to_string(), vec![edit]).unwrap();
    assert_eq!(step, git_core::rebase::RebaseStepResult::PausedForEdit);
    repo.config()
        .unwrap()
        .set_str("gpg.program", &script(bin.path(), "gpg-hung", "sleep 30"))
        .unwrap();
    let started = Instant::now();
    let cancel = || started.elapsed() > Duration::from_millis(200);
    let mut recorder = Recorder::default();

    let result = git_core::rebase::rebase_continue_with(
        &repo,
        &mut state,
        git_core::hooks::HookOptions {
            cancel: Some(&cancel),
            reporter: Some(&mut recorder),
            ..Default::default()
        },
    );

    assert!(
        matches!(
            result,
            Err(git_core::rebase::RebaseError::Sign(SignError::Cancelled))
        ),
        "{result:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    assert_eq!(recorder.0, vec!["started:signing", "finished:signing"]);
    assert_eq!(state.stop, git_core::rebase::RebaseStop::Edit);
    assert_eq!(repo.head().unwrap().target(), Some(state.expected_head));
    // With a signer that answers, Continue lands the step.
    repo.config()
        .unwrap()
        .set_str("gpg.program", &script(bin.path(), "gpg-ok", FAKE_SIGNATURE))
        .unwrap();
    assert_eq!(
        git_core::rebase::rebase_continue(&repo, &mut state).unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );
}
