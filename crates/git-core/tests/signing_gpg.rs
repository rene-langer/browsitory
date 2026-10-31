//! gpg tests live in their own test binary: the fixture sets GNUPGHOME process-wide (once,
//! before any test here spawns gpg) so concurrent tests in other binaries are unaffected.
//!
//! Every keyring here is a throwaway temp dir, and gpg auto-starts a `gpg-agent` for each one
//! that outlives the test binary unless it is stopped explicitly (#188): leaked agents each
//! hold an inotify instance until the per-user limit is exhausted. [`Keyring`] stops them.

mod common;

use std::path::Path;
use std::sync::OnceLock;

use common::{commit_all, init_repo, program_available, write_file};
use git_core::signing::{verify_commit, SignatureFormat, SignatureStatus};

const UID: &str = "Test User <test@example.com>";

/// A throwaway gpg home dir. Dropping it stops the gpg daemons gpg started for it, then
/// deletes it, so neither survives a test that panics.
struct Keyring(tempfile::TempDir);

impl Keyring {
    /// A new, empty keyring dir under the system temp dir, named `<prefix>XXXXXX`.
    fn new(prefix: &str) -> Self {
        Keyring(tempfile::Builder::new().prefix(prefix).tempdir().unwrap())
    }

    fn path(&self) -> &Path {
        self.0.path()
    }
}

impl Drop for Keyring {
    fn drop(&mut self) {
        shut_down_keyring(self.path());
    }
}

/// Stops every gpg daemon (`gpg-agent`, plus `keyboxd`/`dirmngr` if gpg started them) running
/// for `home`, then deletes `home`. Best-effort: errors are ignored.
fn shut_down_keyring(home: &Path) {
    let _ = std::process::Command::new("gpgconf")
        .arg("--homedir")
        .arg(home)
        .args(["--kill", "all"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    let _ = std::fs::remove_dir_all(home);
}

/// The process keyring behind [`gpg_home`]. A static is never dropped, so [`Keyring`]'s `Drop`
/// never runs for it; [`stop_process_keyring`] (an `atexit` handler) cleans it up instead.
static HOME: OnceLock<Option<Keyring>> = OnceLock::new();

/// `atexit` handler. The test harness's `main` either returns or calls `std::process::exit`,
/// and both run `atexit` handlers. (A process killed by a signal still leaks the agent.)
#[cfg(unix)]
extern "C" fn stop_process_keyring() {
    if let Some(Some(home)) = HOME.get() {
        shut_down_keyring(home.path());
    }
}

/// Some(path) once a throwaway keyring with an ultimately trusted, passphrase-less key for `UID`
/// is the process's GNUPGHOME; None when gpg is unavailable.
fn gpg_home() -> Option<&'static Path> {
    HOME.get_or_init(|| {
        // Git-for-Windows' gpg is an MSYS binary: it reads `C:\...` as a relative path and
        // cannot start gpg-agent in CI, so these tests run on Unix only.
        if cfg!(windows) || !program_available("gpg") {
            return None;
        }
        // Until it is stored in `HOME`, a panic below drops `home`, which stops its agent.
        let home = Keyring::new("bgpg");
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
        // Registered once: `get_or_init` runs this closure to completion at most once.
        #[cfg(unix)]
        {
            // SAFETY: `atexit` only stores the function pointer. The handler is a plain
            // `extern "C" fn` that does not unwind (every error in it is ignored).
            let registered = unsafe { libc::atexit(stop_process_keyring) };
            assert_eq!(registered, 0, "atexit could not register the gpg cleanup");
        }
        Some(home)
    })
    .as_ref()
    .map(Keyring::path)
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
    git_core::commit::commit(
        repo,
        &format!("add {name}"),
        git_core::hooks::HookOptions::default(),
    )
    .unwrap()
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
    // Dropping `other` stops the agent gpg starts for it below, even if an assertion panics.
    let other = Keyring::new("bgpg2");
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
