# SM Parity Batch 3c (Commit Signing) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Commits Browsitory creates are signed when git config says `commit.gpgSign=true` (OpenPGP via `gpg`, SSH via `ssh-keygen`), and the commit details pane shows each commit's signature status, in both the Tauri app and the VSCode extension.

**Architecture:** New `git-core::signing` module owns three things. `signing_config` reads git config; `create_commit` writes a commit object (signed through `commit_create_buffer` → signer subprocess → `commit_signed`, or unsigned through `repo.commit(None, …)`) and then moves HEAD itself with libgit2's exact reflog message; `verify_commit` runs `gpg --verify` / `ssh-keygen -Y verify|check-novalidate` and maps the output to a `SignatureStatus`. All five existing commit sites switch to `create_commit`. `repo-service` adds a `GetCommitSignature` read command, Tauri and the sidecar expose `get_commit_signature`, and `CommitHeader` renders a "Signature" row.

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `tempfile` 3, `serde` in tauri-app/sidecar), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver), `@vscode/test-electron`.

**Spec:** `docs/superpowers/specs/2026-10-19-sm-parity-batch3c-design.md`

## Global Constraints

- Formats: `openpgp` (default when `gpg.format` is unset) and `ssh`. `gpg.format=x509` → `SignError::UnsupportedFormat("x509")` when signing; `Error` status ("unsupported signature type") when verifying.
- Config keys honored, exactly: `commit.gpgSign`, `gpg.format`, `user.signingKey`, `gpg.openpgp.program` > `gpg.program` > `gpg`, `gpg.ssh.program` > `ssh-keygen`, `gpg.ssh.allowedSignersFile`. Nothing else (no `gpg.ssh.defaultKeyCommand`, no `tag.gpgSign`).
- Signer invocations, exactly:
  - gpg sign: `<program> --status-fd=2 -bsau <key>`
  - ssh sign: `<program> -Y sign -n git -f <keyfile>`, plus `-U` when the key is a literal
  - gpg verify: `<program> --status-fd=1 --verify <sigfile> -`
  - ssh verify: `-Y find-principals -f <allowed> -s <sigfile>`, `-Y verify -n git -f <allowed> -I <principal> -s <sigfile>`, `-Y check-novalidate -n git -s <sigfile>`
  - The buffer always goes on stdin.
- Reflog messages are byte-identical to what libgit2 wrote before this batch: `commit: <summary>`, `commit (initial): <summary>`, `commit (merge): <summary>`, `commit (amend): <summary>`.
- A signing failure must leave HEAD, every ref, the index and the repository state (MERGE_HEAD, CHERRY_PICK_HEAD, rebase state) untouched.
- Error copy, exactly:
  - "Signing is configured with gpg.format=x509, which Browsitory does not support."
  - "Signing with SSH needs user.signingKey to be set."
  - "Signing program 'gpg' was not found." (program name as configured)
  - "Signing failed: ⟨stderr tail⟩"
- UI copy, exactly:
  - row label "Signature", loading text "Checking signature…"
  - status texts per the spec table: Good signature / Valid signature, signer not trusted / Signed with an unknown key / Signature expired / Signed with an expired key / Bad signature / Signed with a revoked key / Signature could not be checked
- Wire format: `status` values `unsigned|good|goodUntrusted|bad|unknownKey|expired|expiredKey|revokedKey|error`; `format` values `openpgp|ssh|null`.
- Dependencies: only `tempfile` moves from dev to runtime in `git-core` (already vetted, MIT OR Apache-2.0); update its `docs/LICENSE_COMPLIANCE.md` row. No other new Rust or npm deps.
- Tests: `git-core`/`repo-service` use real temp-dir repos and real `gpg`/`ssh-keygen` with throwaway keys. A test whose program is missing prints `skipping: <program> not on PATH` and returns. Frontend mocks `RepoClient` only.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook); the changelog lands in Task 9, so push only after Task 9.
- Do not change `repo-service/worker/undo_store.rs`'s anchor commit. It stays unsigned through `repo.commit`.

## Review Focus

1. **Signing on during a conflicted merge, signer fails.** MERGE_HEAD must survive and HEAD stay put, so the user can fix the key and commit again. Pinned in Task 2 (`failed_signing_during_a_merge_keeps_the_merge_in_progress`).
2. **Literal SSH public key in `user.signingKey`** (the GitHub-docs setup, `key::ssh-ed25519 AAAA…`) has to sign through the agent. Pinned in Task 1 (`signs_with_a_literal_ssh_public_key_through_the_agent`), which is skipped when no `ssh-agent` is available.
3. **A detached HEAD commit** (e.g. after "check out commit") moves HEAD directly and does not create a branch named `HEAD`. Pinned in Task 1 (`signed_commit_on_a_detached_head_moves_head_only`).
4. **History from the other format.** An SSH-signed commit in a repo configured for openpgp (or with no signing config at all) still verifies by its armor header. Pinned in Task 3 (`verifies_ssh_signature_without_any_signing_config`).
5. **gpg absent on the machine while history has PGP-signed commits.** The header still renders and the row says "Signature could not be checked" instead of failing the pane. Pinned in Task 3 (`missing_verifier_reports_error_status_not_err`) and Task 7 (`shows the error status text`).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/signing.rs` (new) | `SigningConfig`, `signing_config`, `sign_buffer`, `HeadUpdate`, `create_commit`, `verify_commit`, status parsing |
| `crates/git-core/src/lib.rs` | `pub mod signing;` |
| `crates/git-core/Cargo.toml` | `tempfile = "3"` under `[dependencies]` |
| `crates/git-core/src/commit.rs`, `pick.rs`, `rebase.rs` | switch to `create_commit`; `Sign(#[from] SignError)` error variants |
| `crates/git-core/tests/common/mod.rs` | `program_available`, `ssh_key_fixture`, `enable_ssh_signing` helpers |
| `crates/git-core/tests/create_commit.rs` (new) | unsigned `create_commit` + reflog parity with libgit2 |
| `crates/git-core/tests/signing_ssh.rs` (new) | ssh signing at every commit site, failures, ssh verification |
| `crates/git-core/tests/signing_gpg.rs` (new) | gpg signing and verification (own binary because it sets `GNUPGHOME`) |
| `crates/repo-service/src/worker/mod.rs`, `status.rs` | `Command::GetCommitSignature`, handler, `WorkerHandle::get_commit_signature`, tests |
| `crates/tauri-app/src/commands/mod.rs`, `status.rs`, `main.rs` | `CommitSignatureDto`, `get_commit_signature` command |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | `get_commit_signature` method + DTO |
| `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` (+ tests) | `CommitSignature` type, `getCommitSignature` |
| `frontend/src/components/CommitHeader.tsx`, `.module.css`, `.test.tsx` | Signature row |
| `frontend/src/lib/signatureStatus.ts` (new) + test | status → tone/text, short key |
| `e2e/specs/commit-signing.spec.ts`, `extension/e2e/src/specs/commit-signing.spec.ts` (new) | E2E |
| `CHANGELOG.md`, `docs/USER_GUIDE.md`, `docs/ARCHITECTURE.md`, `docs/LICENSE_COMPLIANCE.md` | docs |

---

### Task 1: `git-core::signing` — config, signer, `create_commit`

**Files:**
- Create: `crates/git-core/src/signing.rs`
- Modify: `crates/git-core/src/lib.rs`, `crates/git-core/Cargo.toml`, `docs/LICENSE_COMPLIANCE.md:19`, `crates/git-core/tests/common/mod.rs`
- Test: `crates/git-core/tests/create_commit.rs`, `crates/git-core/tests/signing_ssh.rs`

**Interfaces:**
- Produces (used by Tasks 2–4):

```rust
pub enum SignatureFormat { OpenPgp, Ssh }
pub struct SigningConfig { pub format: SignatureFormat, pub program: String, pub key: Option<String> }
pub enum SignError { UnsupportedFormat(String), MissingSshKey, SignerNotFound(String), SignerFailed(String), Git(git2::Error), Io(std::io::Error) }
pub fn signing_config(repo: &Repository) -> Result<Option<SigningConfig>, SignError>;
pub fn sign_buffer(config: &SigningConfig, committer: &Signature<'_>, buffer: &[u8]) -> Result<String, SignError>;
pub struct HeadUpdate<'a> { pub expected: Option<Oid>, pub reflog_operation: &'a str }
pub fn create_commit(repo: &Repository, head_update: Option<HeadUpdate<'_>>, author: &Signature<'_>, committer: &Signature<'_>, message: &str, tree: &Tree<'_>, parents: &[&Commit<'_>]) -> Result<Oid, SignError>;
pub(crate) fn run_program(program: &str, args: &[&str], stdin: &[u8]) -> Result<ProgramOutput, SignError>;
pub(crate) struct ProgramOutput { pub status_success: bool, pub stdout: String, pub stderr: String }
```

- Test helpers (in `tests/common/mod.rs`):
  - `program_available(&str) -> bool`
  - `ssh_key_fixture(dir: &Path) -> SshKey { private: PathBuf, public: PathBuf, public_line: String }`
  - `enable_ssh_signing(repo, &SshKey)`

> **Deviation from the spec (spec says `update_ref: Option<&str>` + `reflog_kind`).** Every caller updates
> `HEAD`, and amend's expected current target is HEAD's commit, not `parents[0]`. `HeadUpdate`
> therefore carries the expected current HEAD oid explicitly (`None` = unborn branch).

- [ ] **Step 1: Promote `tempfile`, register the module, add test helpers**

`crates/git-core/Cargo.toml`: move `tempfile = "3"` from `[dev-dependencies]` to `[dependencies]`. Leave `[dev-dependencies]` in place even if it ends up empty.

`docs/LICENSE_COMPLIANCE.md` line 19: change the tempfile row's last column to:
`dev-dependency of \`tauri-app\` and \`repo-service\`; runtime dependency of \`config\` (atomic config-file writes) and \`git-core\` (signature temp files for the signer programs)`

`crates/git-core/src/lib.rs`: add `pub mod signing;` after `pub mod reset;` (alphabetical: `reset`, `signing`, `stage`).

Append to `crates/git-core/tests/common/mod.rs`:

```rust
/// True when `program` can be spawned at all. Used to skip signing tests on machines (Windows
/// CI) without gpg / ssh-keygen.
#[allow(dead_code)]
pub fn program_available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

#[allow(dead_code)]
pub struct SshKey {
    pub private: std::path::PathBuf,
    pub public: std::path::PathBuf,
    /// The public key file's single line, e.g. "ssh-ed25519 AAAA… test@example.com".
    pub public_line: String,
}

/// Generates a passphrase-less ed25519 key pair in `dir` plus an allowed-signers file
/// (`dir/allowed_signers`) that trusts it for principal `test@example.com`.
#[allow(dead_code)]
pub fn ssh_key_fixture(dir: &Path) -> SshKey {
    let private = dir.join("id_test");
    let status = std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "test@example.com", "-f"])
        .arg(&private)
        .status()
        .expect("run ssh-keygen");
    assert!(status.success(), "ssh-keygen failed");
    let public = dir.join("id_test.pub");
    let public_line = std::fs::read_to_string(&public).unwrap().trim().to_string();
    std::fs::write(
        dir.join("allowed_signers"),
        format!("test@example.com {public_line}\n"),
    )
    .unwrap();
    SshKey { private, public, public_line }
}

/// `commit.gpgSign=true`, `gpg.format=ssh`, `user.signingKey=<private key path>`.
#[allow(dead_code)]
pub fn enable_ssh_signing(repo: &Repository, key: &SshKey) {
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "ssh").unwrap();
    config
        .set_str("user.signingKey", key.private.to_str().unwrap())
        .unwrap();
}
```

- [ ] **Step 2: Write the failing unsigned/reflog-parity tests**

`crates/git-core/tests/create_commit.rs`:

```rust
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::signing::{create_commit, HeadUpdate};

fn last_reflog_message(repo: &git2::Repository, name: &str) -> String {
    let reflog = repo.reflog(name).unwrap();
    reflog.get(0).unwrap().message().unwrap().to_string()
}

fn head_tree(repo: &git2::Repository) -> git2::Oid {
    let mut index = repo.index().unwrap();
    index.write_tree().unwrap()
}

#[test]
fn initial_commit_matches_libgit2_reflog_and_moves_unborn_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(head_tree(&repo)).unwrap();
    let sig = repo.signature().unwrap();

    let oid = create_commit(
        &repo,
        Some(HeadUpdate { expected: None, reflog_operation: "commit" }),
        &sig, &sig, "first\n\nbody", &tree, &[],
    )
    .unwrap();

    assert_eq!(repo.head().unwrap().target(), Some(oid));
    assert_eq!(last_reflog_message(&repo, "HEAD"), "commit (initial): first");
    let branch = repo.head().unwrap().name().unwrap().to_string();
    assert_eq!(last_reflog_message(&repo, &branch), "commit (initial): first");
}

#[test]
fn normal_commit_matches_libgit2_reflog() {
    // Control: what libgit2's own repo.commit(Some("HEAD")) writes.
    let (control_dir, control) = init_repo();
    write_file(control_dir.path(), "a.txt", "a\n");
    commit_all(&control, "base");
    write_file(control_dir.path(), "b.txt", "b\n");
    commit_all(&control, "second");
    let expected = last_reflog_message(&control, "HEAD");

    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "b.txt", "b\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("b.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(head_tree(&repo)).unwrap();
    let sig = repo.signature().unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();

    create_commit(
        &repo,
        Some(HeadUpdate { expected: Some(parent.id()), reflog_operation: "commit" }),
        &sig, &sig, "second", &tree, &[&parent],
    )
    .unwrap();

    assert_eq!(last_reflog_message(&repo, "HEAD"), expected);
    assert_eq!(expected, "commit: second");
}

#[test]
fn two_parent_commit_uses_the_merge_suffix() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let tree = base.tree().unwrap();
    let sig = repo.signature().unwrap();
    let other = repo.find_commit(
        repo.commit(None, &sig, &sig, "other", &tree, &[&base]).unwrap(),
    ).unwrap();

    create_commit(
        &repo,
        Some(HeadUpdate { expected: Some(base.id()), reflog_operation: "commit" }),
        &sig, &sig, "merge it", &tree, &[&base, &other],
    )
    .unwrap();

    assert_eq!(last_reflog_message(&repo, "HEAD"), "commit (merge): merge it");
}

#[test]
fn refuses_when_head_moved_and_moves_nothing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "moved on");
    let tip = repo.head().unwrap().target().unwrap();
    let sig = repo.signature().unwrap();
    let tree = base.tree().unwrap();

    let result = create_commit(
        &repo,
        Some(HeadUpdate { expected: Some(base.id()), reflog_operation: "commit" }),
        &sig, &sig, "stale", &tree, &[&base],
    );

    assert!(result.is_err());
    assert_eq!(repo.head().unwrap().target(), Some(tip));
}

#[test]
fn without_head_update_writes_the_object_only() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();

    let oid = create_commit(&repo, None, &sig, &sig, "loose", &base.tree().unwrap(), &[&base])
        .unwrap();

    assert_eq!(repo.head().unwrap().target(), Some(base.id()));
    assert_eq!(repo.find_commit(oid).unwrap().summary(), Some("loose"));
}

#[test]
fn unsigned_when_commit_gpgsign_is_unset() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    let oid = create_commit(&repo, None, &sig, &sig, "x", &base.tree().unwrap(), &[&base]).unwrap();
    assert_eq!(
        repo.extract_signature(&oid, None).unwrap_err().code(),
        git2::ErrorCode::NotFound
    );
    assert!(git_core::signing::signing_config(&repo).unwrap().is_none());
}

#[test]
fn x509_format_is_refused() {
    let (_dir, repo) = init_repo();
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "x509").unwrap();
    let err = git_core::signing::signing_config(&repo).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Signing is configured with gpg.format=x509, which Browsitory does not support."
    );
}

#[test]
fn ssh_without_signing_key_is_refused() {
    let (_dir, repo) = init_repo();
    let mut config = repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.format", "ssh").unwrap();
    let err = git_core::signing::signing_config(&repo).unwrap_err();
    assert_eq!(err.to_string(), "Signing with SSH needs user.signingKey to be set.");
}
```

`init_repo` sets config on the repo-local config. A user's global `commit.gpgSign=true` could still leak into a test repo, but `signing_config` reads `repo.config()`, which includes global. To keep tests hermetic, `init_repo` must pin `commit.gpgSign=false`. Add this line in `tests/common/mod.rs::init_repo` after `user.email`:

```rust
        config.set_bool("commit.gpgSign", false).unwrap();
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p git-core --test create_commit`
Expected: FAIL to compile with `could not find signing in git_core`.

- [ ] **Step 4: Implement `signing.rs` (config, signer, create_commit)**

`crates/git-core/src/signing.rs`:

```rust
//! Commit signing (OpenPGP via gpg, SSH via ssh-keygen) and signature verification. See
//! `docs/superpowers/specs/2026-10-19-sm-parity-batch3c-design.md`.

use std::io::Write;
use std::process::{Command, Stdio};

use git2::{Commit, Oid, Repository, Signature, Tree};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureFormat {
    OpenPgp,
    Ssh,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningConfig {
    pub format: SignatureFormat,
    pub program: String,
    pub key: Option<String>,
}

#[derive(Debug, Error)]
pub enum SignError {
    #[error("Signing is configured with gpg.format={0}, which Browsitory does not support.")]
    UnsupportedFormat(String),
    #[error("Signing with SSH needs user.signingKey to be set.")]
    MissingSshKey,
    #[error("Signing program '{0}' was not found.")]
    SignerNotFound(String),
    #[error("Signing failed: {0}")]
    SignerFailed(String),
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

fn config_string(config: &git2::Config, key: &str) -> Option<String> {
    config.get_string(key).ok().filter(|value| !value.is_empty())
}

/// The program that signs/verifies `format`, honoring the same config precedence as git.
pub(crate) fn program_for(config: &git2::Config, format: SignatureFormat) -> String {
    match format {
        SignatureFormat::OpenPgp => config_string(config, "gpg.openpgp.program")
            .or_else(|| config_string(config, "gpg.program"))
            .unwrap_or_else(|| "gpg".to_string()),
        SignatureFormat::Ssh => {
            config_string(config, "gpg.ssh.program").unwrap_or_else(|| "ssh-keygen".to_string())
        }
    }
}

/// `Ok(None)` when `commit.gpgSign` is false or unset.
pub fn signing_config(repo: &Repository) -> Result<Option<SigningConfig>, SignError> {
    let config = repo.config()?;
    if !config.get_bool("commit.gpgSign").unwrap_or(false) {
        return Ok(None);
    }
    let format = match config_string(&config, "gpg.format").as_deref() {
        None | Some("openpgp") => SignatureFormat::OpenPgp,
        Some("ssh") => SignatureFormat::Ssh,
        Some(other) => return Err(SignError::UnsupportedFormat(other.to_string())),
    };
    let key = config_string(&config, "user.signingKey");
    if format == SignatureFormat::Ssh && key.is_none() {
        return Err(SignError::MissingSshKey);
    }
    Ok(Some(SigningConfig {
        format,
        program: program_for(&config, format),
        key,
    }))
}

pub(crate) struct ProgramOutput {
    pub status_success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `program args…` with `stdin` piped in and stdout/stderr captured; no TTY, no console
/// window on Windows. Stdin is written from a separate thread so a signer that writes a lot to
/// stderr before reading all of stdin cannot deadlock against us.
pub(crate) fn run_program(
    program: &str,
    args: &[&str],
    stdin: &[u8],
) -> Result<ProgramOutput, SignError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            SignError::SignerNotFound(program.to_string())
        } else {
            SignError::Io(error)
        }
    })?;
    let mut child_stdin = child.stdin.take().expect("stdin is piped");
    let input = stdin.to_vec();
    let writer = std::thread::spawn(move || {
        // A signer that exits early closes its end; that is reported through its exit status.
        let _ = child_stdin.write_all(&input);
    });
    let output = child.wait_with_output()?;
    let _ = writer.join();
    Ok(ProgramOutput {
        status_success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Last few non-empty stderr lines, without gpg's `[GNUPG:]` status lines.
fn stderr_tail(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("[GNUPG:]"))
        .collect();
    let start = lines.len().saturating_sub(3);
    let tail = lines[start..].join("\n");
    if tail.is_empty() {
        "the signing program exited with an error".to_string()
    } else {
        tail
    }
}

fn is_literal_ssh_key(key: &str) -> Option<&str> {
    if let Some(rest) = key.strip_prefix("key::") {
        Some(rest)
    } else if key.starts_with("ssh-") {
        Some(key)
    } else {
        None
    }
}

fn expand_tilde(path: &str) -> std::path::PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
        if let Some(home) = home {
            return std::path::PathBuf::from(home).join(rest);
        }
    }
    std::path::PathBuf::from(path)
}

/// Detached, ASCII-armored signature over `buffer`. `committer` supplies gpg's default key
/// (`Name <email>`) when `user.signingKey` is unset, as git does.
pub fn sign_buffer(
    config: &SigningConfig,
    committer: &Signature<'_>,
    buffer: &[u8],
) -> Result<String, SignError> {
    match config.format {
        SignatureFormat::OpenPgp => {
            let key = config.key.clone().unwrap_or_else(|| {
                format!(
                    "{} <{}>",
                    committer.name().unwrap_or_default(),
                    committer.email().unwrap_or_default()
                )
            });
            let output = run_program(&config.program, &["--status-fd=2", "-bsau", &key], buffer)?;
            if !output.status_success || !output.stderr.contains("[GNUPG:] SIG_CREATED ") {
                return Err(SignError::SignerFailed(stderr_tail(&output.stderr)));
            }
            Ok(output.stdout)
        }
        SignatureFormat::Ssh => {
            let key = config.key.as_deref().ok_or(SignError::MissingSshKey)?;
            // A literal public key is written to a temp file; ssh-keygen then signs with the
            // matching private key from the agent (`-U`), exactly like git.
            let mut _literal_file: Option<tempfile::NamedTempFile> = None;
            let (key_path, use_agent) = match is_literal_ssh_key(key) {
                Some(literal) => {
                    let mut file = tempfile::NamedTempFile::new()?;
                    writeln!(file, "{literal}")?;
                    let path = file.path().to_path_buf();
                    _literal_file = Some(file);
                    (path, true)
                }
                None => (expand_tilde(key), false),
            };
            let key_arg = key_path.to_string_lossy().into_owned();
            let mut args = vec!["-Y", "sign", "-n", "git", "-f", key_arg.as_str()];
            if use_agent {
                args.push("-U");
            }
            let output = run_program(&config.program, &args, buffer)?;
            if !output.status_success || !output.stdout.contains("-----BEGIN SSH SIGNATURE-----") {
                return Err(SignError::SignerFailed(stderr_tail(&output.stderr)));
            }
            Ok(output.stdout)
        }
    }
}

/// How `create_commit` moves HEAD after writing the commit object.
pub struct HeadUpdate<'a> {
    /// The commit HEAD must currently point at; `None` = HEAD's branch is unborn.
    pub expected: Option<Oid>,
    /// `"commit"` or `"commit (amend)"`; the reflog message is built from it like libgit2 does.
    pub reflog_operation: &'a str,
}

/// libgit2's `git_reference__update_for_commit` message: `<op><type>: <summary>`, where type is
/// ` (initial)` for a parentless commit and ` (merge)` for one with two or more parents.
fn reflog_message(operation: &str, commit: &Commit<'_>) -> String {
    let kind = match commit.parent_count() {
        0 => " (initial)",
        1 => "",
        _ => " (merge)",
    };
    format!("{operation}{kind}: {}", commit.summary().unwrap_or_default())
}

fn move_head(repo: &Repository, oid: Oid, update: &HeadUpdate<'_>) -> Result<(), SignError> {
    let commit = repo.find_commit(oid)?;
    let message = reflog_message(update.reflog_operation, &commit);
    let head = repo.find_reference("HEAD")?;
    // Attached HEAD: move its branch (libgit2 appends HEAD's reflog too). Detached: move HEAD.
    let target_name = match head.symbolic_target() {
        Some(branch) => branch.to_string(),
        None => "HEAD".to_string(),
    };
    match update.expected {
        Some(expected) => {
            repo.reference_matching(&target_name, oid, true, expected, &message)?;
        }
        None => {
            repo.reference(&target_name, oid, false, &message)?;
        }
    }
    Ok(())
}

/// The single entry point for every commit `git-core` creates for the user. Writes the commit
/// object (signed when `commit.gpgSign` asks for it), then, when `head_update` is set, moves HEAD.
/// The signer runs before any ref moves, so a signing failure changes nothing.
#[allow(clippy::too_many_arguments)]
pub fn create_commit(
    repo: &Repository,
    head_update: Option<HeadUpdate<'_>>,
    author: &Signature<'_>,
    committer: &Signature<'_>,
    message: &str,
    tree: &Tree<'_>,
    parents: &[&Commit<'_>],
) -> Result<Oid, SignError> {
    let oid = match signing_config(repo)? {
        None => repo.commit(None, author, committer, message, tree, parents)?,
        Some(config) => {
            let buffer = repo.commit_create_buffer(author, committer, message, tree, parents)?;
            let signature = sign_buffer(&config, committer, &buffer)?;
            let content = std::str::from_utf8(&buffer).map_err(|_| {
                SignError::Git(git2::Error::from_str("commit buffer is not valid UTF-8"))
            })?;
            repo.commit_signed(content, &signature, None)?
        }
    };
    if let Some(update) = head_update {
        move_head(repo, oid, &update)?;
    }
    Ok(oid)
}
```

The literal-key branch keeps the `NamedTempFile` in `_literal_file` (outer scope) so it is not deleted before ssh-keygen reads it.

- [ ] **Step 5: Run the unsigned tests**

Run: `cargo test -p git-core --test create_commit`
Expected: PASS (8 tests).

- [ ] **Step 6: Write the failing SSH signing tests for `create_commit`**

`crates/git-core/tests/signing_ssh.rs` (first part; Task 2 and Task 3 append to it):

```rust
mod common;

use common::{commit_all, enable_ssh_signing, init_repo, program_available, ssh_key_fixture, write_file};
use git_core::signing::{create_commit, HeadUpdate};

macro_rules! require {
    ($program:expr) => {
        if !program_available($program) {
            eprintln!("skipping: {} not on PATH", $program);
            return;
        }
    };
}

fn is_ssh_signed(repo: &git2::Repository, oid: git2::Oid) -> bool {
    match repo.extract_signature(&oid, None) {
        Ok((signature, _)) => std::str::from_utf8(&signature)
            .unwrap()
            .starts_with("-----BEGIN SSH SIGNATURE-----"),
        Err(_) => false,
    }
}

#[test]
fn create_commit_signs_with_an_ssh_key_file() {
    require!("ssh-keygen");
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    enable_ssh_signing(&repo, &key);
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();

    let oid = create_commit(
        &repo,
        Some(HeadUpdate { expected: Some(base.id()), reflog_operation: "commit" }),
        &sig, &sig, "signed one", &base.tree().unwrap(), &[&base],
    )
    .unwrap();

    assert!(is_ssh_signed(&repo, oid));
    assert_eq!(repo.head().unwrap().target(), Some(oid));
    let reflog = repo.reflog("HEAD").unwrap();
    assert_eq!(reflog.get(0).unwrap().message(), Some("commit: signed one"));
}

#[test]
fn signed_commit_on_a_detached_head_moves_head_only() {
    require!("ssh-keygen");
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let branch = repo.head().unwrap().name().unwrap().to_string();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.set_head_detached(base.id()).unwrap();
    enable_ssh_signing(&repo, &key);
    let sig = repo.signature().unwrap();

    let oid = create_commit(
        &repo,
        Some(HeadUpdate { expected: Some(base.id()), reflog_operation: "commit" }),
        &sig, &sig, "detached", &base.tree().unwrap(), &[&base],
    )
    .unwrap();

    assert!(repo.head_detached().unwrap());
    assert_eq!(repo.head().unwrap().target(), Some(oid));
    assert_eq!(repo.find_reference(&branch).unwrap().target(), Some(base.id()));
    assert!(repo.find_reference("refs/heads/HEAD").is_err());
}

#[test]
fn signer_failure_leaves_head_untouched() {
    require!("ssh-keygen");
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config.set_str("gpg.format", "ssh").unwrap();
        config.set_str("user.signingKey", "/nonexistent/key").unwrap();
    }
    let sig = repo.signature().unwrap();

    let err = create_commit(
        &repo,
        Some(HeadUpdate { expected: Some(base.id()), reflog_operation: "commit" }),
        &sig, &sig, "nope", &base.tree().unwrap(), &[&base],
    )
    .unwrap_err();

    assert!(err.to_string().starts_with("Signing failed: "), "{err}");
    assert_eq!(repo.head().unwrap().target(), Some(base.id()));
}

#[test]
fn missing_signer_program_is_reported_by_name() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config.set_str("gpg.program", "browsitory-no-such-gpg").unwrap();
    }
    let sig = repo.signature().unwrap();
    let err = create_commit(&repo, None, &sig, &sig, "x", &base.tree().unwrap(), &[&base])
        .unwrap_err();
    assert_eq!(err.to_string(), "Signing program 'browsitory-no-such-gpg' was not found.");
}

#[test]
fn signs_with_a_literal_ssh_public_key_through_the_agent() {
    require!("ssh-keygen");
    require!("ssh-agent");
    require!("ssh-add");
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    // Private socket for a throwaway agent, killed at the end of the test.
    let socket = keys.path().join("agent.sock");
    let mut agent = std::process::Command::new("ssh-agent")
        .args(["-D", "-a"])
        .arg(&socket)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    for _ in 0..50 {
        if socket.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let added = std::process::Command::new("ssh-add")
        .env("SSH_AUTH_SOCK", &socket)
        .arg(&key.private)
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(added.success());
    // ssh-keygen finds the agent through SSH_AUTH_SOCK; this test binary sets it only here
    // and no other test in this file reads it.
    std::env::set_var("SSH_AUTH_SOCK", &socket);

    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config.set_str("gpg.format", "ssh").unwrap();
        config
            .set_str("user.signingKey", &format!("key::{}", key.public_line))
            .unwrap();
    }
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    let result = create_commit(&repo, None, &sig, &sig, "agent", &base.tree().unwrap(), &[&base]);
    let _ = agent.kill();

    assert!(is_ssh_signed(&repo, result.unwrap()));
}
```

`tempfile` is reachable from integration tests because it is now a regular dependency of `git-core`. Add `tempfile = "3"` back under `[dev-dependencies]` only if the compiler says otherwise; integration tests see normal deps.

- [ ] **Step 7: Run SSH tests**

Run: `cargo test -p git-core --test signing_ssh`
Expected: PASS (5 tests; literal-key test may print `skipping: ssh-agent not on PATH`).

- [ ] **Step 8: Lint and commit**

Run: `cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: no warnings, no diff.

```bash
git add crates/git-core docs/LICENSE_COMPLIANCE.md
git commit -m "feat(git-core): signing config, signer subprocess and create_commit"
```

---

### Task 2: Every commit site goes through `create_commit`

**Files:**
- Modify: `crates/git-core/src/commit.rs`, `crates/git-core/src/pick.rs:96-110`, `crates/git-core/src/rebase.rs:5-18,322-404`
- Test: `crates/git-core/tests/signing_ssh.rs` (append)

**Interfaces:**
- Consumes: `create_commit`, `HeadUpdate`, `SignError` (Task 1); test helpers from Task 1.
- Produces: `CommitError::Sign(SignError)`, `PickError::Sign(SignError)`, `RebaseError::Sign(SignError)`, each `#[error("{0}")]`.

- [ ] **Step 1: Write the failing per-site tests**

Append to `crates/git-core/tests/signing_ssh.rs`:

```rust
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};

fn signing_repo() -> Option<(tempfile::TempDir, tempfile::TempDir, git2::Repository)> {
    if !program_available("ssh-keygen") {
        eprintln!("skipping: ssh-keygen not on PATH");
        return None;
    }
    let (dir, repo) = init_repo();
    let keys = tempfile::TempDir::new().unwrap();
    let key = ssh_key_fixture(keys.path());
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    enable_ssh_signing(&repo, &key);
    Some((dir, keys, repo))
}

fn head_oid(repo: &git2::Repository) -> git2::Oid {
    repo.head().unwrap().target().unwrap()
}

fn stage(repo: &git2::Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(path)).unwrap();
    index.write().unwrap();
}

#[test]
fn commit_and_amend_are_signed_with_unchanged_reflog() {
    let Some((dir, _keys, mut repo)) = signing_repo() else { return };
    write_file(dir.path(), "a.txt", "a\n");
    stage(&repo, "a.txt");
    let oid = git_core::commit::commit(&mut repo, "add a").unwrap();
    let oid = git2::Oid::from_str(&oid).unwrap();
    assert!(is_ssh_signed(&repo, oid));
    assert_eq!(repo.reflog("HEAD").unwrap().get(0).unwrap().message(), Some("commit: add a"));

    let amended = git_core::commit::amend_head(&repo, "add a, amended").unwrap();
    let amended = git2::Oid::from_str(&amended).unwrap();
    assert!(is_ssh_signed(&repo, amended));
    assert_eq!(head_oid(&repo), amended);
    let original = repo.find_commit(oid).unwrap();
    let amended_commit = repo.find_commit(amended).unwrap();
    assert_eq!(amended_commit.parent_id(0).unwrap(), original.parent_id(0).unwrap());
    assert_eq!(amended_commit.author().name(), original.author().name());
    assert_eq!(
        repo.reflog("HEAD").unwrap().get(0).unwrap().message(),
        Some("commit (amend): add a, amended")
    );
}

#[test]
fn cherry_pick_and_revert_are_signed() {
    let Some((dir, _keys, mut repo)) = signing_repo() else { return };
    let base = head_oid(&repo);
    write_file(dir.path(), "a.txt", "a\n");
    stage(&repo, "a.txt");
    let picked = git_core::commit::commit(&mut repo, "add a").unwrap();

    let reverted = match git_core::pick::revert(&repo, &picked).unwrap() {
        git_core::pick::PickOutcome::Committed { commit_id } => commit_id,
        other => panic!("unexpected {other:?}"),
    };
    assert!(is_ssh_signed(&repo, git2::Oid::from_str(&reverted).unwrap()));

    // Back to base on a detached HEAD, then cherry-pick "add a" onto it.
    repo.set_head_detached(base).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    let cherry = match git_core::pick::cherry_pick(&repo, &picked).unwrap() {
        git_core::pick::PickOutcome::Committed { commit_id } => commit_id,
        other => panic!("unexpected {other:?}"),
    };
    assert!(is_ssh_signed(&repo, git2::Oid::from_str(&cherry).unwrap()));
}

#[test]
fn rebase_steps_and_squash_collapse_are_signed() {
    let Some((dir, _keys, mut repo)) = signing_repo() else { return };
    let onto = head_oid(&repo).to_string();
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        write_file(dir.path(), &format!("{name}.txt"), "x\n");
        stage(&repo, &format!("{name}.txt"));
        ids.push(git_core::commit::commit(&mut repo, &format!("add {name}")).unwrap());
    }
    let plan = vec![
        RebasePlanEntry { commit_id: ids[0].clone(), action: RebaseAction::Pick, combined_message: None },
        RebasePlanEntry { commit_id: ids[1].clone(), action: RebaseAction::Squash, combined_message: None },
        RebasePlanEntry {
            commit_id: ids[2].clone(),
            action: RebaseAction::Reword { message: "c reworded".into() },
            combined_message: None,
        },
    ];

    let (_state, result) = git_core::rebase::start_rebase(&repo, &onto, plan).unwrap();
    assert_eq!(result, RebaseStepResult::Done);

    let tip = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(tip.summary(), Some("c reworded"));
    assert!(is_ssh_signed(&repo, tip.id()));
    let collapsed = tip.parent(0).unwrap();
    assert_eq!(collapsed.summary(), Some("add a"));
    assert!(is_ssh_signed(&repo, collapsed.id()));
}

#[test]
fn failed_signing_during_a_merge_keeps_the_merge_in_progress() {
    let Some((dir, _keys, mut repo)) = signing_repo() else { return };
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &base, false).unwrap();
    write_file(dir.path(), "main.txt", "m\n");
    stage(&repo, "main.txt");
    git_core::commit::commit(&mut repo, "on main").unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    write_file(dir.path(), "feature.txt", "f\n");
    stage(&repo, "feature.txt");
    git_core::commit::commit(&mut repo, "on feature").unwrap();
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();

    let outcome = git_core::merge::start_merge(&repo, "feature").unwrap();
    assert_eq!(outcome, git_core::merge::MergeOutcome::Merged);
    let before = head_oid(&repo);
    repo.config().unwrap().set_str("user.signingKey", "/nonexistent/key").unwrap();

    let err = git_core::commit::commit(&mut repo, "merge feature").unwrap_err();

    assert!(err.to_string().starts_with("Signing failed: "), "{err}");
    assert_eq!(head_oid(&repo), before);
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
}

#[test]
fn signing_off_by_default_keeps_commits_unsigned() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    stage(&repo, "a.txt");
    let oid = git_core::commit::commit(&mut repo, "plain").unwrap();
    assert!(!is_ssh_signed(&repo, git2::Oid::from_str(&oid).unwrap()));
    assert!(repo.extract_signature(&git2::Oid::from_str(&oid).unwrap(), None).is_err());
}
```

If `MergeOutcome` lacks `PartialEq`/`Debug`, use `assert!(matches!(outcome, git_core::merge::MergeOutcome::Merged))` instead; the same goes for `RebaseStepResult` (it derives both). If `PickOutcome` lacks `Debug`, replace the `panic!("unexpected {other:?}")` arms with `_ => panic!("not committed")`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test signing_ssh`
Expected: the new site tests FAIL (`is_ssh_signed` false) because the sites still call `repo.commit`.

- [ ] **Step 3: Switch `commit.rs`**

In `crates/git-core/src/commit.rs`, add to `CommitError`:

```rust
    #[error("{0}")]
    Sign(#[from] crate::signing::SignError),
```

Replace the `repo.commit(...)` call in `commit` with:

```rust
        crate::signing::create_commit(
            repo,
            Some(crate::signing::HeadUpdate {
                expected: head_parent.as_ref().map(|commit| commit.id()),
                reflog_operation: "commit",
            }),
            pick_author.as_ref().unwrap_or(&signature),
            &signature,
            message,
            &tree,
            &parents,
        )?
```

Replace the body of `amend_head` after `let committer = repo.signature()?;` with:

```rust
    let parents: Vec<git2::Commit> = head.parents().collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = crate::signing::create_commit(
        repo,
        Some(crate::signing::HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit (amend)",
        }),
        &head.author(),
        &committer,
        message,
        &tree,
        &parent_refs,
    )?;
    Ok(oid.to_string())
```

Update the `amend_head` doc comment's last sentence to read: "Goes through `signing::create_commit`, so it is signed when `commit.gpgSign` asks for it."

- [ ] **Step 4: Switch `pick.rs`**

Add to `PickError`:

```rust
    #[error("{0}")]
    Sign(#[from] crate::signing::SignError),
```

Replace line 106:

```rust
    let oid = crate::signing::create_commit(
        repo,
        Some(crate::signing::HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit",
        }),
        &author,
        &committer,
        &message,
        &tree,
        &[&head],
    )?;
```

The signer runs before `repo.cleanup_state()`, so a signing failure leaves CHERRY_PICK_HEAD/REVERT_HEAD and the applied index in place. That matches git: the user can fix the key and commit (commit.rs finishes the pick).

- [ ] **Step 5: Switch `rebase.rs`**

Add to `RebaseError`:

```rust
    #[error("{0}")]
    Sign(#[from] crate::signing::SignError),
```

In `land_current_step`, replace the step commit:

```rust
    crate::signing::create_commit(
        repo,
        Some(crate::signing::HeadUpdate {
            expected: Some(parent.id()),
            reflog_operation: "commit",
        }),
        &original_commit.author(),
        &committer,
        &message,
        &tree,
        &[&parent],
    )?;
```

and the group collapse:

```rust
        let final_oid = crate::signing::create_commit(
            repo,
            None,
            &leader_commit.author(),
            &committer,
            &combined_message,
            &final_tree,
            &[&group_parent],
        )?;
```

The existing comment above the collapse ("Not `update_ref: Some("HEAD")` here…") stays accurate; change its first words to "No `HeadUpdate` here:".

A signing failure during a step: `land_current_step` returns `Err` before the cursor advances. `start_rebase` rolls back to the original branch (existing behavior for any `Err`). `rebase_continue` leaves the paused state as it was, so the user can fix the key and continue. No new code is needed for that.

- [ ] **Step 6: Run the whole git-core suite**

Run: `cargo test -p git-core`
Expected: PASS, including every pre-existing `amend.rs`, `pick.rs`, `rebase.rs`, `stage_commit.rs`, `merge.rs`, `reflog*.rs` and `undo.rs` test. If an existing test asserted `CommitError::Write`/`PickError::Git` for a commit-creation failure, it now sees `::Sign(SignError::Git(_))`. Update the match, not the behavior.

- [ ] **Step 7: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Then: `cargo test -p repo-service` (undo tests exercise commit/amend/pick/rebase through the worker).
Expected: PASS.

```bash
git add crates/git-core
git commit -m "feat(git-core): sign commit, amend, cherry-pick, revert and rebase commits"
```

---

### Task 3: `git-core::signing::verify_commit`

**Files:**
- Modify: `crates/git-core/src/signing.rs`
- Test: `crates/git-core/tests/signing_ssh.rs` (append), `crates/git-core/tests/signing_gpg.rs` (new)

**Interfaces:**
- Consumes: `run_program`, `program_for`, `SignatureFormat` (Task 1).
- Produces (used by Tasks 4–5):

```rust
pub enum SignatureStatus { Unsigned, Good, GoodUntrusted, Bad, UnknownKey, Expired, ExpiredKey, RevokedKey, Error }
pub struct CommitSignature { pub status: SignatureStatus, pub format: Option<SignatureFormat>, pub signer: Option<String>, pub key: Option<String>, pub detail: Option<String> }
pub fn verify_commit(repo: &Repository, commit_id: &str) -> Result<CommitSignature, SignError>;
```

- [ ] **Step 1: Write the failing SSH verification tests**

Append to `crates/git-core/tests/signing_ssh.rs`:

```rust
use git_core::signing::{verify_commit, SignatureFormat, SignatureStatus};

fn signed_head(dir: &std::path::Path, repo: &mut git2::Repository) -> String {
    write_file(dir, "v.txt", "v\n");
    stage(repo, "v.txt");
    git_core::commit::commit(repo, "verify me").unwrap()
}

#[test]
fn verifies_ssh_signature_as_good_with_allowed_signers() {
    let Some((dir, keys, mut repo)) = signing_repo() else { return };
    let id = signed_head(dir.path(), &mut repo);
    repo.config()
        .unwrap()
        .set_str("gpg.ssh.allowedSignersFile", keys.path().join("allowed_signers").to_str().unwrap())
        .unwrap();

    let result = verify_commit(&repo, &id).unwrap();

    assert_eq!(result.status, SignatureStatus::Good);
    assert_eq!(result.format, Some(SignatureFormat::Ssh));
    assert_eq!(result.signer.as_deref(), Some("test@example.com"));
    assert!(result.key.as_deref().unwrap().starts_with("SHA256:"));
}

#[test]
fn verifies_ssh_signature_without_any_signing_config() {
    let Some((dir, _keys, mut repo)) = signing_repo() else { return };
    let id = signed_head(dir.path(), &mut repo);
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
        config.set_str("gpg.format", "openpgp").unwrap();
    }

    let result = verify_commit(&repo, &id).unwrap();

    assert_eq!(result.status, SignatureStatus::GoodUntrusted);
    assert_eq!(result.format, Some(SignatureFormat::Ssh));
    assert_eq!(
        result.detail.as_deref(),
        Some("signer is not in gpg.ssh.allowedSignersFile")
    );
    assert!(result.key.as_deref().unwrap().starts_with("SHA256:"));
}

#[test]
fn unlisted_ssh_signer_is_good_untrusted() {
    let Some((dir, keys, mut repo)) = signing_repo() else { return };
    let id = signed_head(dir.path(), &mut repo);
    let other = keys.path().join("other_signers");
    std::fs::write(&other, "someone@example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGq2pQ0lYkTzUuWQnqf8NYvU0bS7u9a8m2g5Yb2G8b9h\n").unwrap();
    repo.config().unwrap().set_str("gpg.ssh.allowedSignersFile", other.to_str().unwrap()).unwrap();

    let result = verify_commit(&repo, &id).unwrap();

    assert_eq!(result.status, SignatureStatus::GoodUntrusted);
}

#[test]
fn tampered_ssh_signed_commit_is_bad() {
    let Some((dir, _keys, mut repo)) = signing_repo() else { return };
    let id = signed_head(dir.path(), &mut repo);
    let oid = git2::Oid::from_str(&id).unwrap();
    let (signature, data) = repo.extract_signature(&oid, None).unwrap();
    let tampered = std::str::from_utf8(&data).unwrap().replace("verify me", "verify m3");
    let forged = repo
        .commit_signed(&tampered, std::str::from_utf8(&signature).unwrap(), None)
        .unwrap();

    let result = verify_commit(&repo, &forged.to_string()).unwrap();

    assert_eq!(result.status, SignatureStatus::Bad);
}

#[test]
fn unsigned_commit_reports_unsigned() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    stage(&repo, "a.txt");
    let id = git_core::commit::commit(&mut repo, "plain").unwrap();
    let result = verify_commit(&repo, &id).unwrap();
    assert_eq!(result.status, SignatureStatus::Unsigned);
    assert_eq!(result.format, None);
}

#[test]
fn bad_commit_id_is_an_err() {
    let (_dir, repo) = init_repo();
    assert!(verify_commit(&repo, "not-a-sha").is_err());
}
```

`CommitSignature`/`SignatureStatus`/`SignatureFormat` must derive `Debug, Clone, PartialEq, Eq` (`Copy` for the two enums).

- [ ] **Step 2: Write the failing gpg tests**

`crates/git-core/tests/signing_gpg.rs`:

```rust
//! gpg tests live in their own test binary: the fixture sets GNUPGHOME process-wide (once,
//! before any test here spawns gpg) so concurrent tests in other binaries are unaffected.

mod common;

use std::sync::OnceLock;

use common::{commit_all, init_repo, program_available, write_file};
use git_core::signing::{verify_commit, SignatureFormat, SignatureStatus};

const UID: &str = "Test User <test@example.com>";

/// Some(()) once a throwaway keyring with an ultimately trusted, passphrase-less key for `UID`
/// is the process's GNUPGHOME; None (after printing a skip reason) when gpg is unavailable.
fn gpg_home() -> Option<&'static std::path::Path> {
    static HOME: OnceLock<Option<tempfile::TempDir>> = OnceLock::new();
    HOME.get_or_init(|| {
        if !program_available("gpg") {
            return None;
        }
        let home = tempfile::Builder::new().prefix("bgpg").tempdir().unwrap();
        std::env::set_var("GNUPGHOME", home.path());
        let status = std::process::Command::new("gpg")
            .args([
                "--batch", "--pinentry-mode", "loopback", "--passphrase", "",
                "--quick-gen-key", UID, "ed25519", "sign", "never",
            ])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "gpg --quick-gen-key failed");
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
    repo.config().unwrap().set_bool("commit.gpgSign", true).unwrap();
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
    let tampered = std::str::from_utf8(&data).unwrap().replace("add a.txt", "add b.txt");
    let forged = repo
        .commit_signed(&tampered, std::str::from_utf8(&signature).unwrap(), None)
        .unwrap();

    assert_eq!(verify_commit(&repo, &forged.to_string()).unwrap().status, SignatureStatus::Bad);
}

#[test]
fn signature_from_a_key_not_in_the_keyring_is_unknown_key() {
    require_gpg!();
    let (dir, repo) = signing_repo();
    // A second, separate keyring signs the buffer; the process keyring has never seen that key.
    let other = tempfile::Builder::new().prefix("bgpg2").tempdir().unwrap();
    let gen = std::process::Command::new("gpg")
        .arg("--homedir").arg(other.path())
        .args(["--batch", "--pinentry-mode", "loopback", "--passphrase", "",
               "--quick-gen-key", "Stranger <stranger@example.com>", "ed25519", "sign", "never"])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(gen.success());
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    let buffer = repo
        .commit_create_buffer(&sig, &sig, "stranger", &base.tree().unwrap(), &[&base])
        .unwrap();
    let mut child = std::process::Command::new("gpg")
        .arg("--homedir").arg(other.path())
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
    repo.config().unwrap().set_str("gpg.program", "browsitory-no-such-gpg").unwrap();

    let result = verify_commit(&repo, &id).unwrap();

    assert_eq!(result.status, SignatureStatus::Error);
    assert_eq!(result.detail.as_deref(), Some("browsitory-no-such-gpg was not found"));
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p git-core --test signing_ssh --test signing_gpg`
Expected: FAIL to compile (`verify_commit` not found).

- [ ] **Step 4: Implement verification**

Change the import line to `use git2::{Commit, ErrorCode, Oid, Repository, Signature, Tree};`, then append to `crates/git-core/src/signing.rs`:

```rust
fn is_not_found(error: &git2::Error) -> bool {
    error.code() == ErrorCode::NotFound
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureStatus {
    Unsigned,
    Good,
    GoodUntrusted,
    Bad,
    UnknownKey,
    Expired,
    ExpiredKey,
    RevokedKey,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSignature {
    pub status: SignatureStatus,
    pub format: Option<SignatureFormat>,
    pub signer: Option<String>,
    pub key: Option<String>,
    pub detail: Option<String>,
}

impl CommitSignature {
    fn new(status: SignatureStatus, format: Option<SignatureFormat>) -> Self {
        Self { status, format, signer: None, key: None, detail: None }
    }

    fn error(format: Option<SignatureFormat>, detail: impl Into<String>) -> Self {
        Self { detail: Some(detail.into()), ..Self::new(SignatureStatus::Error, format) }
    }
}

/// Signature status of `commit_id`. Only repository-level failures (bad id, unreadable object)
/// are `Err`; a missing or failing verifier program is an `Error` status.
pub fn verify_commit(repo: &Repository, commit_id: &str) -> Result<CommitSignature, SignError> {
    let oid = Oid::from_str(commit_id)?;
    repo.find_commit(oid)?;
    let (signature, data) = match repo.extract_signature(&oid, None) {
        Ok(pair) => pair,
        Err(error) if is_not_found(&error) => {
            return Ok(CommitSignature::new(SignatureStatus::Unsigned, None))
        }
        Err(error) => return Err(error.into()),
    };
    let signature = String::from_utf8_lossy(&signature).into_owned();
    let config = repo.config()?;
    let format = if signature.starts_with("-----BEGIN PGP SIGNATURE-----") {
        SignatureFormat::OpenPgp
    } else if signature.starts_with("-----BEGIN SSH SIGNATURE-----") {
        SignatureFormat::Ssh
    } else {
        return Ok(CommitSignature::error(None, "unsupported signature type"));
    };
    let program = program_for(&config, format);
    let mut sig_file = tempfile::NamedTempFile::new()?;
    sig_file.write_all(signature.as_bytes())?;
    sig_file.flush()?;
    let sig_path = sig_file.path().to_string_lossy().into_owned();
    let result = match format {
        SignatureFormat::OpenPgp => verify_gpg(&program, &sig_path, &data),
        SignatureFormat::Ssh => {
            let allowed = config_string(&config, "gpg.ssh.allowedSignersFile")
                .map(|path| expand_tilde(&path).to_string_lossy().into_owned());
            verify_ssh(&program, &sig_path, allowed.as_deref(), &data)
        }
    };
    match result {
        Ok(signature) => Ok(signature),
        Err(SignError::SignerNotFound(program)) => Ok(CommitSignature::error(
            Some(format),
            format!("{program} was not found"),
        )),
        Err(SignError::Io(error)) => Ok(CommitSignature::error(Some(format), error.to_string())),
        Err(other) => Err(other),
    }
}

fn verify_gpg(program: &str, sig_path: &str, data: &[u8]) -> Result<CommitSignature, SignError> {
    let output = run_program(program, &["--status-fd=1", "--verify", sig_path, "-"], data)?;
    Ok(parse_gpg_status(&output.stdout, &output.stderr))
}

/// Maps gpg `--status-fd` lines to a status, like git's `parse_gpg_output`.
pub(crate) fn parse_gpg_status(status: &str, stderr: &str) -> CommitSignature {
    let mut result = CommitSignature::new(SignatureStatus::Error, Some(SignatureFormat::OpenPgp));
    let mut trusted = false;
    let mut decided = false;
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("[GNUPG:] ") else { continue };
        let mut parts = rest.splitn(3, ' ');
        let keyword = parts.next().unwrap_or_default();
        let first = parts.next().map(str::to_string);
        let remainder = parts.next().map(str::to_string);
        let set = |result: &mut CommitSignature, status: SignatureStatus| {
            result.status = status;
            result.key = result.key.clone().or(first.clone());
            result.signer = remainder.clone();
        };
        match keyword {
            "GOODSIG" => { set(&mut result, SignatureStatus::Good); decided = true; }
            "BADSIG" => { set(&mut result, SignatureStatus::Bad); decided = true; }
            "EXPSIG" => { set(&mut result, SignatureStatus::Expired); decided = true; }
            "EXPKEYSIG" => { set(&mut result, SignatureStatus::ExpiredKey); decided = true; }
            "REVKEYSIG" => { set(&mut result, SignatureStatus::RevokedKey); decided = true; }
            "ERRSIG" if !decided => {
                result.key = first.clone();
            }
            "NO_PUBKEY" if !decided => {
                result.status = SignatureStatus::UnknownKey;
                result.key = result.key.clone().or(first.clone());
                decided = true;
            }
            // VALIDSIG <fingerprint> …: the full fingerprint beats GOODSIG's long key id.
            "VALIDSIG" => result.key = first.clone(),
            "TRUST_FULLY" | "TRUST_ULTIMATE" => trusted = true,
            _ => {}
        }
    }
    if result.status == SignatureStatus::Good && !trusted {
        result.status = SignatureStatus::GoodUntrusted;
        result.detail = Some("the signing key is not trusted in your keyring".to_string());
    }
    if !decided {
        result.detail = Some(stderr_tail(stderr));
    }
    result
}

fn ssh_fingerprint(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|word| word.starts_with("SHA256:"))
        .map(str::to_string)
}

fn verify_ssh(
    program: &str,
    sig_path: &str,
    allowed: Option<&str>,
    data: &[u8],
) -> Result<CommitSignature, SignError> {
    let format = Some(SignatureFormat::Ssh);
    if let Some(allowed) = allowed {
        let found = run_program(
            program,
            &["-Y", "find-principals", "-f", allowed, "-s", sig_path],
            &[],
        )?;
        let principal = found.stdout.lines().next().map(str::trim).unwrap_or_default();
        if found.status_success && !principal.is_empty() {
            let verified = run_program(
                program,
                &["-Y", "verify", "-n", "git", "-f", allowed, "-I", principal, "-s", sig_path],
                data,
            )?;
            let text = format!("{}\n{}", verified.stdout, verified.stderr);
            let mut result = CommitSignature::new(
                if verified.status_success { SignatureStatus::Good } else { SignatureStatus::Bad },
                format,
            );
            result.signer = Some(principal.to_string());
            result.key = ssh_fingerprint(&text);
            return Ok(result);
        }
    }
    let checked = run_program(program, &["-Y", "check-novalidate", "-n", "git", "-s", sig_path], data)?;
    let text = format!("{}\n{}", checked.stdout, checked.stderr);
    let mut result = if checked.status_success {
        CommitSignature {
            detail: Some("signer is not in gpg.ssh.allowedSignersFile".to_string()),
            ..CommitSignature::new(SignatureStatus::GoodUntrusted, format)
        }
    } else {
        CommitSignature::new(SignatureStatus::Bad, format)
    };
    result.key = ssh_fingerprint(&text);
    Ok(result)
}
```

If `ssh-keygen -Y check-novalidate` on a tampered buffer prints its fingerprint only on success, `key` stays `None` for `Bad`. That is fine.

- [ ] **Step 5: Add a unit test for the gpg status parser**

At the bottom of `signing.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpg_status_mapping() {
        let good = "[GNUPG:] GOODSIG AAAA Test User <t@e>\n[GNUPG:] VALIDSIG FPR123 2026-10-20\n[GNUPG:] TRUST_ULTIMATE 0 pgp\n";
        let parsed = parse_gpg_status(good, "");
        assert_eq!(parsed.status, SignatureStatus::Good);
        assert_eq!(parsed.signer.as_deref(), Some("Test User <t@e>"));
        assert_eq!(parsed.key.as_deref(), Some("FPR123"));

        let untrusted = "[GNUPG:] GOODSIG AAAA U <u@e>\n[GNUPG:] TRUST_UNDEFINED 0 pgp\n";
        assert_eq!(parse_gpg_status(untrusted, "").status, SignatureStatus::GoodUntrusted);

        let unknown = "[GNUPG:] ERRSIG BBBB 22 10 00 1700000000 9 -\n[GNUPG:] NO_PUBKEY BBBB\n";
        let parsed = parse_gpg_status(unknown, "");
        assert_eq!(parsed.status, SignatureStatus::UnknownKey);
        assert_eq!(parsed.key.as_deref(), Some("BBBB"));

        for (line, status) in [
            ("[GNUPG:] BADSIG A U <u@e>", SignatureStatus::Bad),
            ("[GNUPG:] EXPSIG A U <u@e>", SignatureStatus::Expired),
            ("[GNUPG:] EXPKEYSIG A U <u@e>", SignatureStatus::ExpiredKey),
            ("[GNUPG:] REVKEYSIG A U <u@e>", SignatureStatus::RevokedKey),
        ] {
            assert_eq!(parse_gpg_status(line, "").status, status, "{line}");
        }

        let nothing = parse_gpg_status("", "gpg: no valid OpenPGP data found.\n");
        assert_eq!(nothing.status, SignatureStatus::Error);
        assert_eq!(nothing.detail.as_deref(), Some("gpg: no valid OpenPGP data found."));
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p git-core --lib signing && cargo test -p git-core --test signing_ssh --test signing_gpg`
Expected: PASS.

- [ ] **Step 7: Lint and commit**

Run: `cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core
git commit -m "feat(git-core): verify commit signatures (gpg and ssh)"
```

---

### Task 4: `repo-service` `GetCommitSignature`

**Files:**
- Modify: `crates/repo-service/src/worker/mod.rs` (Command enum near line 98, dispatch near line 607, tests), `crates/repo-service/src/worker/status.rs` (handler after `get_commit_message` at line 58, handle method after line 360)

**Interfaces:**
- Consumes: `git_core::signing::{verify_commit, CommitSignature}` (Task 3).
- Produces: `WorkerHandle::get_commit_signature(&self, commit_id: String) -> Result<git_core::signing::CommitSignature, String>`.

- [ ] **Step 1: Write the failing worker tests**

In `crates/repo-service/src/worker/mod.rs`'s test module, after `stage_then_commit_round_trips_through_the_worker`:

```rust
    #[test]
    fn get_commit_signature_reports_unsigned_for_a_plain_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "plain");
        let id = repo.head().unwrap().target().unwrap().to_string();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let signature = worker.handle().get_commit_signature(id).unwrap();

        assert_eq!(signature.status, git_core::signing::SignatureStatus::Unsigned);
    }

    #[test]
    fn failed_signing_replies_err_and_records_no_undo_entry() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "base");
        {
            let mut config = repo.config().unwrap();
            config.set_bool("commit.gpgSign", true).unwrap();
            config.set_str("gpg.program", "browsitory-no-such-gpg").unwrap();
        }
        write_file(dir.path(), "b.txt", "b");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("b.txt".into()).unwrap();
        let undo_before = handle.get_undo_state().unwrap().undo;
        let result = handle.commit("signed?".into());

        assert_eq!(
            result.unwrap_err(),
            "Signing program 'browsitory-no-such-gpg' was not found."
        );
        assert_eq!(handle.get_undo_state().unwrap().undo, undo_before);
    }
```

The worker test module's `init_repo` must pin `commit.gpgSign=false` like git-core's (a developer's global `commit.gpgSign=true` would otherwise break every worker test). Add `config.set_bool("commit.gpgSign", false).unwrap();` to it.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service get_commit_signature failed_signing`
Expected: FAIL to compile (`get_commit_signature` missing).

- [ ] **Step 3: Implement**

`mod.rs` `Command` enum, after `GetCommitMessage`:

```rust
    GetCommitSignature {
        commit_id: String,
        reply: Sender<Result<git_core::signing::CommitSignature, String>>,
    },
```

Dispatch, after the `GetCommitMessage` arm:

```rust
                    Command::GetCommitSignature { commit_id, reply } => {
                        status::get_commit_signature(&repo, commit_id, reply)
                    }
```

If the compiler reports a non-exhaustive match elsewhere (e.g. undo's `plan_for(&Command)`), add `Command::GetCommitSignature { .. }` to the not-recorded / read-only arm there.

`status.rs`, after `get_commit_message`:

```rust
pub(super) fn get_commit_signature(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<git_core::signing::CommitSignature, String>>,
) {
    let _ = reply.send(
        git_core::signing::verify_commit(repo, &commit_id).map_err(|error| error.to_string()),
    );
}
```

`impl WorkerHandle`, after `get_commit_message`:

```rust
    pub fn get_commit_signature(
        &self,
        commit_id: String,
    ) -> Result<git_core::signing::CommitSignature, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitSignature {
                commit_id,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p repo-service`
Expected: PASS (both new tests plus the existing suite).

- [ ] **Step 5: Commit**

```bash
git add crates/repo-service
git commit -m "feat(repo-service): GetCommitSignature worker command"
```

---

### Task 5: Tauri command and sidecar method

**Files:**
- Modify: `crates/tauri-app/src/commands/mod.rs` (DTO near `UndoStateDto` at line 539, re-export list at line 47, test near line 1577), `crates/tauri-app/src/commands/status.rs` (after `get_commit_message` at line 107), `crates/tauri-app/src/main.rs:12,75`
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (method table line 87, function after line 653, DTO near `UndoStateDto` at line 1715), `crates/vscode-sidecar/tests/protocol_roundtrip.rs` (after the `get_commit_message` call at line 514)

**Interfaces:**
- Consumes: `WorkerHandle::get_commit_signature` (Task 4).
- Produces: wire method `get_commit_signature` with params `{ repoPath, commitId }` returning `{ status, format, signer, key, detail }`, enum strings per Global Constraints.

- [ ] **Step 1: Write the failing Tauri DTO test**

In `crates/tauri-app/src/commands/mod.rs`'s test module, next to `undo_dtos_serialize_for_the_frontend`:

```rust
    #[test]
    fn commit_signature_dto_serializes_for_the_frontend() {
        use git_core::signing::{CommitSignature, SignatureFormat, SignatureStatus};
        let dto = CommitSignatureDto::from(CommitSignature {
            status: SignatureStatus::GoodUntrusted,
            format: Some(SignatureFormat::OpenPgp),
            signer: Some("Jane <j@e>".into()),
            key: Some("ABCD".into()),
            detail: None,
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({
                "status": "goodUntrusted", "format": "openpgp",
                "signer": "Jane <j@e>", "key": "ABCD", "detail": null
            })
        );
        let unsigned = CommitSignatureDto::from(CommitSignature {
            status: SignatureStatus::Unsigned,
            format: None,
            signer: None,
            key: None,
            detail: None,
        });
        assert_eq!(serde_json::to_value(unsigned).unwrap()["status"], "unsigned");
        for (status, wire) in [
            (SignatureStatus::Good, "good"),
            (SignatureStatus::Bad, "bad"),
            (SignatureStatus::UnknownKey, "unknownKey"),
            (SignatureStatus::Expired, "expired"),
            (SignatureStatus::ExpiredKey, "expiredKey"),
            (SignatureStatus::RevokedKey, "revokedKey"),
            (SignatureStatus::Error, "error"),
        ] {
            let dto = CommitSignatureDto::from(CommitSignature {
                status, format: Some(SignatureFormat::Ssh), signer: None, key: None, detail: None,
            });
            let value = serde_json::to_value(dto).unwrap();
            assert_eq!(value["status"], wire);
            assert_eq!(value["format"], "ssh");
        }
    }
```

`tauri-app` and `vscode-sidecar` both depend on `git-core` directly, so import `git_core::signing::…`.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p tauri-app commit_signature_dto`
Expected: FAIL to compile (`CommitSignatureDto` missing).

- [ ] **Step 3: Implement the Tauri DTO and command**

`crates/tauri-app/src/commands/mod.rs`, after `UndoStateDto`'s `impl`:

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SignatureStatusDto {
    Unsigned,
    Good,
    GoodUntrusted,
    Bad,
    UnknownKey,
    Expired,
    ExpiredKey,
    RevokedKey,
    Error,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SignatureFormatDto {
    Openpgp,
    Ssh,
}

#[derive(Serialize)]
pub struct CommitSignatureDto {
    status: SignatureStatusDto,
    format: Option<SignatureFormatDto>,
    signer: Option<String>,
    key: Option<String>,
    detail: Option<String>,
}

impl From<git_core::signing::CommitSignature> for CommitSignatureDto {
    fn from(signature: git_core::signing::CommitSignature) -> Self {
        use git_core::signing::{SignatureFormat, SignatureStatus};
        Self {
            status: match signature.status {
                SignatureStatus::Unsigned => SignatureStatusDto::Unsigned,
                SignatureStatus::Good => SignatureStatusDto::Good,
                SignatureStatus::GoodUntrusted => SignatureStatusDto::GoodUntrusted,
                SignatureStatus::Bad => SignatureStatusDto::Bad,
                SignatureStatus::UnknownKey => SignatureStatusDto::UnknownKey,
                SignatureStatus::Expired => SignatureStatusDto::Expired,
                SignatureStatus::ExpiredKey => SignatureStatusDto::ExpiredKey,
                SignatureStatus::RevokedKey => SignatureStatusDto::RevokedKey,
                SignatureStatus::Error => SignatureStatusDto::Error,
            },
            format: signature.format.map(|format| match format {
                SignatureFormat::OpenPgp => SignatureFormatDto::Openpgp,
                SignatureFormat::Ssh => SignatureFormatDto::Ssh,
            }),
            signer: signature.signer,
            key: signature.key,
            detail: signature.detail,
        }
    }
}
```

`crates/tauri-app/src/commands/status.rs`, after `get_commit_message` (add `CommitSignatureDto` to that file's `use super::{…}`):

```rust
#[tauri::command]
pub async fn get_commit_signature(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<CommitSignatureDto, String> {
    Ok(CommitSignatureDto::from(
        worker_handle(&state, &repo_path)?.get_commit_signature(commit_id)?,
    ))
}
```

Add `get_commit_signature` to the `pub use status::{…}` list in `commands/mod.rs` (line 47 area) and to both the `use` list (line 12) and `tauri::generate_handler![…]` (line 75 area) in `main.rs`, next to `get_commit_message`.

- [ ] **Step 4: Write the failing sidecar roundtrip assertion**

In `crates/vscode-sidecar/tests/protocol_roundtrip.rs`, right after the `get_commit_message` assertion at line 519 (pick an id not used elsewhere in that test; check the file and use the next free one, shown here as `60`):

```rust
    let signature = sidecar.call(
        60,
        "get_commit_signature",
        serde_json::json!({"repoPath": repo_path, "commitId": commit_id}),
    );
    assert_eq!(
        signature["result"],
        serde_json::json!({
            "status": "unsigned", "format": null, "signer": null, "key": null, "detail": null
        })
    );
```

If the roundtrip test's repo setup does not pin `commit.gpgSign=false`, add it where that test sets `user.name`.

- [ ] **Step 5: Implement the sidecar method**

`dispatch.rs` method table, after `"get_commit_message" => …`:

```rust
        "get_commit_signature" => get_commit_signature(params, repos),
```

After `fn get_commit_message`:

```rust
fn get_commit_signature(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: GetCommitFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let signature =
        worker_handle(repos, &params.repo_path)?.get_commit_signature(params.commit_id)?;
    serde_json::to_value(CommitSignatureDto::from(signature)).map_err(|error| error.to_string())
}
```

Next to `UndoStateDto`, add the same `SignatureStatusDto`, `SignatureFormatDto`, `CommitSignatureDto` and `From` impl as in Step 3 (private, without `pub`). The sidecar keeps its own DTO copies by convention, the same way `UndoStateDto` is duplicated.

- [ ] **Step 6: Run tests**

Run: `cargo test -p tauri-app commit_signature_dto && cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 7: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat: expose get_commit_signature over Tauri and the VSCode sidecar"
```

---

### Task 6: `RepoClient.getCommitSignature` in both clients

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts` (types near `RebasePlanEntry` at line 240, method after `getCommitMessage` at line 340), `frontend/src/ipc/tauriRepoClient.ts:124`, `frontend/src/ipc/vscodeRepoClient.ts:233`
- Test: `frontend/src/ipc/tauriRepoClient.test.ts`, `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: wire method `get_commit_signature` (Task 5).
- Produces (used by Task 7):

```ts
export type SignatureStatus = "unsigned" | "good" | "goodUntrusted" | "bad" | "unknownKey" | "expired" | "expiredKey" | "revokedKey" | "error";
export interface CommitSignature { status: SignatureStatus; format: "openpgp" | "ssh" | null; signer: string | null; key: string | null; detail: string | null; }
getCommitSignature(repoPath: string, commitId: string): Promise<CommitSignature>;
```

- [ ] **Step 1: Write the failing client tests**

`tauriRepoClient.test.ts`, in the test that contains `getUndoState` (around line 307), append:

```ts
    await tauriRepoClient.getCommitSignature("/repo", "abc");
    expect(invoke).toHaveBeenLastCalledWith("get_commit_signature", { repoPath: "/repo", commitId: "abc" });
```

`vscodeRepoClient.test.ts`, new test after "wires the undo commands":

```ts
  it("wires getCommitSignature", async () => {
    const promise = vscodeRepoClient.getCommitSignature("/repo", "abc");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_signature",
      params: { repoPath: "/repo", commitId: "abc" },
    });
    const result = { status: "good", format: "ssh", signer: "me", key: "SHA256:x", detail: null };
    respond(1, result);
    await expect(promise).resolves.toEqual(result);
  });
```

Match the id the neighbouring tests use for their first call (they reset the counter in `beforeEach`; if not, use the next id the file's pattern dictates).

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL (`getCommitSignature is not a function`).

- [ ] **Step 3: Implement**

`RepoClient.ts`, near the other commit types:

```ts
export type SignatureStatus =
  | "unsigned"
  | "good"
  | "goodUntrusted"
  | "bad"
  | "unknownKey"
  | "expired"
  | "expiredKey"
  | "revokedKey"
  | "error";

export interface CommitSignature {
  status: SignatureStatus;
  format: "openpgp" | "ssh" | null;
  signer: string | null;
  key: string | null;
  detail: string | null;
}
```

In the `RepoClient` interface, after `getCommitMessage`:

```ts
  getCommitSignature(repoPath: string, commitId: string): Promise<CommitSignature>;
```

`tauriRepoClient.ts`, after `getCommitMessage`:

```ts
  getCommitSignature: (repoPath: string, commitId: string) =>
    loggedInvoke<CommitSignature>("get_commit_signature", { repoPath, commitId }),
```

`vscodeRepoClient.ts`, after `getCommitMessage`:

```ts
  getCommitSignature: (repoPath: string, commitId: string) =>
    call<CommitSignature>("get_commit_signature", { repoPath, commitId }),
```

Add `CommitSignature` to each file's type import from `./RepoClient`.

- [ ] **Step 4: Fix full `RepoClient` test fixtures**

Run: `cd frontend && pnpm exec tsc --noEmit -p .`
For every object literal typed `: RepoClient` that now errors with "Property 'getCommitSignature' is missing" (e.g. `src/state/useAppState.test.ts:1120,1198,1279` and `src/App.test.tsx`'s base client near line 107), add:

```ts
    getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
```

Run `tsc` again until it is clean.

- [ ] **Step 5: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient.getCommitSignature in both clients"
```

---

### Task 7: Signature row in `CommitHeader`

**Files:**
- Create: `frontend/src/lib/signatureStatus.ts`, `frontend/src/lib/signatureStatus.test.ts`
- Modify: `frontend/src/components/CommitHeader.tsx`, `CommitHeader.module.css`
- Test: `frontend/src/components/CommitHeader.test.tsx`

**Interfaces:**
- Consumes: `CommitSignature`, `SignatureStatus`, `RepoClient.getCommitSignature` (Task 6).
- Produces: `signatureLabel(status: Exclude<SignatureStatus, "unsigned">): { tone: "success" | "warning" | "danger" | "neutral"; text: string }`, `shortKey(key: string): string`.

- [ ] **Step 1: Write the failing helper tests**

`frontend/src/lib/signatureStatus.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { shortKey, signatureLabel } from "./signatureStatus";

describe("signatureLabel", () => {
  it.each([
    ["good", "success", "Good signature"],
    ["goodUntrusted", "warning", "Valid signature, signer not trusted"],
    ["unknownKey", "warning", "Signed with an unknown key"],
    ["expired", "warning", "Signature expired"],
    ["expiredKey", "warning", "Signed with an expired key"],
    ["bad", "danger", "Bad signature"],
    ["revokedKey", "danger", "Signed with a revoked key"],
    ["error", "neutral", "Signature could not be checked"],
  ] as const)("%s", (status, tone, text) => {
    expect(signatureLabel(status)).toEqual({ tone, text });
  });
});

describe("shortKey", () => {
  it("keeps the last 16 hex chars of a gpg fingerprint", () => {
    expect(shortKey("0123456789ABCDEF0123456789ABCDEF01234567")).toBe("89ABCDEF01234567");
  });
  it("keeps SHA256: plus 12 chars of an ssh fingerprint", () => {
    expect(shortKey("SHA256:abcdefghijklmnopqrstuvwxyz")).toBe("SHA256:abcdefghijkl");
  });
  it("leaves short keys alone", () => {
    expect(shortKey("ABCD")).toBe("ABCD");
  });
});
```

- [ ] **Step 2: Write the failing component tests**

In `CommitHeader.test.tsx`, replace `clientWithMessage` with a client that also answers `getCommitSignature`, and add tests:

```tsx
import type { CommitSignature } from "../ipc/RepoClient";

const UNSIGNED: CommitSignature = { status: "unsigned", format: null, signer: null, key: null, detail: null };

function clientWithMessage(
  message: string,
  signature: CommitSignature | Promise<CommitSignature> | Error = UNSIGNED,
): RepoClient {
  return {
    getCommitMessage: vi.fn(async () => message),
    getCommitSignature: vi.fn(async () => {
      if (signature instanceof Error) throw signature;
      return signature;
    }),
  } as unknown as RepoClient;
}
```

Tests:

```tsx
  describe("signature row", () => {
    it("is hidden for an unsigned commit", async () => {
      renderHeader();
      await screen.findByText(FULL_ID);
      await waitFor(() => expect(screen.queryByText("Checking signature…")).not.toBeInTheDocument());
      expect(screen.queryByText("Signature")).not.toBeInTheDocument();
    });

    it("shows a good signature with signer and short key", async () => {
      renderHeader({
        client: clientWithMessage("Fix the thing\n", {
          status: "good",
          format: "openpgp",
          signer: "Jane Doe <jane@example.com>",
          key: "0123456789ABCDEF0123456789ABCDEF01234567",
          detail: null,
        }),
      });
      expect(await screen.findByText("Good signature")).toBeInTheDocument();
      expect(screen.getByText("Signature")).toBeInTheDocument();
      expect(screen.getByText(/Jane Doe <jane@example.com>/, { selector: "span" })).toBeInTheDocument();
      expect(screen.getByText("89ABCDEF01234567")).toHaveAttribute(
        "title",
        "0123456789ABCDEF0123456789ABCDEF01234567",
      );
    });

    it("shows the untrusted warning with the detail as a tooltip", async () => {
      renderHeader({
        client: clientWithMessage("Fix the thing\n", {
          status: "goodUntrusted",
          format: "ssh",
          signer: null,
          key: "SHA256:abcdefghijklmnop",
          detail: "signer is not in gpg.ssh.allowedSignersFile",
        }),
      });
      const pill = await screen.findByText("Valid signature, signer not trusted");
      expect(pill.closest("[data-tone]")).toHaveAttribute("data-tone", "warning");
      expect(pill.closest("[title]")).toHaveAttribute("title", "signer is not in gpg.ssh.allowedSignersFile");
    });

    it("marks a bad signature as danger", async () => {
      renderHeader({
        client: clientWithMessage("Fix the thing\n", {
          status: "bad", format: "openpgp", signer: null, key: null, detail: null,
        }),
      });
      const pill = await screen.findByText("Bad signature");
      expect(pill.closest("[data-tone]")).toHaveAttribute("data-tone", "danger");
    });

    it("shows the error status text", async () => {
      renderHeader({
        client: clientWithMessage("Fix the thing\n", {
          status: "error", format: "openpgp", signer: null, key: null, detail: "gpg was not found",
        }),
      });
      expect(await screen.findByText("Signature could not be checked")).toBeInTheDocument();
    });

    it("shows a loading text while checking", async () => {
      renderHeader({ client: clientWithMessage("Fix the thing\n", new Promise(() => {})) });
      expect(await screen.findByText("Checking signature…")).toBeInTheDocument();
    });

    it("hides the row when the fetch fails", async () => {
      renderHeader({ client: clientWithMessage("Fix the thing\n", new Error("boom")) });
      await waitFor(() => expect(screen.queryByText("Checking signature…")).not.toBeInTheDocument());
      expect(screen.queryByText("Signature")).not.toBeInTheDocument();
    });
  });
```

`vi.fn(async () => …)` returning a never-resolving promise: `async () => signature` with `signature` a pending `Promise` resolves to it, so the loading test works as written. If TypeScript complains, widen the parameter type as shown.

- [ ] **Step 3: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/signatureStatus src/components/CommitHeader`
Expected: FAIL (module missing; no "Good signature").

- [ ] **Step 4: Implement the helper**

`frontend/src/lib/signatureStatus.ts`:

```ts
import type { SignatureStatus } from "../ipc/RepoClient";

export type SignatureTone = "success" | "warning" | "danger" | "neutral";

const LABELS: Record<Exclude<SignatureStatus, "unsigned">, { tone: SignatureTone; text: string }> = {
  good: { tone: "success", text: "Good signature" },
  goodUntrusted: { tone: "warning", text: "Valid signature, signer not trusted" },
  unknownKey: { tone: "warning", text: "Signed with an unknown key" },
  expired: { tone: "warning", text: "Signature expired" },
  expiredKey: { tone: "warning", text: "Signed with an expired key" },
  bad: { tone: "danger", text: "Bad signature" },
  revokedKey: { tone: "danger", text: "Signed with a revoked key" },
  error: { tone: "neutral", text: "Signature could not be checked" },
};

export function signatureLabel(status: Exclude<SignatureStatus, "unsigned">) {
  return LABELS[status];
}

/** gpg fingerprints: last 16 hex chars (the long key id). ssh: `SHA256:` + first 12 chars. */
export function shortKey(key: string): string {
  if (key.startsWith("SHA256:")) return key.slice(0, "SHA256:".length + 12);
  return key.length > 16 ? key.slice(-16) : key;
}
```

- [ ] **Step 5: Implement the row**

`CommitHeader.tsx`, imports:

```tsx
import type { CommitSignature, GraphCommit, RepoClient } from "../ipc/RepoClient";
import { shortKey, signatureLabel } from "../lib/signatureStatus";
```

State and effect. Add next to the message state, and extend the existing effect so one effect fetches both (same `ignore` flag):

```tsx
  // undefined = still checking, null = unsigned or the check failed (row hidden).
  const [signature, setSignature] = useState<CommitSignature | null | undefined>(undefined);
```

Inside the existing `useEffect`, after the message chain and before `return () => {…}`:

```tsx
    setSignature(undefined);
    Promise.resolve()
      .then(() => client.getCommitSignature(repoPath, commitId))
      .then((next) => {
        if (!ignore) setSignature(next.status === "unsigned" ? null : next);
      })
      .catch(() => {
        // Like the body: the header is complete without it.
        if (!ignore) setSignature(null);
      });
```

Update the component doc comment's last sentence: "only the full message body and the signature status need a `RepoClient` round trip."

Above the `return`, derive the label (the `!== "unsigned"` check narrows the type, so no cast):

```tsx
  const label =
    signature != null && signature.status !== "unsigned" ? signatureLabel(signature.status) : null;
```

Render after the `commit !== undefined` fragment (Author/Date), before `<dt>SHA</dt>`:

```tsx
        {signature === undefined && (
          <>
            <dt>Signature</dt>
            <dd className={styles.relative}>Checking signature…</dd>
          </>
        )}
        {signature != null && label !== null && (
          <>
            <dt>Signature</dt>
            <dd>
              <span className={styles.signature} data-tone={label.tone} title={signature.detail ?? undefined}>
                <span aria-hidden="true" className={styles.signatureIcon} />
                {label.text}
              </span>
              {signature.signer !== null && <span>{signature.signer}</span>}
              {signature.key !== null && (
                <code className={styles.sha} title={signature.key}>
                  {shortKey(signature.key)}
                </code>
              )}
            </dd>
          </>
        )}
```

The "Checking signature…" row briefly showing for unsigned commits is intended and matches the spec. It disappears as soon as the (process-free) unsigned answer arrives.

`CommitHeader.module.css`, append:

```css
.signature {
  display: inline-flex;
  align-items: center;
  gap: var(--space-1);
  padding: 0 var(--space-2);
  border-radius: 999px;
  font-size: var(--text-sm);
  background: var(--color-bg-subtle);
  color: var(--color-text);
}

.signature[data-tone="success"] {
  background: var(--color-success-bg);
  color: var(--color-success-text);
}

.signature[data-tone="warning"] {
  background: var(--color-warning-bg);
  color: var(--color-warning-text);
}

.signature[data-tone="danger"] {
  background: var(--color-danger-bg);
  color: var(--color-danger-text);
}

/* Shape carries the tone too, so colour is never the only signal. */
.signatureIcon::before {
  content: "?";
}

.signature[data-tone="success"] .signatureIcon::before {
  content: "✓";
}

.signature[data-tone="warning"] .signatureIcon::before {
  content: "!";
}

.signature[data-tone="danger"] .signatureIcon::before {
  content: "✗";
}
```

All tokens used here exist in `frontend/src/styles/tokens.css` (light and dark).

- [ ] **Step 6: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS. Existing `App.test.tsx`/`DiffPane.test.tsx` mocks that lack `getCommitSignature` hit the `.catch` and hide the row, so they must not need changes beyond Task 6's typed fixtures.

- [ ] **Step 7: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): show commit signature status in the commit header"
```

---

### Task 8: E2E flows

**Files:**
- Create: `e2e/specs/commit-signing.spec.ts`, `extension/e2e/src/specs/commit-signing.spec.ts`

**Interfaces:**
- Consumes: the full stack (Tasks 1–7). Fixture repos: `${os.tmpdir()}/browsitory-e2e-repo` (Tauri) and the VSCode e2e workspace folder.

Both specs turn signing on in the shared fixture repo's local config and **must** turn it off in `after`/`finally`. Later specs commit too.

- [ ] **Step 1: Write the Tauri spec**

`e2e/specs/commit-signing.spec.ts`:

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "signing-e2e.txt";
const SUMMARY = "e2e: signed commit";

function git(...args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();
}

describe("Browsitory commit signing", () => {
  let keyDir = "";

  before(function () {
    try {
      execFileSync("ssh-keygen", ["-?"], { stdio: "ignore" });
    } catch (error) {
      // ssh-keygen -? exits non-zero but exists; only ENOENT means it's missing.
      if ((error as NodeJS.ErrnoException).code === "ENOENT") this.skip();
    }
    keyDir = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-e2e-ssh-"));
    const key = path.join(keyDir, "id_e2e");
    execFileSync("ssh-keygen", ["-q", "-t", "ed25519", "-N", "", "-C", "e2e@example.com", "-f", key]);
    const pub = fs.readFileSync(`${key}.pub`, "utf8").trim();
    const email = git("config", "user.email");
    fs.writeFileSync(path.join(keyDir, "allowed_signers"), `${email} ${pub}\n`);
    git("config", "commit.gpgSign", "true");
    git("config", "gpg.format", "ssh");
    git("config", "user.signingKey", key);
    git("config", "gpg.ssh.allowedSignersFile", path.join(keyDir, "allowed_signers"));
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "signed\n");
  });

  after(() => {
    for (const key of ["commit.gpgSign", "gpg.format", "user.signingKey", "gpg.ssh.allowedSignersFile"]) {
      try {
        git("config", "--unset", key);
      } catch {
        // already unset
      }
    }
    if (keyDir) fs.rmSync(keyDir, { recursive: true, force: true });
  });

  it("commits a signed commit and shows a good signature", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });
    const stageButton = await $(`button[aria-label="Stage ${FILE}"]`);
    await stageButton.waitForExist({ timeout: 10000 });
    // Same opacity-0 workaround as first-flow.spec.ts.
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    await browser.execute(
      (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
        setter?.call(el, value);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      commitMessageInput,
      SUMMARY,
    );
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Commit"));

    await browser.waitUntil(() => git("log", "-1", "--format=%s") === SUMMARY, {
      timeout: 10000,
      timeoutMsg: "commit did not land",
    });
    if (!git("cat-file", "commit", "HEAD").includes("-----BEGIN SSH SIGNATURE-----")) {
      throw new Error("HEAD commit is not ssh-signed");
    }

    const row = await $(`li*=${SUMMARY}`);
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), row);
    await browser.waitUntil(
      async () => (await browser.execute(() => document.body.textContent ?? "")).includes("Good signature"),
      { timeout: 10000, timeoutMsg: "expected 'Good signature' in the commit header" },
    );
  });
});
```

If clicking the `<li>` does not select the commit (the row's click target may be an inner element), use whichever selector `commit-graph.spec.ts` uses to select a commit row.

- [ ] **Step 2: Run the Tauri E2E**

Run (from repo root, per CLAUDE.md order):

```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm test -- --spec specs/commit-signing.spec.ts
```

Expected: PASS. Then run the full `pnpm test` once, to prove the `after` cleanup leaves later specs unaffected.

- [ ] **Step 3: Write the VSCode spec**

`extension/e2e/src/specs/commit-signing.spec.ts`:

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension commit signing", () => {
  it("shows a good signature for a commit signed through the app", async function () {
    this.timeout(120000);
    try {
      execFileSync("ssh-keygen", ["-?"], { stdio: "ignore" });
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") this.skip();
    }
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();
    const file = "vscode-signing.txt";
    const summary = "vscode-e2e: signed commit";
    const configKeys = ["commit.gpgSign", "gpg.format", "user.signingKey", "gpg.ssh.allowedSignersFile"];

    const keyDir = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-vscode-ssh-"));
    const key = path.join(keyDir, "id_e2e");
    execFileSync("ssh-keygen", ["-q", "-t", "ed25519", "-N", "", "-f", key]);
    const pub = fs.readFileSync(`${key}.pub`, "utf8").trim();
    fs.writeFileSync(path.join(keyDir, "allowed_signers"), `${git("config", "user.email")} ${pub}\n`);
    git("config", "commit.gpgSign", "true");
    git("config", "gpg.format", "ssh");
    git("config", "user.signingKey", key);
    git("config", "gpg.ssh.allowedSignersFile", path.join(keyDir, "allowed_signers"));

    await vscode.workspace.fs.writeFile(vscode.Uri.joinPath(workspaceFolder.uri, file), Buffer.from("signed\n"));
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);
    const waitUntil = async (done: () => boolean, what: string) => {
      const deadline = Date.now() + 15000;
      while (!done()) {
        if (Date.now() > deadline) throw new Error(what);
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
    };
    try {
      await webview.waitForText("span", `${file} (New)`, 15000);
      await webview.clickByText("button", "Stage all");
      await webview.fill('textarea[aria-label="Commit message"]', summary);
      await webview.clickByText("button", "Commit");
      await waitUntil(() => git("log", "-1", "--format=%s") === summary, "commit did not land");
      await webview.clickByText("li", summary);
      await webview.waitForText("span", "Good signature", 15000);
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      for (const configKey of configKeys) {
        try {
          git("config", "--unset", configKey);
        } catch {
          // already unset
        }
      }
      fs.rmSync(keyDir, { recursive: true, force: true });
    }
  });
});
```

If `webview.clickByText("li", …)` is not a supported helper shape, check `extension/e2e/src/support/webviewPage.ts` for the available methods and use the one `cherry-pick.spec.ts` uses to select a commit row.

- [ ] **Step 4: Run the VSCode E2E**

Run (from repo root, per CLAUDE.md order):

```bash
cargo build --workspace
cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
cd extension && pnpm install && pnpm run compile && cd ..
cd extension/e2e && pnpm install --ignore-workspace && pnpm test
```

Expected: PASS (all specs).

- [ ] **Step 5: Commit**

```bash
git add e2e/specs/commit-signing.spec.ts extension/e2e/src/specs/commit-signing.spec.ts
git commit -m "test(e2e): signed commit shows a good signature in both frontends"
```

---

### Task 9: Docs

**Files:**
- Modify: `CHANGELOG.md`, `docs/USER_GUIDE.md` (new section after "Staging and committing", around line 77–118), `docs/ARCHITECTURE.md`

- [ ] **Step 1: CHANGELOG**

Under the topmost unreleased heading in `CHANGELOG.md` (match the existing heading/bullet style there), add:

```markdown
- Commit signing: when `commit.gpgSign` is set, commits, amends, merges, cherry-picks, reverts and
  rebased commits are signed with gpg (`gpg.format=openpgp`) or SSH (`gpg.format=ssh`), using
  your existing gpg-agent / ssh-agent. If signing fails, no commit is made.
- The commit details pane shows the signature status of signed commits (good, untrusted, unknown
  key, expired, bad, revoked).
```

- [ ] **Step 2: USER_GUIDE**

Insert after the "Discarding and ignoring files" subsection, before "## Browsing history and diffs":

```markdown
### Signing commits

Browsitory signs commits the way `git commit` does, from your git config. It has no setting of
its own:

- `commit.gpgSign = true` turns signing on.
- `gpg.format` picks `openpgp` (default, uses `gpg`) or `ssh` (uses `ssh-keygen`). `x509` is not
  supported.
- `user.signingKey` picks the key. For gpg it defaults to your committer identity. For SSH it is
  required: either a key file path or a literal public key (`key::ssh-ed25519 …`) whose private
  key is in your ssh-agent.
- `gpg.program`, `gpg.openpgp.program` and `gpg.ssh.program` override the programs.

Passphrases come from gpg-agent's pinentry or from ssh-agent. Browsitory cannot prompt in a
terminal. If signing fails, nothing is committed and the signer's error is shown.

Selecting a signed commit shows a **Signature** row in the commit details:

- **Good signature**: valid, from a trusted key (gpg) or a signer listed in
  `gpg.ssh.allowedSignersFile` (SSH).
- **Valid signature, signer not trusted**: cryptographically valid, but the key is not trusted
  or not listed.
- **Signed with an unknown key**: the public key is not in your keyring.
- **Signature expired** / **Signed with an expired key** / **Signed with a revoked key**.
- **Bad signature**: the commit does not match its signature.
- **Signature could not be checked**: for example gpg is not installed.
```

- [ ] **Step 3: ARCHITECTURE**

Add a "Commit signing" subsection next to the "Undo stack" section in `docs/ARCHITECTURE.md`:

```markdown
### Commit signing

- `git-core::signing::create_commit` is the only way `git-core` creates a user-facing commit
  (commit, amend, cherry-pick, revert, rebase step and squash collapse).
  - It writes the object with `repo.commit(None, …)`, or when `commit.gpgSign` is set, with
    `commit_create_buffer` → signer → `commit_signed`.
  - It then moves HEAD with `reference_matching` against the expected current target, writing
    libgit2's own reflog message (`commit: …`, `commit (initial): …`, `commit (merge): …`,
    `commit (amend): …`).
  - The signer runs before any ref moves, so a failure leaves the repo untouched. The undo
    anchor commit deliberately bypasses it.
- Signers are external programs run exactly as git runs them: `gpg --status-fd=2 -bsau <key>`
  and `ssh-keygen -Y sign -n git -f <key>` (`-U` for a literal key), with the buffer on stdin,
  no TTY, and no console window on Windows. They block the repo's worker thread while pinentry
  waits.
- `verify_commit` picks the format from the signature armor, not from config:
  - gpg: `--status-fd` lines map to `SignatureStatus`.
  - ssh: `find-principals` + `verify` against `gpg.ssh.allowedSignersFile`, falling back to
    `check-novalidate` (→ `GoodUntrusted`).
  - Unsigned commits never spawn a process; there is no cache.
- Tests use real `gpg`/`ssh-keygen` with throwaway keys and skip when they are missing.
  `tests/signing_gpg.rs` is its own binary because it sets `GNUPGHOME` once per process.
```

- [ ] **Step 4: Full verification and commit**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```

Expected: all PASS.

```bash
git add CHANGELOG.md docs/USER_GUIDE.md docs/ARCHITECTURE.md
git commit -m "docs: commit signing"
```
