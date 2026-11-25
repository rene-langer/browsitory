//! Signing and verifying never run a program that a repository's own config names, and never
//! trust its allowed signers file (#1121): `gpg.program`, `gpg.openpgp.program`,
//! `gpg.ssh.program` and `gpg.ssh.allowedSignersFile` count only from the user's own config
//! levels (system, XDG, global). A repository folder that arrives with its `.git/config` (an
//! archive, a shared drive) could otherwise run any program when a commit is selected.
#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::{
    commit_all, enable_ssh_signing, init_repo, program_available, ssh_key_fixture, user_config,
    write_file,
};
use git_core::signing::{
    create_commit, signing_config, verify_commit, HeadUpdate, SignatureFormat, SignatureStatus,
};

const PGP_ARMOR: &str = "-----BEGIN PGP SIGNATURE-----\nZmFrZQ==\n-----END PGP SIGNATURE-----\n";
const SSH_ARMOR: &str = "-----BEGIN SSH SIGNATURE-----\nZmFrZQ==\n-----END SSH SIGNATURE-----\n";

/// An executable script `name` in `dir` that first creates `<name>.ran`, then runs `body`.
/// Returns the script's path and the marker's.
fn script(dir: &Path, name: &str, body: &str) -> (String, PathBuf) {
    let path = dir.join(name);
    let marker = dir.join(format!("{name}.ran"));
    std::fs::write(
        &path,
        format!("#!/bin/sh\ntouch '{}'\n{body}\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (path.to_str().unwrap().to_string(), marker)
}

/// A commit on top of HEAD whose `gpgsig` header holds `armor`: any text with the right first
/// line is enough to make a viewer start the verifier.
fn fake_signed_commit(dir: &Path, repo: &git2::Repository, armor: &str) -> String {
    write_file(dir, "a.txt", "a\n");
    commit_all(repo, "base");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    let buffer = repo
        .commit_create_buffer(&sig, &sig, "signed?", &head.tree().unwrap(), &[&head])
        .unwrap();
    repo.commit_signed(std::str::from_utf8(&buffer).unwrap(), armor, None)
        .unwrap()
        .to_string()
}

fn ignored_program(setting: &str, program: &str) -> String {
    format!(
        "{setting} is set to \"{program}\" only in this repository's git config. Browsitory \
         ignores signing programs from a repository's own config, so a repository can't make it \
         run commands. Set it in your global git config instead, for example: git config \
         --global {setting} {program}"
    )
}

fn try_commit(repo: &git2::Repository) -> Result<git2::Oid, git_core::signing::SignError> {
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    create_commit(
        repo,
        Some(HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit",
        }),
        &sig,
        &sig,
        "signed?",
        &head.tree().unwrap(),
        &[&head],
    )
}

#[test]
fn verifying_never_runs_a_gpg_program_set_only_in_the_repository() {
    for setting in ["gpg.program", "gpg.openpgp.program"] {
        let (dir, repo) = init_repo();
        let bin = tempfile::TempDir::new().unwrap();
        let (program, marker) = script(bin.path(), "verify.sh", "exit 1");
        let id = fake_signed_commit(dir.path(), &repo, PGP_ARMOR);
        repo.config().unwrap().set_str(setting, &program).unwrap();

        let result = verify_commit(&repo, &id).unwrap();

        assert!(!marker.exists(), "{setting}: the repository's program ran");
        assert_eq!(result.status, SignatureStatus::Error, "{setting}");
        assert_eq!(result.format, Some(SignatureFormat::OpenPgp), "{setting}");
        assert_eq!(
            result.detail.as_deref(),
            Some(ignored_program(setting, &program).as_str())
        );
    }
}

#[test]
fn verifying_never_runs_an_ssh_program_set_only_in_the_repository() {
    let (dir, repo) = init_repo();
    let bin = tempfile::TempDir::new().unwrap();
    let (program, marker) = script(bin.path(), "verify.sh", "exit 1");
    let id = fake_signed_commit(dir.path(), &repo, SSH_ARMOR);
    repo.config()
        .unwrap()
        .set_str("gpg.ssh.program", &program)
        .unwrap();

    let result = verify_commit(&repo, &id).unwrap();

    assert!(!marker.exists(), "the repository's program ran");
    assert_eq!(result.status, SignatureStatus::Error);
    assert_eq!(result.format, Some(SignatureFormat::Ssh));
    assert_eq!(
        result.detail.as_deref(),
        Some(ignored_program("gpg.ssh.program", &program).as_str())
    );
}

#[test]
fn the_users_own_gpg_program_verifies_and_the_repository_cannot_override_it() {
    let (dir, repo) = init_repo();
    let bin = tempfile::TempDir::new().unwrap();
    let (good, good_ran) = script(
        bin.path(),
        "user-gpg",
        "cat >/dev/null\n\
         echo '[GNUPG:] GOODSIG AAAA Fake Signer <fake@example.com>'\n\
         echo '[GNUPG:] VALIDSIG FPR0123 2026-11-25'\n\
         echo '[GNUPG:] TRUST_ULTIMATE 0 pgp'",
    );
    let (evil, evil_ran) = script(bin.path(), "repo-gpg", "exit 1");
    let id = fake_signed_commit(dir.path(), &repo, PGP_ARMOR);
    user_config(&repo).set_str("gpg.program", &good).unwrap();
    let mut local = repo.config().unwrap();
    local.set_str("gpg.program", &evil).unwrap();
    local.set_str("gpg.openpgp.program", &evil).unwrap();

    let result = verify_commit(&repo, &id).unwrap();

    assert!(!evil_ran.exists(), "the repository's program ran");
    assert!(good_ran.exists(), "the user's program never ran");
    assert_eq!(result.status, SignatureStatus::Good);
    assert_eq!(
        result.signer.as_deref(),
        Some("Fake Signer <fake@example.com>")
    );
}

#[test]
fn signing_refuses_a_gpg_program_set_only_in_the_repository() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let head = repo.head().unwrap().target().unwrap();
    let bin = tempfile::TempDir::new().unwrap();
    let (program, marker) = script(bin.path(), "sign.sh", "exit 1");
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.program", &program).unwrap();

    let error = try_commit(&repo).unwrap_err();

    assert!(!marker.exists(), "the repository's program ran");
    assert_eq!(error.to_string(), ignored_program("gpg.program", &program));
    assert_eq!(
        signing_config(&repo).unwrap_err().to_string(),
        ignored_program("gpg.program", &program)
    );
    assert_eq!(repo.head().unwrap().target(), Some(head));
}

#[test]
fn signing_refuses_an_ssh_program_set_only_in_the_repository() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let bin = tempfile::TempDir::new().unwrap();
    let (program, marker) = script(bin.path(), "sign.sh", "exit 1");
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "ssh").unwrap();
    config
        .set_str("user.signingKey", "/nonexistent/key")
        .unwrap();
    config.set_str("gpg.ssh.program", &program).unwrap();

    let error = try_commit(&repo).unwrap_err();

    assert!(!marker.exists(), "the repository's program ran");
    assert_eq!(
        error.to_string(),
        ignored_program("gpg.ssh.program", &program)
    );
}

#[test]
fn signing_uses_the_users_own_program_over_the_repositorys() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let bin = tempfile::TempDir::new().unwrap();
    let (good, good_ran) = script(
        bin.path(),
        "user-gpg",
        "cat >/dev/null\n\
         printf -- '-----BEGIN PGP SIGNATURE-----\\nZmFrZQ==\\n-----END PGP SIGNATURE-----\\n'\n\
         echo '[GNUPG:] SIG_CREATED D 1 8 00 0 0' >&2",
    );
    let (evil, evil_ran) = script(bin.path(), "repo-gpg", "exit 1");
    user_config(&repo).set_str("gpg.program", &good).unwrap();
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.program", &evil).unwrap();

    let oid = try_commit(&repo).unwrap();

    assert!(!evil_ran.exists(), "the repository's program ran");
    assert!(good_ran.exists(), "the user's program never ran");
    assert!(repo.extract_signature(&oid, None).is_ok());
}

#[test]
fn an_allowed_signers_file_set_only_in_the_repository_is_ignored() {
    if !program_available("ssh-keygen") {
        eprintln!("skipping: ssh-keygen not on PATH");
        return;
    }
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    enable_ssh_signing(&repo, &key);
    let oid = try_commit(&repo).unwrap().to_string();
    let allowed = keys.path().join("allowed_signers");
    repo.config()
        .unwrap()
        .set_str("gpg.ssh.allowedSignersFile", allowed.to_str().unwrap())
        .unwrap();

    let result = verify_commit(&repo, &oid).unwrap();

    assert_eq!(result.status, SignatureStatus::GoodUntrusted);
    assert_eq!(result.signer, None);
    assert_eq!(
        result.detail.as_deref(),
        Some(
            "gpg.ssh.allowedSignersFile is set only in this repository's git config. Browsitory \
             ignores it there, so a repository can't vouch for its own signers. Set it in your \
             global git config instead."
        )
    );

    user_config(&repo)
        .set_str("gpg.ssh.allowedSignersFile", allowed.to_str().unwrap())
        .unwrap();
    let result = verify_commit(&repo, &oid).unwrap();
    assert_eq!(result.status, SignatureStatus::Good);
    assert_eq!(result.signer.as_deref(), Some("test@example.com"));
}
