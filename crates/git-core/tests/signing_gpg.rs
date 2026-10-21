//! gpg tests live in their own test binary: the fixture sets GNUPGHOME process-wide (once,
//! before any test here spawns gpg) so concurrent tests in other binaries are unaffected.

mod common;

use std::sync::OnceLock;

use common::{commit_all, init_repo, program_available, write_file};
use git_core::signing::{verify_commit, SignatureFormat, SignatureStatus};

const UID: &str = "Test User <test@example.com>";

/// Some(path) once a throwaway keyring with an ultimately trusted, passphrase-less key for `UID`
/// is the process's GNUPGHOME; None when gpg is unavailable.
fn gpg_home() -> Option<&'static std::path::Path> {
    static HOME: OnceLock<Option<tempfile::TempDir>> = OnceLock::new();
    HOME.get_or_init(|| {
        // Git-for-Windows' gpg is an MSYS binary: it reads `C:\...` as a relative path and
        // cannot start gpg-agent in CI, so these tests run on Unix only.
        if cfg!(windows) || !program_available("gpg") {
            return None;
        }
        let home = tempfile::Builder::new().prefix("bgpg").tempdir().unwrap();
        std::env::set_var("GNUPGHOME", home.path());
        let status = std::process::Command::new("gpg")
            .args([
                "--batch",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                "",
                "--quick-gen-key",
                UID,
                "ed25519",
                "sign",
                "never",
            ])
            .output()
            .unwrap();
        assert!(
            status.status.success(),
            "gpg --quick-gen-key failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
        Some(home)
    })
    .as_ref()
    .map(|dir| dir.path())
}

macro_rules! require_gpg {
    () => {
        if gpg_home().is_none() {
            eprintln!("skipping: gpg not on PATH");
            return;
        }
    };
}

fn signing_repo() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    repo.config()
        .unwrap()
        .set_bool("commit.gpgSign", true)
        .unwrap();
    (dir, repo)
}

fn commit_file(dir: &std::path::Path, repo: &mut git2::Repository, name: &str) -> String {
    write_file(dir, name, "x\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(name)).unwrap();
    index.write().unwrap();
    git_core::commit::commit(repo, &format!("add {name}")).unwrap()
}

#[test]
fn commit_is_gpg_signed_with_the_committer_identity_by_default() {
    require_gpg!();
    let (dir, mut repo) = signing_repo();
    let id = commit_file(dir.path(), &mut repo, "a.txt");
    let (signature, _) = repo
        .extract_signature(&git2::Oid::from_str(&id).unwrap(), None)
        .unwrap();
    assert!(std::str::from_utf8(&signature)
        .unwrap()
        .starts_with("-----BEGIN PGP SIGNATURE-----"));
}

#[test]
fn own_gpg_signature_verifies_as_good() {
    require_gpg!();
    let (dir, mut repo) = signing_repo();
    let id = commit_file(dir.path(), &mut repo, "a.txt");

    let result = verify_commit(&repo, &id).unwrap();

    assert_eq!(result.status, SignatureStatus::Good);
    assert_eq!(result.format, Some(SignatureFormat::OpenPgp));
    assert_eq!(result.signer.as_deref(), Some(UID));
    assert_eq!(result.key.as_deref().map(str::len), Some(40));
}

#[test]
fn tampered_gpg_signed_commit_is_bad() {
    require_gpg!();
    let (dir, mut repo) = signing_repo();
    let id = commit_file(dir.path(), &mut repo, "a.txt");
    let (signature, data) = repo
        .extract_signature(&git2::Oid::from_str(&id).unwrap(), None)
        .unwrap();
    let tampered = std::str::from_utf8(&data)
        .unwrap()
        .replace("add a.txt", "add b.txt");
    let forged = repo
        .commit_signed(&tampered, std::str::from_utf8(&signature).unwrap(), None)
        .unwrap();

    assert_eq!(
        verify_commit(&repo, &forged.to_string()).unwrap().status,
        SignatureStatus::Bad
    );
}

#[test]
fn signature_from_a_key_not_in_the_keyring_is_unknown_key() {
    require_gpg!();
    let (dir, repo) = signing_repo();
    // A second, separate keyring signs the buffer; the process keyring has never seen that key.
    let other = tempfile::Builder::new().prefix("bgpg2").tempdir().unwrap();
    let generated = std::process::Command::new("gpg")
        .arg("--homedir")
        .arg(other.path())
        .args([
            "--batch",
            "--pinentry-mode",
            "loopback",
            "--passphrase",
            "",
            "--quick-gen-key",
            "Stranger <stranger@example.com>",
            "ed25519",
            "sign",
            "never",
        ])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(generated.success());
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    let buffer = repo
        .commit_create_buffer(&sig, &sig, "stranger", &base.tree().unwrap(), &[&base])
        .unwrap();
    let mut child = std::process::Command::new("gpg")
        .arg("--homedir")
        .arg(other.path())
        .args(["-bsau", "stranger@example.com"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(&buffer).unwrap();
    let out = child.wait_with_output().unwrap();
    let forged = repo
        .commit_signed(
            std::str::from_utf8(&buffer).unwrap(),
            std::str::from_utf8(&out.stdout).unwrap(),
            None,
        )
        .unwrap();
    let _ = dir;

    let result = verify_commit(&repo, &forged.to_string()).unwrap();

    assert_eq!(result.status, SignatureStatus::UnknownKey);
    assert!(result.key.is_some());
}

#[test]
fn missing_verifier_reports_error_status_not_err() {
    require_gpg!();
    let (dir, mut repo) = signing_repo();
    let id = commit_file(dir.path(), &mut repo, "a.txt");
    repo.config()
        .unwrap()
        .set_str("gpg.program", "browsitory-no-such-gpg")
        .unwrap();

    let result = verify_commit(&repo, &id).unwrap();

    assert_eq!(result.status, SignatureStatus::Error);
    assert_eq!(
        result.detail.as_deref(),
        Some("browsitory-no-such-gpg was not found")
    );
}
