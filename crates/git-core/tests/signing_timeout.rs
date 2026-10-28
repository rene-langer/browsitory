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
