# SM Parity Batch 3a (Clone and New Repository) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clone a remote (progress, cancel, HTTPS token or SSH agent, optional branch, optional submodules) and create a new empty repository from the Tauri desktop app.

**Architecture:** `git-core::clone` wraps `git2::build::RepoBuilder` with the same credential/progress/cancel traits fetch uses; `git-core::repo::init_repo` wraps `Repository::init_opts`. `repo-service::clone::CloneService` (repo-less, not a per-repo worker) owns a cancel registry and the one-shot HTTPS credential, saving it only after success. Tauri exposes `clone_repo` / `cancel_clone` / `init_repo`; the frontend `CloneDialog` drives them through `RepoClient`, VSCode reports the feature unsupported.

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `tauri` 2), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver).

**Spec:** `docs/superpowers/specs/2026-10-17-sm-parity-batch3a-design.md`

## Global Constraints

- Tauri only. `vscodeRepoClient.supportsCloneAndInit = false`; its clone/init methods reject with "Not supported in the VSCode extension". No `vscode-sidecar` changes.
- No new dependencies (Rust or npm). If one becomes unavoidable: license check + `docs/LICENSE_COMPLIANCE.md` row in the same commit.
- HTTPS token: held in memory for the clone, saved to the keychain (`CredentialService::save_https`) and origin profile set to `HttpsToken { username }` **only after a successful clone**. Never logged, never in an error string.
- Clone URL validation: allow https/http/ssh/git/scp-style/`file://`/local paths; reject empty, remote-helper `<transport>::<address>` (a `::` with no `/` before it), and URLs with an embedded password. Submodules keep `update_submodule`'s strict `file://` ban.
- Clone progress uses the existing `transfer-progress` event with `operation: "Clone"`, **Progress events only** — no `Started`/`Completed` events for clone (see Review Focus 1).
- Error transport for `clone_repo` / `init_repo`: `{ kind, message }` object, surfaced in TS as `CloneError` with `.kind`.
- Copy strings (exact): toasts "Cloned ⟨name⟩", "Cloned ⟨name⟩ — N submodules failed to update", "Cloned ⟨name⟩, but the token couldn't be saved.", "Created repository ⟨name⟩"; dialog messages per the table in Task 7.
- Tests: `git-core`/`repo-service` use real temp-dir repos (no mocked `Repository`); frontend mocks `RepoClient` only.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook) — the changelog lands in Task 9; push only after Task 9.

## Review Focus

1. **A repo tab's fetch/push UI must not adopt a clone's progress.** `useRemoteTransferActions` latches onto the first `phase: "Starting"` event while a transfer request is pending; a clone emitting `Starting` would hijack it. Clone emits Progress only — pinned by a `repo-service` test in Task 4 asserting no `Started`/`Completed` event is sent.
2. **Clone into a non-empty or file destination must not touch existing files** — even after failure cleanup. Pinned in Task 1 (`refuses_a_non_empty_destination_and_leaves_it_untouched`, `refuses_a_destination_that_is_a_file`).
3. **Cancel/failure into a pre-existing empty dir keeps the dir** (user picked it) but empties it. Pinned in Task 1 (`failed_clone_into_existing_empty_dir_keeps_the_dir_empty`).
4. **A token pasted into the URL (`https://user:tok@host/...`)** must be refused, not cloned and persisted into `.git/config`. Pinned in Task 1 (`rejects_a_url_with_an_embedded_password`) and its message in Task 7.
5. **Whitespace around the URL / blank branch field** (copy-paste) must clone normally, not fail as "branch '' not found". Pinned in Task 1 (`trims_the_url_and_ignores_a_blank_branch`).

---

### Task 1: `git-core` clone (no submodules)

**Files:**
- Create: `crates/git-core/src/clone.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod clone;` alphabetically after `branch`)
- Modify: `crates/git-core/src/remote.rs:24-30` (add `Clone` to `TransferOperation`)
- Test: `crates/git-core/tests/clone.rs`

**Interfaces:**
- Consumes: `git_core::remote::{CredentialProvider, TransferReporter, TransferProgress, TransferPhase, TransferOperation, TransferErrorKind, RemoteError}` (`RemoteError::transfer_error_kind()` classifies callback errors).
- Produces:
  ```rust
  pub struct CloneOptions { pub url: String, pub dest: PathBuf, pub branch: Option<String>, pub recurse_submodules: bool }
  pub struct CloneOutcome { pub path: PathBuf, pub submodule_failures: Vec<String> }
  pub enum CloneError { InvalidUrl, CredentialBearingUrl, DestNotEmpty, BranchNotFound, Cancelled,
                        Transfer { kind: TransferErrorKind, message: String }, Io(std::io::Error) }
  pub fn clone_repo(opts: &CloneOptions, credentials: &mut dyn CredentialProvider,
                    reporter: &mut dyn TransferReporter, cancel: &dyn Fn() -> bool)
      -> Result<CloneOutcome, CloneError>
  ```
  `TransferOperation::Clone` (Debug name `"Clone"` is the wire value).

- [ ] **Step 1: Add `TransferOperation::Clone`**

In `crates/git-core/src/remote.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferOperation {
    Fetch,
    Pull,
    PushBranch,
    ForcePushBranch,
    PushTags,
    Clone,
}
```

Run `cargo build --workspace`; fix any exhaustive `match` on `TransferOperation` the compiler flags (map `Clone` the same way as `Fetch` for labels).

- [ ] **Step 2: Write the failing tests**

`crates/git-core/tests/clone.rs`:

```rust
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
        Err(git2::Error::from_str("credentials were not expected for a local remote"))
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
        .push(&["refs/heads/main:refs/heads/main", "refs/heads/feature:refs/heads/feature"], None)
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
    CloneOptions { url: url.to_string(), dest, branch: None, recurse_submodules: false }
}

fn run(opts: &CloneOptions) -> (Result<git_core::clone::CloneOutcome, CloneError>, VecReporter) {
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
    assert_eq!(repo.head().unwrap().shorthand(), Some("main"));
    assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "main only\n");
    assert_eq!(repo.find_remote("origin").unwrap().url(), Some(source.url.as_str()));
    let upstream = repo.find_branch("main", git2::BranchType::Local).unwrap().upstream().unwrap();
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
    assert_eq!(repo.head().unwrap().shorthand(), Some("feature"));
    assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "two\n");
}

#[test]
fn unknown_branch_is_reported_and_the_created_destination_removed() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut options = opts(&source.url, dest.clone());
    options.branch = Some("nope".into());

    let (result, _) = run(&options);

    assert!(matches!(result, Err(CloneError::BranchNotFound)), "{result:?}");
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
    assert_eq!(repo.head().unwrap().shorthand(), Some("main"));
}

#[test]
fn refuses_a_non_empty_destination_and_leaves_it_untouched() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    std::fs::write(target.path().join("keep.txt"), "mine").unwrap();

    let (result, _) = run(&opts(&source.url, target.path().to_path_buf()));

    assert!(matches!(result, Err(CloneError::DestNotEmpty)), "{result:?}");
    assert_eq!(std::fs::read_to_string(target.path().join("keep.txt")).unwrap(), "mine");
    assert_eq!(std::fs::read_dir(target.path()).unwrap().count(), 1);
}

#[test]
fn refuses_a_destination_that_is_a_file() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let file = target.path().join("clone");
    std::fs::write(&file, "mine").unwrap();

    let (result, _) = run(&opts(&source.url, file.clone()));

    assert!(matches!(result, Err(CloneError::DestNotEmpty)), "{result:?}");
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
fn cancel_aborts_and_removes_the_created_destination() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut reporter = VecReporter::default();

    let result = clone_repo(&opts(&source.url, dest.clone()), &mut NoCredentials, &mut reporter, &|| true);

    assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    assert!(!dest.exists());
}

#[test]
fn reports_receiving_then_updating_progress_as_clone() {
    let source = source();
    let target = tempfile::TempDir::new().unwrap();

    let (result, reporter) = run(&opts(&source.url, target.path().join("clone")));

    result.unwrap();
    assert!(reporter.events.iter().all(|e| e.operation == TransferOperation::Clone));
    let first_updating = reporter.events.iter().position(|e| e.phase == TransferPhase::Updating);
    let last_receiving = reporter.events.iter().rposition(|e| e.phase == TransferPhase::Receiving);
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
        assert!(matches!(result, Err(CloneError::InvalidUrl)), "{url}: {result:?}");
        assert!(!dest.exists(), "{url}: destination must not be created");
    }
}

#[test]
fn rejects_a_url_with_an_embedded_password() {
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");

    let (result, _) = run(&opts("https://alice:secret@example.test/repo.git", dest.clone()));

    assert!(matches!(result, Err(CloneError::CredentialBearingUrl)), "{result:?}");
    assert!(!dest.exists());
}

#[test]
fn accepts_scp_style_and_ipv6_ssh_urls_at_validation() {
    for url in ["git@example.test:org/repo.git", "ssh://git@[::1]/repo.git", "/abs/local/repo"] {
        assert!(git_core::clone::validate_clone_url(url).is_ok(), "{url}");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p git-core --test clone`
Expected: compile error, `unresolved import git_core::clone`.

- [ ] **Step 4: Implement `clone.rs`**

`crates/git-core/src/clone.rs`:

```rust
//! Clone a remote into a new working directory, with the same credential, progress and cancel
//! seams as `remote::fetch_remote`. See
//! `docs/superpowers/specs/2026-10-17-sm-parity-batch3a-design.md`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{ErrorCode, FetchOptions, RemoteCallbacks};
use thiserror::Error;
use url::Url;

use crate::remote::{
    CredentialProvider, RemoteError, TransferErrorKind, TransferOperation, TransferPhase,
    TransferProgress, TransferReporter,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOptions {
    pub url: String,
    pub dest: PathBuf,
    pub branch: Option<String>,
    pub recurse_submodules: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneOutcome {
    pub path: PathBuf,
    /// Submodule paths whose init/update failed; the clone itself succeeded.
    pub submodule_failures: Vec<String>,
}

#[derive(Debug, Error)]
pub enum CloneError {
    #[error("Unsupported URL.")]
    InvalidUrl,
    #[error("The URL contains a password or token.")]
    CredentialBearingUrl,
    #[error("Destination already exists and isn't empty.")]
    DestNotEmpty,
    #[error("Branch not found on remote.")]
    BranchNotFound,
    #[error("Clone cancelled.")]
    Cancelled,
    #[error("{message}")]
    Transfer {
        kind: TransferErrorKind,
        message: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// User-typed clone URLs may point at a local repository, so unlike submodule URLs `file://`
/// and plain paths are allowed. Refused: empty input, remote-helper syntax
/// (`<transport>::<address>`, recognised as a `::` with no `/` before it so `ssh://[::1]/r`
/// still passes), and URLs carrying a password.
pub fn validate_clone_url(url: &str) -> Result<(), CloneError> {
    let url = url.trim();
    if url.is_empty() {
        return Err(CloneError::InvalidUrl);
    }
    if let Some(index) = url.find("::") {
        if !url[..index].contains('/') {
            return Err(CloneError::InvalidUrl);
        }
    }
    if let Ok(parsed) = Url::parse(url) {
        if parsed.password().is_some() {
            return Err(CloneError::CredentialBearingUrl);
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum DestState {
    Created,
    ExistingEmpty,
}

fn prepare_dest(dest: &Path) -> Result<DestState, CloneError> {
    match std::fs::symlink_metadata(dest) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(dest)?;
            Ok(DestState::Created)
        }
        Err(error) => Err(error.into()),
        Ok(meta) if meta.is_dir() && std::fs::read_dir(dest)?.next().is_none() => {
            Ok(DestState::ExistingEmpty)
        }
        Ok(_) => Err(CloneError::DestNotEmpty),
    }
}

/// Undoes a failed clone: everything in `dest` was written by us, because `prepare_dest` only
/// accepts a missing or empty directory. A directory the user picked stays, emptied.
fn cleanup(dest: &Path, state: DestState) {
    match state {
        DestState::Created => {
            let _ = std::fs::remove_dir_all(dest);
        }
        DestState::ExistingEmpty => {
            let Ok(entries) = std::fs::read_dir(dest) else { return };
            for entry in entries.flatten() {
                let path = entry.path();
                let is_real_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let _ = if is_real_dir {
                    std::fs::remove_dir_all(&path)
                } else {
                    std::fs::remove_file(&path)
                };
            }
        }
    }
}

pub fn clone_repo(
    opts: &CloneOptions,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<CloneOutcome, CloneError> {
    validate_clone_url(&opts.url)?;
    let url = opts.url.trim();
    let branch = opts.branch.as_deref().map(str::trim).filter(|b| !b.is_empty());
    let state = prepare_dest(&opts.dest)?;

    let credentials = RefCell::new(credentials);
    let reporter = RefCell::new(reporter);
    let result = {
        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|url, username, allowed| {
            credentials.borrow_mut().credential(url, username, allowed)
        });
        callbacks.transfer_progress(|progress| {
            if cancel() {
                return false;
            }
            reporter.borrow_mut().report(TransferProgress {
                operation_id: String::new(),
                operation: TransferOperation::Clone,
                phase: TransferPhase::Receiving,
                current: progress.received_objects(),
                total: progress.total_objects(),
                received_bytes: progress.received_bytes(),
                message: None,
            });
            true
        });
        let mut fetch = FetchOptions::new();
        fetch.remote_callbacks(callbacks);

        let mut checkout = CheckoutBuilder::new();
        checkout.progress(|_path, current, total| {
            reporter.borrow_mut().report(TransferProgress {
                operation_id: String::new(),
                operation: TransferOperation::Clone,
                phase: TransferPhase::Updating,
                current,
                total,
                received_bytes: 0,
                message: None,
            });
        });

        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch).with_checkout(checkout);
        if let Some(branch) = branch {
            builder.branch(branch);
        }
        builder.clone(url, &opts.dest)
    };

    let repo = match result {
        Ok(repo) => repo,
        Err(error) => {
            cleanup(&opts.dest, state);
            return Err(classify(error, branch.is_some(), cancel));
        }
    };

    let submodule_failures = if opts.recurse_submodules {
        update_all_submodules(&repo)
    } else {
        Vec::new()
    };
    Ok(CloneOutcome { path: opts.dest.clone(), submodule_failures })
}

fn classify(error: git2::Error, branch_requested: bool, cancel: &dyn Fn() -> bool) -> CloneError {
    if cancel() {
        return CloneError::Cancelled;
    }
    if branch_requested && error.code() == ErrorCode::NotFound {
        return CloneError::BranchNotFound;
    }
    let message = error.message().to_string();
    let kind = RemoteError::Git(error).transfer_error_kind();
    CloneError::Transfer { kind, message }
}

fn update_all_submodules(_repo: &git2::Repository) -> Vec<String> {
    Vec::new() // Task 2
}
```

If `unknown_branch_is_reported...` fails because libgit2 reports a different code, print `error.code()`/`error.class()` in the test, and match on that code instead. Keep the `branch_requested` guard, and don't match on message text: an HTTP "repository not found" must stay a transfer error.

If `cancel_aborts...` fails because the local transport never calls `transfer_progress` for this tiny fixture, also check `cancel()` in `CheckoutBuilder::notify` (return `false` to abort) and keep the `cancel()` check in `classify`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p git-core --test clone` — Expected: all PASS.
Run: `cargo clippy -p git-core --all-targets -- -D warnings` — Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/git-core/src/clone.rs crates/git-core/src/lib.rs crates/git-core/src/remote.rs crates/git-core/tests/clone.rs
git commit -m "feat(git-core): clone a remote with progress, cancel and cleanup"
```

---

### Task 2: `git-core` clone submodules

**Files:**
- Modify: `crates/git-core/src/clone.rs` (`update_all_submodules`)
- Test: `crates/git-core/tests/clone.rs` (append)

**Interfaces:**
- Consumes: `git_core::submodule::{list_submodules, init_submodule, update_submodule}` (`update_submodule(repo, path, recursive)`; rejects `file://` URLs with `DisallowedUrlScheme`).
- Produces: `CloneOutcome.submodule_failures` filled when `recurse_submodules` is true.

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/clone.rs`:

```rust
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
    git2::Repository::open_bare(&bare_path).unwrap().set_head(&head).unwrap();
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
    assert!(outcome.submodule_failures.is_empty(), "{:?}", outcome.submodule_failures);
    assert_eq!(std::fs::read_to_string(dest.join("deps/child/child.txt")).unwrap(), "child\n");
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
    let (_dir, url) = source_with_submodule(|p| url_for(p)); // file:// is refused for submodules
    let target = tempfile::TempDir::new().unwrap();
    let dest = target.path().join("clone");
    let mut options = opts(&url, dest.clone());
    options.recurse_submodules = true;

    let (result, _) = run(&options);

    let outcome = result.unwrap();
    assert_eq!(outcome.submodule_failures, vec!["deps/child".to_string()]);
    assert!(git2::Repository::open(&dest).is_ok());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test clone submodule`
Expected: `initializes_and_checks_out_submodules_when_requested` and `a_failing_submodule_...` FAIL (stub returns no work / no failures).

- [ ] **Step 3: Implement**

Replace the stub in `clone.rs`:

```rust
use crate::submodule::{init_submodule, list_submodules, update_submodule};

/// Best-effort: a submodule that can't be fetched (private URL, refused scheme) must not undo
/// an otherwise good clone. Failures are reported by path.
fn update_all_submodules(repo: &git2::Repository) -> Vec<String> {
    let submodules = match list_submodules(repo) {
        Ok(submodules) => submodules,
        Err(_) => return vec![".gitmodules".to_string()],
    };
    submodules
        .into_iter()
        .filter(|submodule| {
            init_submodule(repo, &submodule.path)
                .and_then(|()| update_submodule(repo, &submodule.path, true))
                .is_err()
        })
        .map(|submodule| submodule.path)
        .collect()
}
```

(Move the `use` to the top import block.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test clone` — Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/clone.rs crates/git-core/tests/clone.rs
git commit -m "feat(git-core): best-effort submodule update after clone"
```

---

### Task 3: `git-core` init_repo

**Files:**
- Modify: `crates/git-core/src/repo.rs:5-13`
- Test: `crates/git-core/tests/repo.rs` (append)

**Interfaces:**
- Produces: `pub fn init_repo(path: &Path) -> Result<(), RepoError>`; new variants `RepoError::AlreadyRepository`, `RepoError::Init(git2::Error)`, `RepoError::Io(std::io::Error)`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/git-core/tests/repo.rs`:

```rust
fn expected_initial_branch() -> String {
    git2::Config::open_default()
        .ok()
        .and_then(|config| config.get_string("init.defaultBranch").ok())
        .filter(|branch| !branch.is_empty())
        .unwrap_or_else(|| "main".to_string())
}

#[test]
fn init_creates_a_repository_with_missing_parents() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("a/b/new-repo");

    git_core::repo::init_repo(&path).unwrap();

    let repo = git2::Repository::open(&path).unwrap();
    let head = repo.find_reference("HEAD").unwrap();
    assert_eq!(
        head.symbolic_target(),
        Some(format!("refs/heads/{}", expected_initial_branch()).as_str())
    );
}

#[test]
fn init_uses_an_existing_empty_directory() {
    let dir = tempfile::TempDir::new().unwrap();

    git_core::repo::init_repo(dir.path()).unwrap();

    assert!(dir.path().join(".git").is_dir());
}

#[test]
fn init_refuses_an_existing_repository() {
    let (dir, _repo) = init_repo();

    let result = git_core::repo::init_repo(dir.path());

    assert!(matches!(result, Err(git_core::repo::RepoError::AlreadyRepository)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test repo` — Expected: compile error, no `init_repo`.

- [ ] **Step 3: Implement**

In `crates/git-core/src/repo.rs`:

```rust
#[derive(Debug, Error)]
pub enum RepoError {
    #[error("failed to open repository: {0}")]
    Open(#[from] git2::Error),
    #[error("This folder is already a Git repository.")]
    AlreadyRepository,
    #[error("failed to create repository: {0}")]
    Init(git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Creates an empty repository at `path` (and any missing parents). The initial branch honours
/// the user's global `init.defaultBranch`, else `main`.
pub fn init_repo(path: &Path) -> Result<(), RepoError> {
    if path.join(".git").exists() {
        return Err(RepoError::AlreadyRepository);
    }
    std::fs::create_dir_all(path)?;
    let branch = git2::Config::open_default()
        .ok()
        .and_then(|config| config.get_string("init.defaultBranch").ok())
        .filter(|branch| !branch.is_empty())
        .unwrap_or_else(|| "main".to_string());
    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head(&branch);
    git2::Repository::init_opts(path, &options).map_err(RepoError::Init)?;
    Ok(())
}
```

Run `cargo build --workspace`; fix any exhaustive `match` on `RepoError` elsewhere.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test repo` — Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/repo.rs crates/git-core/tests/repo.rs
git commit -m "feat(git-core): init a new repository"
```

---

### Task 4: `repo-service` CloneService

**Files:**
- Create: `crates/repo-service/src/clone.rs`
- Modify: `crates/repo-service/src/lib.rs` (add `pub mod clone;` before `pub mod credentials;`)

**Interfaces:**
- Consumes: `git_core::clone::{clone_repo, CloneOptions, CloneOutcome, CloneError}`; `crate::credentials::{CredentialService, CredentialStore, RemoteCredentialProvider}` (`RemoteCredentialProvider::new(&service, None)`); `crate::worker::TransferEvent::Progress`; `git_core::remote::{set_remote_auth_profile, RemoteAuthMode, CredentialProvider, TransferReporter}`.
- Produces:
  ```rust
  pub struct HttpsCredentialInput { pub username: String, pub token: String }
  pub struct ServiceCloneOutcome { pub outcome: CloneOutcome, pub credential_save_failed: bool }
  pub struct CloneService<S: CredentialStore> { .. }   // Send + Sync
  impl<S: CredentialStore> CloneService<S> {
      pub fn with_store_factory(factory: impl Fn() -> S + Send + Sync + 'static) -> Self;
      pub fn clone_repo(&self, operation_id: &str, opts: CloneOptions,
                        https: Option<HttpsCredentialInput>, events: Sender<TransferEvent>)
          -> Result<ServiceCloneOutcome, CloneError>;
      pub fn cancel(&self, operation_id: &str);
  }
  pub type DefaultCloneService = CloneService<DefaultCredentialStore>;
  impl DefaultCloneService { pub fn new() -> Self }
  ```

- [ ] **Step 1: Write the failing tests**

Bottom of `crates/repo-service/src/clone.rs` (test module first; the impl goes above it in Step 3):

```rust
#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    use git_core::clone::{CloneError, CloneOptions};

    use super::{CloneService, HttpsCredentialInput};
    use crate::credentials::{CredentialKey, CredentialStore, CredentialStoreError};
    use crate::worker::TransferEvent;

    #[derive(Clone, Default)]
    struct SharedStore {
        entries: Arc<Mutex<HashMap<String, String>>>,
        fail_writes: bool,
    }

    impl CredentialStore for SharedStore {
        fn get(&self, key: &CredentialKey) -> Result<Option<String>, CredentialStoreError> {
            Ok(self.entries.lock().unwrap().get(&format!("{key:?}")).cloned())
        }
        fn set(&self, key: &CredentialKey, token: &str) -> Result<(), CredentialStoreError> {
            if self.fail_writes {
                return Err(CredentialStoreError::Keychain("locked".into()));
            }
            self.entries.lock().unwrap().insert(format!("{key:?}"), token.to_string());
            Ok(())
        }
        fn delete(&self, key: &CredentialKey) -> Result<(), CredentialStoreError> {
            self.entries.lock().unwrap().remove(&format!("{key:?}"));
            Ok(())
        }
    }

    fn bare_source() -> (tempfile::TempDir, String) {
        let dir = tempfile::TempDir::new().unwrap();
        let work = git2::Repository::init(dir.path().join("work")).unwrap();
        std::fs::write(dir.path().join("work/a.txt"), "a\n").unwrap();
        let mut index = work.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        let tree = work.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        work.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[]).unwrap();
        let bare = dir.path().join("src.git");
        git2::Repository::init_bare(&bare).unwrap();
        let head = work.head().unwrap().name().unwrap().to_string();
        work.remote("origin", bare.to_str().unwrap())
            .unwrap()
            .push(&[format!("{head}:{head}")], None)
            .unwrap();
        git2::Repository::open_bare(&bare).unwrap().set_head(&head).unwrap();
        let url = format!("file://{}", bare.to_str().unwrap().replace('\\', "/"));
        (dir, url)
    }

    fn options(url: &str, dest: std::path::PathBuf) -> CloneOptions {
        CloneOptions { url: url.into(), dest, branch: None, recurse_submodules: false }
    }

    fn https() -> Option<HttpsCredentialInput> {
        Some(HttpsCredentialInput { username: "alice".into(), token: "tok".into() })
    }

    #[test]
    fn emits_only_progress_events_tagged_with_the_operation_id() {
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let service = CloneService::with_store_factory(SharedStore::default);
        let (tx, rx) = mpsc::channel();

        service.clone_repo("clone-1", options(&url, target.path().join("c")), None, tx).unwrap();

        let events: Vec<TransferEvent> = rx.try_iter().collect();
        assert!(!events.is_empty());
        for event in events {
            match event {
                TransferEvent::Progress(progress) => assert_eq!(progress.operation_id, "clone-1"),
                // A Started event would be adopted by an open repo tab's pending fetch/push.
                other => panic!("unexpected event {other:?}"),
            }
        }
    }

    #[test]
    fn persist_https_saves_the_token_and_sets_the_origin_profile() {
        // `clone_repo` can only reach `persist_https` over a real HTTPS host, which tests don't
        // have; exercise the save step directly on a real cloned repo.
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let dest = target.path().join("c");
        let cloned = CloneService::with_store_factory(SharedStore::default)
            .clone_repo("clone-1", options(&url, dest.clone()), None, mpsc::channel().0)
            .unwrap();
        let store = SharedStore::default();
        let service = crate::credentials::CredentialService::new(store.clone());
        let input = HttpsCredentialInput { username: "alice".into(), token: "tok".into() };

        super::persist_https(&service, "https://example.test/r.git", &cloned.outcome, &input).unwrap();

        assert_eq!(store.entries.lock().unwrap().len(), 1);
        let repo = git2::Repository::open(&dest).unwrap();
        assert_eq!(
            git_core::remote::remote_auth_profile(&repo, "origin").unwrap(),
            Some(git_core::remote::RemoteAuthMode::HttpsToken { username: "alice".into() })
        );
    }

    #[test]
    fn a_token_for_a_non_https_url_is_reported_unsaved_but_the_clone_succeeds() {
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let store = SharedStore::default();
        let shared = store.clone();
        let service = CloneService::with_store_factory(move || shared.clone());

        let outcome = service
            .clone_repo("clone-1", options(&url, target.path().join("c")), https(), mpsc::channel().0)
            .unwrap();

        assert!(outcome.credential_save_failed);
        assert!(store.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failed_clone_saves_nothing() {
        let target = tempfile::TempDir::new().unwrap();
        let store = SharedStore::default();
        let shared = store.clone();
        let service = CloneService::with_store_factory(move || shared.clone());
        let missing = format!("file://{}/missing.git", target.path().to_str().unwrap().replace('\\', "/"));

        let result = service.clone_repo("clone-1", options(&missing, target.path().join("c")), https(), mpsc::channel().0);

        assert!(result.is_err());
        assert!(store.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn a_keychain_failure_is_an_error_from_persist_https() {
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let cloned = CloneService::with_store_factory(SharedStore::default)
            .clone_repo("clone-1", options(&url, target.path().join("c")), None, mpsc::channel().0)
            .unwrap();
        let service = crate::credentials::CredentialService::new(SharedStore { fail_writes: true, ..Default::default() });
        let input = HttpsCredentialInput { username: "alice".into(), token: "tok".into() };

        let result = super::persist_https(&service, "https://example.test/r.git", &cloned.outcome, &input);

        assert!(result.is_err());
    }

    #[test]
    fn cancel_before_start_is_honoured() {
        // The frontend may call cancel_clone before clone_repo's thread has registered its flag.
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let service = CloneService::with_store_factory(SharedStore::default);
        service.cancel("clone-3");

        let result = service.clone_repo("clone-3", options(&url, target.path().join("c")), None, mpsc::channel().0);

        assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    }

    #[test]
    fn cancel_flag_is_registered_while_running_and_removed_after() {
        let service = CloneService::with_store_factory(SharedStore::default);
        service.register("clone-9");
        service.cancel("clone-9");
        assert!(service.is_cancelled("clone-9"));
        service.unregister("clone-9");
        assert!(!service.is_cancelled("clone-9"));
    }

    #[test]
    fn a_pre_cancelled_clone_reports_cancelled() {
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let service = CloneService::with_store_factory(SharedStore::default);
        service.register("clone-2");
        service.cancel("clone-2");

        let result = service.clone_repo("clone-2", options(&url, target.path().join("c")), None, mpsc::channel().0);

        assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    }
}
```

`CredentialKey` must implement `Debug` for the fake's map key; if it doesn't, add `#[derive(Debug)]` to it in `credentials.rs`. If `TransferEvent` lacks `Debug`, add `#[derive(Debug)]` to it in `worker/mod.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service clone::` — Expected: compile errors (module items missing).

- [ ] **Step 3: Implement**

Top of `crates/repo-service/src/clone.rs`:

```rust
//! Repo-less clone jobs. A clone has no repository (and so no per-repo worker) until it
//! finishes, so it runs here: one call per clone on the caller's thread, with its own cancel
//! registry. Progress goes out as `TransferEvent::Progress` only — never `Started`/`Completed`,
//! which an open repo tab's pending fetch/push would adopt as its own transfer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use git_core::clone::{clone_repo, CloneError, CloneOptions, CloneOutcome};
use git_core::remote::{
    set_remote_auth_profile, CredentialProvider, RemoteAuthMode, TransferProgress,
    TransferReporter,
};

use crate::credentials::{CredentialService, CredentialStore, RemoteCredentialProvider};
use crate::worker::TransferEvent;

pub struct HttpsCredentialInput {
    pub username: String,
    pub token: String,
}

#[derive(Debug)]
pub struct ServiceCloneOutcome {
    pub outcome: CloneOutcome,
    pub credential_save_failed: bool,
}

type StoreFactory<S> = Box<dyn Fn() -> S + Send + Sync>;

pub struct CloneService<S: CredentialStore> {
    new_store: StoreFactory<S>,
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[cfg(not(feature = "forge-fixture-override"))]
pub type DefaultCredentialStore = crate::credentials::KeyringCredentialStore;
#[cfg(feature = "forge-fixture-override")]
pub type DefaultCredentialStore = crate::credentials::InMemoryCredentialStore;

pub type DefaultCloneService = CloneService<DefaultCredentialStore>;

impl DefaultCloneService {
    /// Same store choice as `Worker::spawn`: the OS keychain, or the in-memory store in E2E
    /// builds.
    pub fn new() -> Self {
        #[cfg(not(feature = "forge-fixture-override"))]
        return Self::with_store_factory(|| crate::credentials::KeyringCredentialStore);
        #[cfg(feature = "forge-fixture-override")]
        return Self::with_store_factory(crate::credentials::InMemoryCredentialStore::default);
    }
}

impl Default for DefaultCloneService {
    fn default() -> Self {
        Self::new()
    }
}

/// The token the user typed into the clone dialog, offered once from memory. It reaches the
/// keychain only after the clone succeeds (`persist_https`), so a wrong token is never saved.
struct OneShotHttps<'a> {
    input: &'a HttpsCredentialInput,
}

impl CredentialProvider for OneShotHttps<'_> {
    fn credential(
        &mut self,
        _url: &str,
        _username: Option<&str>,
        _allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        git2::Cred::userpass_plaintext(&self.input.username, &self.input.token)
    }
}

struct ChannelReporter<'a> {
    events: &'a Sender<TransferEvent>,
    operation_id: &'a str,
}

impl TransferReporter for ChannelReporter<'_> {
    fn report(&mut self, progress: TransferProgress) {
        let _ = self.events.send(TransferEvent::Progress(TransferProgress {
            operation_id: self.operation_id.to_string(),
            ..progress
        }));
    }
}

impl<S: CredentialStore> CloneService<S> {
    pub fn with_store_factory(factory: impl Fn() -> S + Send + Sync + 'static) -> Self {
        Self { new_store: Box::new(factory), running: Mutex::new(HashMap::new()) }
    }

    /// Registers `operation_id`'s cancel flag (idempotent: a flag set by an early `cancel`
    /// survives) and returns it.
    pub(crate) fn register(&self, operation_id: &str) -> Arc<AtomicBool> {
        Arc::clone(
            self.running
                .lock()
                .unwrap()
                .entry(operation_id.to_string())
                .or_insert_with(|| Arc::new(AtomicBool::new(false))),
        )
    }

    pub(crate) fn unregister(&self, operation_id: &str) {
        self.running.lock().unwrap().remove(operation_id);
    }

    #[cfg(test)]
    pub(crate) fn is_cancelled(&self, operation_id: &str) -> bool {
        self.running
            .lock()
            .unwrap()
            .get(operation_id)
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    /// Sets `operation_id`'s cancel flag, creating it if the clone hasn't registered yet — the
    /// frontend can cancel before `clone_repo`'s blocking thread starts. A flag for an id that
    /// never starts is a few bytes (ids are UUIDs, created only by a user pressing Cancel).
    pub fn cancel(&self, operation_id: &str) {
        self.running
            .lock()
            .unwrap()
            .entry(operation_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Ordering::Relaxed);
    }

    pub fn clone_repo(
        &self,
        operation_id: &str,
        opts: CloneOptions,
        https: Option<HttpsCredentialInput>,
        events: Sender<TransferEvent>,
    ) -> Result<ServiceCloneOutcome, CloneError> {
        let flag = self.register(operation_id);
        let cancel = || flag.load(Ordering::Relaxed);
        let mut reporter = ChannelReporter { events: &events, operation_id };
        let service = CredentialService::new((self.new_store)());
        let result = match &https {
            Some(input) => clone_repo(&opts, &mut OneShotHttps { input }, &mut reporter, &cancel),
            None => {
                let mut provider = RemoteCredentialProvider::new(&service, None);
                clone_repo(&opts, &mut provider, &mut reporter, &cancel)
            }
        };
        self.unregister(operation_id);
        let outcome = result?;
        let credential_save_failed = match &https {
            Some(input) => persist_https(&service, opts.url.trim(), &outcome, input).is_err(),
            None => false,
        };
        Ok(ServiceCloneOutcome { outcome, credential_save_failed })
    }
}

fn persist_https<S: CredentialStore>(
    service: &CredentialService<S>,
    url: &str,
    outcome: &CloneOutcome,
    input: &HttpsCredentialInput,
) -> Result<(), String> {
    service
        .save_https(url, &input.username, &input.token)
        .map_err(|error| error.to_string())?;
    let repo = git2::Repository::open(&outcome.path).map_err(|error| error.to_string())?;
    set_remote_auth_profile(
        &repo,
        "origin",
        RemoteAuthMode::HttpsToken { username: input.username.clone() },
    )
    .map_err(|error| error.to_string())
}
```

`CredentialKey::for_https` rejects non-HTTPS URLs, so a token given for a `file://`/SSH URL comes back as `credential_save_failed: true`. That's intended: there's no HTTPS host to key it by.

`RemoteCredentialProvider` is `pub(crate)` (same crate, fine). `InMemoryCredentialStore` must implement `Default` for `DefaultCloneService::new` under `forge-fixture-override`; add `#[derive(Default)]` if it's missing.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p repo-service clone::` — Expected: PASS.
Run: `cargo test -p repo-service --features forge-fixture-override clone::` — Expected: PASS (both store aliases compile).
Run: `cargo clippy --workspace --all-targets -- -D warnings` — Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/repo-service/src/clone.rs crates/repo-service/src/lib.rs crates/repo-service/src/credentials.rs crates/repo-service/src/worker/mod.rs
git commit -m "feat(repo-service): clone service with cancel registry and one-shot HTTPS token"
```

---

### Task 5: Tauri commands and DTOs

**Files:**
- Create: `crates/tauri-app/src/commands/clone.rs`
- Modify: `crates/tauri-app/src/commands/mod.rs` (add `mod clone;`, `pub use clone::{cancel_clone, clone_repo, init_repo, CloneState};`, and the DTO tests in the existing `#[cfg(test)]` module)
- Modify: `crates/tauri-app/src/main.rs:55-56` (`.manage(CloneState::default())`, register 3 commands in `generate_handler!`)

**Interfaces:**
- Consumes: `repo_service::clone::{DefaultCloneService, HttpsCredentialInput, ServiceCloneOutcome}`, `git_core::clone::{CloneError, CloneOptions}`, `git_core::repo::{init_repo, RepoError}`, `commands::emit_transfer_events(app, rx)`.
- Produces (wire):
  - `clone_repo({ operationId, request: { url, dest, branch, recurseSubmodules, https: { username, token } | null } }) -> { path, submoduleFailures, credentialSaveFailed }`
  - `cancel_clone({ operationId }) -> null`
  - `init_repo({ path }) -> null`
  - Errors: `{ kind: string, message: string }`, kind ∈ `InvalidUrl | CredentialBearingUrl | DestNotEmpty | BranchNotFound | Cancelled | AlreadyRepository | Io | InitFailed | <TransferErrorKind>`.

- [ ] **Step 1: Write the failing DTO tests**

In `crates/tauri-app/src/commands/clone.rs` (bottom):

```rust
#[cfg(test)]
mod tests {
    use git_core::clone::{CloneError, CloneOutcome};
    use git_core::remote::TransferErrorKind;
    use repo_service::clone::ServiceCloneOutcome;

    use super::{CloneErrorDto, CloneOutcomeDto, CloneRequestDto};

    #[test]
    fn outcome_serializes_camel_case() {
        let dto = CloneOutcomeDto::from(ServiceCloneOutcome {
            outcome: CloneOutcome { path: "/tmp/r".into(), submodule_failures: vec!["deps/a".into()] },
            credential_save_failed: true,
        });
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({ "path": "/tmp/r", "submoduleFailures": ["deps/a"], "credentialSaveFailed": true })
        );
    }

    #[test]
    fn errors_serialize_as_kind_and_message() {
        let cases = [
            (CloneError::InvalidUrl, "InvalidUrl"),
            (CloneError::CredentialBearingUrl, "CredentialBearingUrl"),
            (CloneError::DestNotEmpty, "DestNotEmpty"),
            (CloneError::BranchNotFound, "BranchNotFound"),
            (CloneError::Cancelled, "Cancelled"),
            (
                CloneError::Transfer { kind: TransferErrorKind::MissingCredential, message: "x".into() },
                "MissingCredential",
            ),
        ];
        for (error, kind) in cases {
            let value = serde_json::to_value(CloneErrorDto::from(error)).unwrap();
            assert_eq!(value["kind"], kind);
            assert!(value["message"].is_string());
        }
    }

    #[test]
    fn already_repository_init_error_has_its_own_kind() {
        let dto = CloneErrorDto::from(git_core::repo::RepoError::AlreadyRepository);
        assert_eq!(serde_json::to_value(dto).unwrap()["kind"], "AlreadyRepository");
    }

    #[test]
    fn request_deserializes_from_camel_case_with_null_https() {
        let request: CloneRequestDto = serde_json::from_value(serde_json::json!({
            "url": "https://example.test/r.git", "dest": "/tmp/r", "branch": null,
            "recurseSubmodules": true, "https": null
        }))
        .unwrap();
        assert!(request.recurse_submodules);
        assert!(request.https.is_none());
    }

    #[test]
    fn clone_progress_reports_its_operation_name() {
        let dto = crate::commands::TransferProgressDto::from(git_core::remote::TransferProgress {
            operation_id: "c".into(),
            operation: git_core::remote::TransferOperation::Clone,
            phase: git_core::remote::TransferPhase::Receiving,
            current: 1,
            total: 2,
            received_bytes: 3,
            message: None,
        });
        assert_eq!(dto.operation, "Clone");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tauri-app clone` — Expected: compile errors.

- [ ] **Step 3: Implement**

Top of `crates/tauri-app/src/commands/clone.rs`:

```rust
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use git_core::clone::{CloneError, CloneOptions};
use git_core::repo::RepoError;
use repo_service::clone::{DefaultCloneService, HttpsCredentialInput, ServiceCloneOutcome};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use super::emit_transfer_events;

#[derive(Default)]
pub struct CloneState(pub Arc<DefaultCloneService>);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpsInputDto {
    pub username: String,
    pub token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneRequestDto {
    pub url: String,
    pub dest: String,
    pub branch: Option<String>,
    pub recurse_submodules: bool,
    pub https: Option<HttpsInputDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloneOutcomeDto {
    pub path: String,
    pub submodule_failures: Vec<String>,
    pub credential_save_failed: bool,
}

impl From<ServiceCloneOutcome> for CloneOutcomeDto {
    fn from(value: ServiceCloneOutcome) -> Self {
        Self {
            path: value.outcome.path.to_string_lossy().into_owned(),
            submodule_failures: value.outcome.submodule_failures,
            credential_save_failed: value.credential_save_failed,
        }
    }
}

/// Clone/init failures cross IPC as `{ kind, message }` so the dialog can branch on `kind`.
/// `message` is a `Display` string from git-core: it never contains the token (the token only
/// lives in `OneShotHttps`, and libgit2 error text doesn't echo credentials).
#[derive(Serialize, Debug)]
pub struct CloneErrorDto {
    pub kind: String,
    pub message: String,
}

impl From<CloneError> for CloneErrorDto {
    fn from(error: CloneError) -> Self {
        let kind = match &error {
            CloneError::InvalidUrl => "InvalidUrl".to_string(),
            CloneError::CredentialBearingUrl => "CredentialBearingUrl".to_string(),
            CloneError::DestNotEmpty => "DestNotEmpty".to_string(),
            CloneError::BranchNotFound => "BranchNotFound".to_string(),
            CloneError::Cancelled => "Cancelled".to_string(),
            CloneError::Transfer { kind, .. } => format!("{kind:?}"),
            CloneError::Io(_) => "Io".to_string(),
        };
        Self { kind, message: error.to_string() }
    }
}

impl From<RepoError> for CloneErrorDto {
    fn from(error: RepoError) -> Self {
        let kind = match error {
            RepoError::AlreadyRepository => "AlreadyRepository",
            _ => "InitFailed",
        };
        Self { kind: kind.to_string(), message: error.to_string() }
    }
}

#[tauri::command]
pub async fn clone_repo(
    operation_id: String,
    request: CloneRequestDto,
    app: AppHandle,
    state: State<'_, CloneState>,
) -> Result<CloneOutcomeDto, CloneErrorDto> {
    let service = Arc::clone(&state.0);
    let (event_tx, event_rx) = mpsc::channel();
    emit_transfer_events(app, event_rx);
    let options = CloneOptions {
        url: request.url,
        dest: PathBuf::from(request.dest),
        branch: request.branch,
        recurse_submodules: request.recurse_submodules,
    };
    let https = request
        .https
        .map(|input| HttpsCredentialInput { username: input.username, token: input.token });
    tauri::async_runtime::spawn_blocking(move || {
        service.clone_repo(&operation_id, options, https, event_tx)
    })
    .await
    .map_err(|error| CloneErrorDto { kind: "Io".into(), message: error.to_string() })?
    .map(CloneOutcomeDto::from)
    .map_err(CloneErrorDto::from)
}

#[tauri::command]
pub async fn cancel_clone(operation_id: String, state: State<'_, CloneState>) -> Result<(), String> {
    state.0.cancel(&operation_id);
    Ok(())
}

#[tauri::command]
pub async fn init_repo(path: String) -> Result<(), CloneErrorDto> {
    git_core::repo::init_repo(std::path::Path::new(&path)).map_err(CloneErrorDto::from)
}
```

Make `emit_transfer_events` and `TransferProgressDto` reachable from the submodule (`pub(crate)` if they're private). In `main.rs`: `.manage(CloneState::default())` next to `.manage(AppState::default())`, plus `clone_repo, cancel_clone, init_repo` in `generate_handler!` and the `use` list.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tauri-app clone` — Expected: PASS.
Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check` — Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app/src/commands/clone.rs crates/tauri-app/src/commands/mod.rs crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): clone, cancel clone and init repository commands"
```

---

### Task 6: `RepoClient` clone/init methods (both clients + fakes)

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts` (types near `TransferOperation` at line 94; methods in `RepoClient` after `scanReposInRoot`)
- Modify: `frontend/src/ipc/tauriRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: every full `RepoClient` fake — find them with `grep -rl "getHeadInfo:" frontend/src` (today: `App.test.tsx`, `components/RepoPicker.test.tsx`, `components/RebasePlanner.test.tsx`, `state/useAppState.test.ts`, `components/ConflictResolutionPane.test.tsx`, `components/DiffPane.test.tsx`)
- Test: `frontend/src/ipc/cloneError.test.ts`

**Interfaces:**
- Produces:
  ```ts
  export type TransferOperation = "Fetch" | "Pull" | "PushBranch" | "PushTags" | "ForcePushBranch" | "Clone";
  export interface CloneRequest { url: string; dest: string; branch: string | null; recurseSubmodules: boolean;
                                  https: { username: string; token: string } | null; }
  export interface CloneOutcome { path: string; submoduleFailures: string[]; credentialSaveFailed: boolean; }
  export type CloneErrorKind = "InvalidUrl" | "CredentialBearingUrl" | "DestNotEmpty" | "BranchNotFound" | "Cancelled"
                             | "AlreadyRepository" | "Io" | "InitFailed" | TransferErrorKind;
  export class CloneError extends Error { readonly kind: CloneErrorKind }
  export function toCloneError(raw: unknown): CloneError;
  // RepoClient:
  readonly supportsCloneAndInit: boolean;
  cloneRepo(operationId: string, request: CloneRequest): Promise<CloneOutcome>;
  cancelClone(operationId: string): Promise<void>;
  initRepo(path: string): Promise<void>;
  ```

- [ ] **Step 1: Write the failing test**

`frontend/src/ipc/cloneError.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { CloneError, toCloneError } from "./RepoClient";

describe("toCloneError", () => {
  it("keeps kind and message from a backend { kind, message } payload", () => {
    const error = toCloneError({ kind: "DestNotEmpty", message: "Destination already exists and isn't empty." });
    expect(error).toBeInstanceOf(CloneError);
    expect(error.kind).toBe("DestNotEmpty");
    expect(error.message).toBe("Destination already exists and isn't empty.");
  });

  it("maps anything else to a TransferFailed error with its string form", () => {
    const error = toCloneError("worker crashed");
    expect(error.kind).toBe("TransferFailed");
    expect(error.message).toBe("worker crashed");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm test -- --run src/ipc/cloneError.test.ts` — Expected: FAIL, `toCloneError` not exported.

- [ ] **Step 3: Implement types and helper in `RepoClient.ts`**

```ts
export type TransferOperation = "Fetch" | "Pull" | "PushBranch" | "PushTags" | "ForcePushBranch" | "Clone";

export interface CloneRequest {
  url: string;
  dest: string;
  branch: string | null;
  recurseSubmodules: boolean;
  https: { username: string; token: string } | null;
}

export interface CloneOutcome {
  path: string;
  submoduleFailures: string[];
  credentialSaveFailed: boolean;
}

export type CloneErrorKind =
  | "InvalidUrl"
  | "CredentialBearingUrl"
  | "DestNotEmpty"
  | "BranchNotFound"
  | "Cancelled"
  | "AlreadyRepository"
  | "Io"
  | "InitFailed"
  | TransferErrorKind;

/** Clone/init failure; the backend sends `{ kind, message }` so callers can branch on `kind`. */
export class CloneError extends Error {
  readonly kind: CloneErrorKind;
  constructor(kind: CloneErrorKind, message: string) {
    super(message);
    this.name = "CloneError";
    this.kind = kind;
  }
}

export function toCloneError(raw: unknown): CloneError {
  if (typeof raw === "object" && raw !== null && "kind" in raw && "message" in raw) {
    const { kind, message } = raw as { kind: unknown; message: unknown };
    if (typeof kind === "string" && typeof message === "string") {
      return new CloneError(kind as CloneErrorKind, message);
    }
  }
  return new CloneError("TransferFailed", String(raw));
}
```

In `interface RepoClient`, after `scanReposInRoot`:

```ts
  /** False in the VSCode extension, where VSCode's own Git: Clone / Initialize cover this. */
  readonly supportsCloneAndInit: boolean;
  /**
   * Clones and resolves when done. `operationId` is chosen by the caller so it can
   * `cancelClone` before this resolves; progress arrives on `subscribeTransferProgress` with
   * `operation: "Clone"` (Progress phases only, no Starting/Completed). Rejects with `CloneError`.
   */
  cloneRepo(operationId: string, request: CloneRequest): Promise<CloneOutcome>;
  cancelClone(operationId: string): Promise<void>;
  /** Rejects with `CloneError` (`AlreadyRepository` when `path/.git` exists). */
  initRepo(path: string): Promise<void>;
```

- [ ] **Step 4: Implement in both clients**

`tauriRepoClient.ts` (add `CloneOutcome`, `CloneRequest` to the type import and `toCloneError` as a value import):

```ts
  supportsCloneAndInit: true,
  cloneRepo: (operationId: string, request: CloneRequest) =>
    loggedInvoke<CloneOutcome>("clone_repo", { operationId, request }).catch((error: unknown) => {
      throw toCloneError(error);
    }),
  cancelClone: (operationId: string) => loggedInvoke<void>("cancel_clone", { operationId }),
  initRepo: (path: string) =>
    loggedInvoke<void>("init_repo", { path }).catch((error: unknown) => {
      throw toCloneError(error);
    }),
```

`loggedInvoke` logs `String(error)` of the payload, which is `{ kind, message }` with no token, so the log stays safe.

`vscodeRepoClient.ts`:

```ts
  supportsCloneAndInit: false,
  cloneRepo: () => Promise.reject(new Error("Not supported in the VSCode extension")),
  cancelClone: () => Promise.reject(new Error("Not supported in the VSCode extension")),
  initRepo: () => Promise.reject(new Error("Not supported in the VSCode extension")),
```

Every fake from the file list gets:

```ts
    supportsCloneAndInit: false,
    cloneRepo: async () => unimplemented(),
    cancelClone: async () => unimplemented(),
    initRepo: async () => unimplemented(),
```

(Use each file's own "unimplemented"/`vi.fn()` idiom.)

- [ ] **Step 5: Verify**

Run: `cd frontend && pnpm test -- --run src/ipc/cloneError.test.ts` — Expected: PASS.
Run: `cd frontend && pnpm exec tsc --noEmit -p . && pnpm lint` — Expected: clean. A type error naming a missing `cloneRepo` means a fake was missed: add it there.
Run: `cd frontend && pnpm test -- --run` — Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): clone and init RepoClient methods"
```

---

### Task 7: `CloneDialog` component

**Files:**
- Create: `frontend/src/components/CloneDialog.tsx`, `frontend/src/components/CloneDialog.module.css`, `frontend/src/lib/cloneNames.ts`
- Test: `frontend/src/components/CloneDialog.test.tsx`, `frontend/src/lib/cloneNames.test.ts`

**Interfaces:**
- Consumes: `RepoClient.{cloneRepo, cancelClone, pickRepoFolder, subscribeTransferProgress}`, `CloneError`, `FormDialog` (`ariaLabel`, `onCancel`, children), `InlineError` (`message`, `onDismiss`), `Toolbar`.
- Produces:
  ```ts
  export function deriveRepoName(url: string): string;
  export function joinPath(parent: string, name: string): string;
  export function CloneDialog(props: { client: RepoClient; onCloned: (outcome: CloneOutcome, name: string) => void; onCancel: () => void }): JSX.Element;
  ```

Error copy (exact):

| kind | shown |
|---|---|
| `DestNotEmpty` | `${dest} already exists and isn't empty.` |
| `BranchNotFound` | `Branch ${branch} not found on the remote.` |
| `InvalidUrl` | `Unsupported URL. Use https://, ssh://, git@host:path or a local path.` |
| `CredentialBearingUrl` | `Remove the password or token from the URL and put it in the token field.` (HTTPS section expands) |
| `MissingCredential` | `This repository needs a username and token.` (HTTPS section expands) |
| `SshAgentFailure` | `SSH agent unavailable or has no matching key.` |
| `Cancelled` | nothing; form unlocks |
| anything else | the error's `message` |

- [ ] **Step 1: Write the failing name helper tests**

`frontend/src/lib/cloneNames.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { deriveRepoName, joinPath } from "./cloneNames";

describe("deriveRepoName", () => {
  it.each([
    ["https://github.com/org/repo.git", "repo"],
    ["https://github.com/org/repo/", "repo"],
    ["git@github.com:org/repo.git", "repo"],
    ["git@host:repo.git", "repo"],
    ["ssh://git@host:22/org/repo", "repo"],
    ["/home/me/src/thing.git", "thing"],
    ["C:\\src\\thing", "thing"],
    ["  https://h/x.git  ", "x"],
    ["", ""],
  ])("%s -> %s", (url, name) => {
    expect(deriveRepoName(url)).toBe(name);
  });
});

describe("joinPath", () => {
  it("joins with the parent's separator and drops trailing separators", () => {
    expect(joinPath("/home/me/src/", "repo")).toBe("/home/me/src/repo");
    expect(joinPath("C:\\src\\", "repo")).toBe("C:\\src\\repo");
    expect(joinPath("/", "repo")).toBe("/repo");
  });
});
```

- [ ] **Step 2: Run to fail, then implement `cloneNames.ts`**

Run: `cd frontend && pnpm test -- --run src/lib/cloneNames.test.ts` — Expected: FAIL (module missing).

```ts
/** Last path segment of a clone URL without `.git` — what `git clone` names the folder. */
export function deriveRepoName(url: string): string {
  const trimmed = url.trim().replace(/[\\/]+$/, "");
  const segment = trimmed.split(/[\\/:]/).pop() ?? "";
  return segment.replace(/\.git$/, "");
}

export function joinPath(parent: string, name: string): string {
  const separator = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  const base = parent.replace(/[\\/]+$/, "");
  return `${base}${separator}${name}`;
}
```

Run again — Expected: PASS.

- [ ] **Step 3: Write the failing dialog tests**

`frontend/src/components/CloneDialog.test.tsx`:

```tsx
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CloneError, type CloneOutcome, type RepoClient, type TransferProgress } from "../ipc/RepoClient";
import { CloneDialog } from "./CloneDialog";

type Listener = (progress: TransferProgress) => void;

function fakeClient(overrides: Partial<RepoClient> = {}) {
  const listeners: Listener[] = [];
  const client = {
    supportsCloneAndInit: true,
    pickRepoFolder: vi.fn(async () => "/home/me/src"),
    cloneRepo: vi.fn(async (): Promise<CloneOutcome> => ({ path: "/home/me/src/repo", submoduleFailures: [], credentialSaveFailed: false })),
    cancelClone: vi.fn(async () => {}),
    subscribeTransferProgress: (listener: Listener) => {
      listeners.push(listener);
      return () => listeners.splice(listeners.indexOf(listener), 1);
    },
    ...overrides,
  } as unknown as RepoClient;
  return { client, emit: (p: TransferProgress) => listeners.forEach((l) => l(p)) };
}

function fill(url: string, parent = "/home/me/src") {
  fireEvent.change(screen.getByLabelText("Repository URL"), { target: { value: url } });
  fireEvent.change(screen.getByLabelText("Parent folder"), { target: { value: parent } });
}

describe("CloneDialog", () => {
  it("derives the folder name from the URL until the user edits it", () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    expect(screen.getByLabelText("Folder name")).toHaveValue("repo");
    fireEvent.change(screen.getByLabelText("Folder name"), { target: { value: "mine" } });
    fill("https://github.com/org/other.git");
    expect(screen.getByLabelText("Folder name")).toHaveValue("mine");
    expect(screen.getByText("/home/me/src/mine")).toBeInTheDocument();
  });

  it("shows the HTTPS fields only for http(s) URLs", () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("git@github.com:org/repo.git");
    expect(screen.queryByRole("button", { name: /HTTPS credentials/ })).not.toBeInTheDocument();
    fill("https://github.com/org/repo.git");
    expect(screen.getByRole("button", { name: /HTTPS credentials/ })).toBeInTheDocument();
  });

  it("disables Clone until URL, parent and name are filled", () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Clone" })).toBeDisabled();
    fill("https://github.com/org/repo.git");
    expect(screen.getByRole("button", { name: "Clone" })).toBeEnabled();
  });

  it("fills the parent folder from Browse", async () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Browse…" }));
    await waitFor(() => expect(screen.getByLabelText("Parent folder")).toHaveValue("/home/me/src"));
  });

  it("sends the request and reports the outcome", async () => {
    const onCloned = vi.fn();
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={onCloned} onCancel={vi.fn()} />);
    fill(" https://github.com/org/repo.git ");
    fireEvent.change(screen.getByLabelText("Branch (optional)"), { target: { value: "  " } });
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(onCloned).toHaveBeenCalled());
    expect(client.cloneRepo).toHaveBeenCalledWith(expect.any(String), {
      url: "https://github.com/org/repo.git",
      dest: "/home/me/src/repo",
      branch: null,
      recurseSubmodules: true,
      https: null,
    });
    expect(onCloned).toHaveBeenCalledWith(expect.objectContaining({ path: "/home/me/src/repo" }), "repo");
  });

  it("sends HTTPS credentials when entered", async () => {
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: /HTTPS credentials/ }));
    fireEvent.change(screen.getByLabelText("Username"), { target: { value: "alice" } });
    fireEvent.change(screen.getByLabelText("Token"), { target: { value: "tok" } });
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(client.cloneRepo).toHaveBeenCalled());
    expect(vi.mocked(client.cloneRepo).mock.calls[0][1].https).toEqual({ username: "alice", token: "tok" });
  });

  it("shows progress for its own operation only and cancels it", async () => {
    let finish: (o: CloneOutcome) => void = () => {};
    const { client, emit } = fakeClient({
      cloneRepo: vi.fn(() => new Promise<CloneOutcome>((resolve) => (finish = resolve))),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    const opId = vi.mocked(client.cloneRepo).mock.calls[0][0];
    const base = { operation: "Clone", errorKind: null, receivedBytes: 0, message: null } as const;
    act(() => emit({ ...base, operationId: "other", phase: "Receiving", current: 9, total: 9 }));
    act(() => emit({ ...base, operationId: opId, phase: "Receiving", current: 3, total: 10 }));
    expect(screen.getByText("Receiving objects 3/10")).toBeInTheDocument();
    expect(screen.getByLabelText("Repository URL")).toBeDisabled();
    act(() => emit({ ...base, operationId: opId, phase: "Updating", current: 1, total: 2 }));
    expect(screen.getByText("Checking out…")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Cancel clone" }));
    expect(client.cancelClone).toHaveBeenCalledWith(opId);
    await act(async () => finish({ path: "/x", submoduleFailures: [], credentialSaveFailed: false }));
  });

  it.each([
    ["DestNotEmpty", "/home/me/src/repo already exists and isn't empty."],
    ["BranchNotFound", "Branch dev not found on the remote."],
    ["InvalidUrl", "Unsupported URL. Use https://, ssh://, git@host:path or a local path."],
    ["SshAgentFailure", "SSH agent unavailable or has no matching key."],
    ["TransferFailed", "backend said no"],
  ] as const)("shows %s inline and stays open", async (kind, text) => {
    const { client } = fakeClient({
      cloneRepo: vi.fn(async () => {
        throw new CloneError(kind, "backend said no");
      }),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.change(screen.getByLabelText("Branch (optional)"), { target: { value: "dev" } });
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(await screen.findByText(text)).toBeInTheDocument();
    expect(screen.getByLabelText("Repository URL")).toBeEnabled();
  });

  it.each([
    ["MissingCredential", "This repository needs a username and token."],
    ["CredentialBearingUrl", "Remove the password or token from the URL and put it in the token field."],
  ] as const)("%s expands the HTTPS section", async (kind, text) => {
    const { client } = fakeClient({
      cloneRepo: vi.fn(async () => {
        throw new CloneError(kind, "x");
      }),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(await screen.findByText(text)).toBeInTheDocument();
    expect(screen.getByLabelText("Token")).toBeInTheDocument();
  });

  it("returns to the form without an error after a cancelled clone", async () => {
    const { client } = fakeClient({
      cloneRepo: vi.fn(async () => {
        throw new CloneError("Cancelled", "Clone cancelled.");
      }),
    });
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(screen.getByLabelText("Repository URL")).toBeEnabled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
```

If `InlineError` doesn't render `role="alert"`, assert `screen.queryByText("Clone cancelled.")` is absent instead.

- [ ] **Step 4: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/CloneDialog.test.tsx` — Expected: FAIL (module missing).

- [ ] **Step 5: Implement `CloneDialog.tsx`**

```tsx
import { useEffect, useRef, useState } from "react";
import type { CloneOutcome, RepoClient } from "../ipc/RepoClient";
import { CloneError } from "../ipc/RepoClient";
import { deriveRepoName, joinPath } from "../lib/cloneNames";
import { FormDialog } from "./primitives/FormDialog";
import { InlineError } from "./primitives/InlineError";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./CloneDialog.module.css";

type Progress = { phase: "Receiving"; current: number; total: number } | { phase: "Updating" } | null;

const isHttpUrl = (url: string) => /^https?:\/\//i.test(url.trim());

function messageFor(error: CloneError, dest: string, branch: string): string | null {
  switch (error.kind) {
    case "Cancelled":
      return null;
    case "DestNotEmpty":
      return `${dest} already exists and isn't empty.`;
    case "BranchNotFound":
      return `Branch ${branch} not found on the remote.`;
    case "InvalidUrl":
      return "Unsupported URL. Use https://, ssh://, git@host:path or a local path.";
    case "CredentialBearingUrl":
      return "Remove the password or token from the URL and put it in the token field.";
    case "MissingCredential":
      return "This repository needs a username and token.";
    case "SshAgentFailure":
      return "SSH agent unavailable or has no matching key.";
    default:
      return error.message;
  }
}

export function CloneDialog({
  client,
  onCloned,
  onCancel,
}: {
  client: RepoClient;
  onCloned: (outcome: CloneOutcome, name: string) => void;
  onCancel: () => void;
}) {
  const [url, setUrl] = useState("");
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const [nameEdited, setNameEdited] = useState(false);
  const [branch, setBranch] = useState("");
  const [recurseSubmodules, setRecurseSubmodules] = useState(true);
  const [httpsOpen, setHttpsOpen] = useState(false);
  const [username, setUsername] = useState("");
  const [token, setToken] = useState("");
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<Progress>(null);
  const [error, setError] = useState<string | null>(null);
  const operationId = useRef<string | null>(null);

  const dest = parent.trim() !== "" && name.trim() !== "" ? joinPath(parent.trim(), name.trim()) : "";
  const showHttps = isHttpUrl(url);

  useEffect(
    () =>
      client.subscribeTransferProgress((event) => {
        if (event.operationId !== operationId.current) return;
        if (event.phase === "Receiving") setProgress({ phase: "Receiving", current: event.current, total: event.total });
        else if (event.phase === "Updating") setProgress({ phase: "Updating" });
      }),
    [client],
  );

  const changeUrl = (value: string) => {
    setUrl(value);
    if (!nameEdited) setName(deriveRepoName(value));
  };

  const browse = () => {
    void client.pickRepoFolder().then((path) => {
      if (path !== null) setParent(path);
    });
  };

  const submit = () => {
    const id = crypto.randomUUID();
    operationId.current = id;
    setRunning(true);
    setProgress(null);
    setError(null);
    const trimmedBranch = branch.trim();
    const https = showHttps && username.trim() !== "" && token !== "" ? { username: username.trim(), token } : null;
    client
      .cloneRepo(id, { url: url.trim(), dest, branch: trimmedBranch === "" ? null : trimmedBranch, recurseSubmodules, https })
      .then((outcome) => onCloned(outcome, name.trim()))
      .catch((raw: unknown) => {
        const failure = raw instanceof CloneError ? raw : new CloneError("TransferFailed", String(raw));
        if (failure.kind === "MissingCredential" || failure.kind === "CredentialBearingUrl") setHttpsOpen(true);
        setError(messageFor(failure, dest, trimmedBranch));
      })
      .finally(() => {
        operationId.current = null;
        setRunning(false);
        setProgress(null);
      });
  };

  return (
    <FormDialog ariaLabel="Clone repository" onCancel={running ? () => {} : onCancel}>
      <form
        className={styles.form}
        onSubmit={(event) => {
          event.preventDefault();
          if (!running && url.trim() !== "" && dest !== "") submit();
        }}
      >
        <label>
          Repository URL
          <input value={url} disabled={running} onChange={(e) => changeUrl(e.target.value)} autoFocus />
        </label>
        <div className={styles.row}>
          <label className={styles.grow}>
            Parent folder
            <input value={parent} disabled={running} onChange={(e) => setParent(e.target.value)} />
          </label>
          <button type="button" disabled={running} onClick={browse}>
            Browse…
          </button>
        </div>
        <label>
          Folder name
          <input
            value={name}
            disabled={running}
            onChange={(e) => {
              setName(e.target.value);
              setNameEdited(true);
            }}
          />
        </label>
        {dest !== "" && <p className={styles.dest}>{dest}</p>}
        <label>
          Branch (optional)
          <input value={branch} disabled={running} onChange={(e) => setBranch(e.target.value)} />
        </label>
        <label className={styles.check}>
          <input
            type="checkbox"
            checked={recurseSubmodules}
            disabled={running}
            onChange={(e) => setRecurseSubmodules(e.target.checked)}
          />
          Include submodules
        </label>
        {showHttps && (
          <section className={styles.https}>
            <button type="button" aria-expanded={httpsOpen} onClick={() => setHttpsOpen((open) => !open)}>
              HTTPS credentials (optional)
            </button>
            {httpsOpen && (
              <>
                <label>
                  Username
                  <input value={username} disabled={running} onChange={(e) => setUsername(e.target.value)} autoComplete="off" />
                </label>
                <label>
                  Token
                  <input type="password" value={token} disabled={running} onChange={(e) => setToken(e.target.value)} autoComplete="off" />
                </label>
                <p className={styles.hint}>Saved to your system keychain only if the clone succeeds.</p>
              </>
            )}
          </section>
        )}
        {error !== null && <InlineError message={error} onDismiss={() => setError(null)} />}
        {running && (
          <p className={styles.progress} aria-live="polite">
            {progress?.phase === "Updating"
              ? "Checking out…"
              : progress?.phase === "Receiving" && progress.total > 0
                ? `Receiving objects ${progress.current}/${progress.total}`
                : "Connecting…"}
          </p>
        )}
        <Toolbar>
          {running ? (
            <button type="button" onClick={() => operationId.current !== null && void client.cancelClone(operationId.current)}>
              Cancel clone
            </button>
          ) : (
            <>
              <button type="submit" disabled={url.trim() === "" || dest === ""}>
                Clone
              </button>
              <button type="button" onClick={onCancel}>
                Cancel
              </button>
            </>
          )}
        </Toolbar>
      </form>
    </FormDialog>
  );
}
```

`CloneDialog.module.css`, using the existing design tokens (copy the variable names from `ResetDialog.module.css` / `FormDialog.module.css`):

```css
.form { display: flex; flex-direction: column; gap: 8px; min-width: min(480px, 90vw); }
.form label { display: flex; flex-direction: column; gap: 4px; }
.row { display: flex; gap: 8px; align-items: flex-end; }
.grow { flex: 1; }
.check { flex-direction: row !important; align-items: center; }
.dest, .hint, .progress { margin: 0; font-size: 0.85em; opacity: 0.8; word-break: break-all; }
.https { display: flex; flex-direction: column; gap: 6px; }
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run src/components/CloneDialog.test.tsx src/lib/cloneNames.test.ts` — Expected: PASS.
Run: `cd frontend && pnpm lint` — Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/CloneDialog.tsx frontend/src/components/CloneDialog.module.css frontend/src/components/CloneDialog.test.tsx frontend/src/lib/cloneNames.ts frontend/src/lib/cloneNames.test.ts
git commit -m "feat(frontend): clone dialog with progress, cancel and inline errors"
```

---

### Task 8: Entry points — RepoPicker, New repository, palette, app toasts

**Files:**
- Modify: `frontend/src/components/RepoPicker.tsx` (toolbar at line 99-102, new props)
- Modify: `frontend/src/App.tsx` (root `App`: state for clone dialog/new-repo flow, root `useToasts` + `ToastRegion`; pass handlers into both `<RepoPicker>` usages at ~877 and ~898 and into `<RepoWorkspace>` ~925; `RepoWorkspace` forwards them to `buildCommands`)
- Modify: `frontend/src/lib/commands.ts:136-148` (`buildCommands` gains a trailing `appCommands?: AppCommands` param)
- Create: `frontend/src/lib/newRepository.ts`
- Test: `frontend/src/components/RepoPicker.test.tsx`, `frontend/src/lib/commands.test.ts` (or wherever `buildCommands` is tested: `grep -rl "buildCommands(" frontend/src --include=*.test.*`), `frontend/src/lib/newRepository.test.ts`

**Interfaces:**
- Consumes: `CloneDialog` (Task 7), `RepoClient.{supportsCloneAndInit, initRepo, pickRepoFolder}`, `CloneError`, `ConfirmDialog` (`ariaLabel`, `message`, `confirmLabel`, `onConfirm`, `onCancel`), `useToasts().push(message)`, `ToastRegion`.
- Produces:
  ```ts
  // RepoPicker new optional props:
  onCloneRepository?: () => void; onNewRepository?: () => void;
  // commands.ts
  export interface AppCommands { onCloneRepository: () => void; onNewRepository: () => void; }
  // newRepository.ts
  export type NewRepositoryResult = { kind: "created"; path: string } | { kind: "exists"; path: string } | { kind: "cancelled" };
  export async function createNewRepository(client: RepoClient): Promise<NewRepositoryResult>;
  export function cloneToastMessage(outcome: CloneOutcome, name: string): string;
  ```

- [ ] **Step 1: Write the failing tests**

`frontend/src/lib/newRepository.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { CloneError, type RepoClient } from "../ipc/RepoClient";
import { cloneToastMessage, createNewRepository } from "./newRepository";

function client(picked: string | null, init: () => Promise<void>) {
  return { pickRepoFolder: vi.fn(async () => picked), initRepo: vi.fn(init) } as unknown as RepoClient;
}

describe("createNewRepository", () => {
  it("is cancelled when no folder is picked", async () => {
    const c = client(null, async () => {});
    expect(await createNewRepository(c)).toEqual({ kind: "cancelled" });
    expect(c.initRepo).not.toHaveBeenCalled();
  });

  it("inits the picked folder", async () => {
    expect(await createNewRepository(client("/src/new", async () => {}))).toEqual({ kind: "created", path: "/src/new" });
  });

  it("reports an existing repository instead of failing", async () => {
    const c = client("/src/old", async () => {
      throw new CloneError("AlreadyRepository", "already");
    });
    expect(await createNewRepository(c)).toEqual({ kind: "exists", path: "/src/old" });
  });

  it("rethrows other failures", async () => {
    const c = client("/src/x", async () => {
      throw new CloneError("InitFailed", "disk full");
    });
    await expect(createNewRepository(c)).rejects.toThrow("disk full");
  });
});

describe("cloneToastMessage", () => {
  const ok = { path: "/p", submoduleFailures: [], credentialSaveFailed: false };
  it("names the repository", () => expect(cloneToastMessage(ok, "repo")).toBe("Cloned repo"));
  it("counts failed submodules", () =>
    expect(cloneToastMessage({ ...ok, submoduleFailures: ["a", "b"] }, "repo")).toBe("Cloned repo — 2 submodules failed to update"));
  it("mentions an unsaved token", () =>
    expect(cloneToastMessage({ ...ok, credentialSaveFailed: true }, "repo")).toBe("Cloned repo, but the token couldn't be saved."));
});
```

Append to `RepoPicker.test.tsx` (its `fakeClient` gained `supportsCloneAndInit` in Task 6):

```tsx
describe("clone and new repository entry points", () => {
  it("shows Clone… and New repository… when handlers are given", () => {
    const onClone = vi.fn();
    const onNew = vi.fn();
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
        onCloneRepository={onClone}
        onNewRepository={onNew}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Clone…" }));
    fireEvent.click(screen.getByRole("button", { name: "New repository…" }));
    expect(onClone).toHaveBeenCalled();
    expect(onNew).toHaveBeenCalled();
  });

  it("hides them without handlers", () => {
    render(
      <RepoPicker
        client={fakeClient({ listRecentRepos: async () => [] })}
        onOpenRepo={vi.fn()}
        onOpenWorkspace={vi.fn()}
        workspaces={[]}
        workspacesLoading={false}
        workspacesError={null}
        onDismissWorkspacesError={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onEditWorkspace={vi.fn()}
        onDeleteWorkspace={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Clone…" })).not.toBeInTheDocument();
  });
});
```

In the `buildCommands` test file, following its existing setup for `appState`:

```ts
it("adds clone and new-repository commands when app commands are given", () => {
  const onCloneRepository = vi.fn();
  const onNewRepository = vi.fn();
  const commands = buildCommands(appState, undefined, [], undefined, undefined, undefined, undefined, undefined, {
    onCloneRepository,
    onNewRepository,
  });
  commands.find((c) => c.label === "Clone repository…")!.run();
  commands.find((c) => c.label === "New repository…")!.run();
  expect(onCloneRepository).toHaveBeenCalled();
  expect(onNewRepository).toHaveBeenCalled();
});

it("omits them without app commands", () => {
  const labels = buildCommands(appState).map((c) => c.label);
  expect(labels).not.toContain("Clone repository…");
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/newRepository.test.ts src/components/RepoPicker.test.tsx` plus the commands test file — Expected: FAIL.

- [ ] **Step 3: Implement `newRepository.ts`**

```ts
import { CloneError, type CloneOutcome, type RepoClient } from "../ipc/RepoClient";

export type NewRepositoryResult =
  | { kind: "created"; path: string }
  | { kind: "exists"; path: string }
  | { kind: "cancelled" };

/** Folder picker (the native dialog can create folders), then `git init` there. */
export async function createNewRepository(client: RepoClient): Promise<NewRepositoryResult> {
  const path = await client.pickRepoFolder();
  if (path === null) return { kind: "cancelled" };
  try {
    await client.initRepo(path);
    return { kind: "created", path };
  } catch (error) {
    if (error instanceof CloneError && error.kind === "AlreadyRepository") return { kind: "exists", path };
    throw error;
  }
}

export function cloneToastMessage(outcome: CloneOutcome, name: string): string {
  if (outcome.submoduleFailures.length > 0) {
    return `Cloned ${name} — ${outcome.submoduleFailures.length} submodules failed to update`;
  }
  if (outcome.credentialSaveFailed) return `Cloned ${name}, but the token couldn't be saved.`;
  return `Cloned ${name}`;
}
```

- [ ] **Step 4: RepoPicker buttons**

Add `onCloneRepository?: () => void; onNewRepository?: () => void;` to the props type and destructuring. Toolbar:

```tsx
        <Toolbar>
          <button onClick={handleOpenFolder}>Open folder</button>
          {onCloneRepository !== undefined && <button onClick={onCloneRepository}>Clone…</button>}
          {onNewRepository !== undefined && <button onClick={onNewRepository}>New repository…</button>}
          <button onClick={() => setCreatingWorkspace(true)}>Open workspace root</button>
        </Toolbar>
```

- [ ] **Step 5: Palette commands**

In `commands.ts`:

```ts
export interface AppCommands {
  onCloneRepository: () => void;
  onNewRepository: () => void;
}
```

Add a trailing parameter `appCommands?: AppCommands` to `buildCommands` (after `diffCommands`). Push these right after the `show-shortcuts` command:

```ts
  if (appCommands !== undefined) {
    commands.push(
      {
        id: "clone-repository",
        label: "Clone repository…",
        keywords: ["clone", "repository", "url", "remote"],
        run: appCommands.onCloneRepository,
      },
      {
        id: "new-repository",
        label: "New repository…",
        keywords: ["new", "init", "create", "repository"],
        run: appCommands.onNewRepository,
      },
    );
  }
```

- [ ] **Step 6: Wire `App.tsx`**

In the root `App` (next to `pickingRepo` at ~669):

```tsx
  const appToasts = useToasts();
  const [cloning, setCloning] = useState(false);
  const [existingRepoPrompt, setExistingRepoPrompt] = useState<string | null>(null);
  const { push: pushAppToast } = appToasts;
  const startClone = useCallback(() => {
    setPickingRepo(false);
    setCloning(true);
  }, []);
  const startNewRepository = useCallback(() => {
    setPickingRepo(false);
    createNewRepository(client).then(
      (result) => {
        if (result.kind === "created") {
          void openRepoTab(result.path);
          pushAppToast(`Created repository ${repoNameOf(result.path)}`);
        } else if (result.kind === "exists") {
          setExistingRepoPrompt(result.path);
        }
      },
      (error: unknown) => setOpenError({ message: String(error instanceof Error ? error.message : error), path: "" }),
    );
  }, [client, openRepoTab, pushAppToast]);
  const appCommands = useMemo(
    () => (client.supportsCloneAndInit ? { onCloneRepository: startClone, onNewRepository: startNewRepository } : undefined),
    [client.supportsCloneAndInit, startClone, startNewRepository],
  );
```

`repoNameOf`: move the helper from `RepoPicker.tsx` into `frontend/src/lib/repoName.ts` (export it, and import it in both places). Don't duplicate it.

The `openError` Retry button calls `openRepoTab(openError.path)`. For an init failure the path is `""`, so render `onRetry` only when `openError.path !== ""`.

Render, next to the `pickingRepo` overlay:

```tsx
      {cloning && (
        <Overlay onClose={() => setCloning(false)}>
          <CloneDialog
            client={client}
            onCancel={() => setCloning(false)}
            onCloned={(outcome, name) => {
              setCloning(false);
              void openRepoTab(outcome.path);
              pushAppToast(cloneToastMessage(outcome, name));
            }}
          />
        </Overlay>
      )}
      {existingRepoPrompt !== null && (
        <ConfirmDialog
          ariaLabel="Repository already exists"
          message={<p>{existingRepoPrompt} is already a repository. Open it?</p>}
          confirmLabel="Open"
          onConfirm={() => {
            void openRepoTab(existingRepoPrompt);
            setExistingRepoPrompt(null);
          }}
          onCancel={() => setExistingRepoPrompt(null)}
        />
      )}
      <ToastRegion toasts={appToasts.toasts} onDismiss={appToasts.dismiss} />
```

Pass `onCloneRepository={appCommands?.onCloneRepository}` and `onNewRepository={appCommands?.onNewRepository}` to **both** `<RepoPicker>` usages. Add an `appCommands?: AppCommands` prop to `RepoWorkspace`, pass `appCommands={appCommands}` at ~925, and append `appCommands` as the last argument of its `buildCommands(...)` call (~325).

A running clone must not be dismissed by the Overlay's Escape/backdrop (the clone would continue invisibly). Add an optional `onRunningChange?: (running: boolean) => void` prop to `CloneDialog`, called with `true` in `submit()` right after `setRunning(true)` and with `false` in the `.finally`. Test it in `CloneDialog.test.tsx`:

```tsx
  it("reports running state to its host", async () => {
    const onRunningChange = vi.fn();
    const { client } = fakeClient();
    render(<CloneDialog client={client} onCloned={vi.fn()} onCancel={vi.fn()} onRunningChange={onRunningChange} />);
    fill("https://github.com/org/repo.git");
    fireEvent.click(screen.getByRole("button", { name: "Clone" }));
    expect(onRunningChange).toHaveBeenCalledWith(true);
    await waitFor(() => expect(onRunningChange).toHaveBeenLastCalledWith(false));
  });
```

In App, keep `const cloneRunning = useRef(false);`. Pass `onRunningChange={(running) => (cloneRunning.current = running)}`, and make the Overlay `onClose={() => { if (!cloneRunning.current) setCloning(false); }}`.

- [ ] **Step 7: Verify**

Run: `cd frontend && pnpm test -- --run` — Expected: all PASS.
Run: `cd frontend && pnpm exec tsc --noEmit -p . && pnpm lint && pnpm build` — Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): clone and new repository entry points"
```

---

### Task 9: E2E, docs, changelog

**Files:**
- Create: `e2e/specs/clone.spec.ts`
- Modify: `docs/USER_GUIDE.md` (new section after the repository-opening section)
- Modify: `CHANGELOG.md` (`## [Unreleased]`, new `### Clone and new repository` subsection on top)

**Interfaces:**
- Consumes: UI labels from Tasks 7-8 ("Open another repository", "Clone…", "Repository URL", "Parent folder", "Folder name", "Clone").

- [ ] **Step 1: Write the E2E spec**

`e2e/specs/clone.spec.ts`:

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const SOURCE = path.join(os.tmpdir(), "browsitory-e2e-clone-source.git");
const PARENT = path.join(os.tmpdir(), "browsitory-e2e-clones");
const NAME = "e2e-cloned";

async function setInput(label: string, value: string) {
  const input = await $(`aria/${label}`);
  await input.waitForExist({ timeout: 10000 });
  await input.setValue(value);
}

describe("Browsitory clone", () => {
  before(() => {
    fs.rmSync(SOURCE, { recursive: true, force: true });
    fs.rmSync(PARENT, { recursive: true, force: true });
    fs.mkdirSync(PARENT, { recursive: true });
    execFileSync("git", ["clone", "--bare", E2E_REPO_PATH, SOURCE], { stdio: "inherit" });
  });

  it("clones a repository from the picker and opens it in a new tab", async () => {
    await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));
    const cloneButton = await $("button=Clone…");
    await cloneButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), cloneButton);

    await setInput("Repository URL", `file://${SOURCE.replace(/\\/g, "/")}`);
    await setInput("Parent folder", PARENT);
    const nameInput = await $("aria/Folder name");
    await nameInput.clearValue();
    await nameInput.setValue(NAME);
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Clone"));

    const tab = await $(`[role="tab"]*=${NAME}`);
    await tab.waitForExist({ timeout: 30000 });
    expect(fs.existsSync(path.join(PARENT, NAME, ".git"))).toBe(true);
    const readmeCommit = await $("li*=Initial commit");
    await readmeCommit.waitForExist({ timeout: 15000 });
  });
});
```

Replace `"Initial commit"` with the fixture's actual first commit message. Find it in `e2e/wdio.conf.ts`, where the fixture repo is created (~line 64). If WebdriverIO's `aria/` selector doesn't resolve label text for these inputs, use `//label[contains(., 'Repository URL')]//input`.

- [ ] **Step 2: Run E2E**

Run (from repo root, per CLAUDE.md):
```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm test -- --spec specs/clone.spec.ts
```
Expected: PASS. Then run the full `pnpm test` in `e2e/`: Expected all PASS (the new root `ToastRegion` must not break existing toast selectors).

- [ ] **Step 3: Docs**

`docs/USER_GUIDE.md`, new section:

```markdown
## Cloning and creating repositories

Desktop app only — in VSCode, use VSCode's own **Git: Clone** and **Git: Initialize Repository**.

**Clone…** (repository picker, or the palette's "Clone repository…"):

- **URL** — `https://…`, `ssh://…`, `git@host:org/repo.git`, or a local path / `file://` URL.
- **Parent folder** and **Folder name** — the clone goes into *parent/name*. The name follows
  the URL until you edit it. The folder must not exist yet, or be empty.
- **Branch** — optional; defaults to the remote's default branch.
- **Include submodules** — on by default. A submodule that can't be fetched doesn't stop the
  clone; the toast says how many failed and the Submodules panel shows them.
- **HTTPS credentials** — for private HTTPS repositories, enter a username and access token.
  The token is saved to your system keychain, and the remote set to use it, **only after the
  clone succeeds**. SSH URLs use your SSH agent. Don't put the token in the URL — Browsitory
  refuses URLs that contain a password.

Progress shows while cloning; **Cancel clone** stops it and removes what was downloaded.

**New repository…** — pick (or create) a folder; Browsitory runs the equivalent of `git init`
there, on your `init.defaultBranch` (or `main`), and opens it. Picking a folder that already
is a repository offers to open it instead.
```

`CHANGELOG.md` under `## [Unreleased]`:

```markdown
### Clone and new repository

- **Clone…** in the repository picker and a "Clone repository…" palette command (desktop app):
  clone from HTTPS, SSH or a local path with progress and cancel, an optional branch, and
  submodules. An HTTPS token entered in the dialog is saved to the keychain only after a
  successful clone.
- **New repository…** creates an empty repository in a folder you pick and opens it.
```

- [ ] **Step 4: Full verification**

```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all clean and passing (the extension must still compile against the widened `RepoClient`).

- [ ] **Step 5: Commit**

```bash
git add e2e/specs/clone.spec.ts docs/USER_GUIDE.md CHANGELOG.md
git commit -m "test(e2e): clone flow; docs: cloning guide and changelog"
```
