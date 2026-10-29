//! Clone a remote into a new working directory, with the same credential, progress and cancel
//! seams as `remote::fetch_remote`. See
//! `docs/superpowers/specs/2026-10-17-parity-batch3a-design.md`.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use git2::build::{CheckoutBuilder, RepoBuilder};
use git2::{CheckoutNotificationType, ErrorClass, ErrorCode, FetchOptions, RemoteCallbacks};
use thiserror::Error;
use url::Url;

use crate::remote::{
    CredentialProvider, RemoteError, TransferErrorKind, TransferOperation, TransferPhase,
    TransferProgress, TransferReporter,
};
use crate::submodule::{init_submodule, list_submodules, update_submodule};
use crate::worktree_guard::{find_case_collision, CasePolicy, GuardSource};

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
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
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
            let Ok(entries) = std::fs::read_dir(dest) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let is_real_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let _ = if is_real_dir {
                    std::fs::remove_dir_all(&path)
                } else {
                    // A Windows folder link or junction is removed with `RemoveDirectoryW`
                    // (the link itself, never its target).
                    std::fs::remove_file(&path).or_else(|_| std::fs::remove_dir(&path))
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
    clone_repo_with(opts, credentials, reporter, cancel, CasePolicy::FromRepo)
}

/// [`clone_repo`] with the case-collision policy as a parameter (tests force it on Linux).
pub(crate) fn clone_repo_with(
    opts: &CloneOptions,
    credentials: &mut dyn CredentialProvider,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
    policy: CasePolicy,
) -> Result<CloneOutcome, CloneError> {
    validate_clone_url(&opts.url)?;
    if !opts.dest.is_absolute() {
        return Err(CloneError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Destination must be an absolute path.",
        )));
    }
    let url = opts.url.trim();
    let branch = opts
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty());
    if cancel() {
        return Err(CloneError::Cancelled);
    }
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

        // No checkout yet: the tree is checked for case collisions first (#156).
        let mut no_checkout = CheckoutBuilder::new();
        no_checkout.dry_run();

        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch).with_checkout(no_checkout);
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

    if let Err(error) = checkout_clone(&repo, &reporter, cancel, policy) {
        drop(repo);
        cleanup(&opts.dest, state);
        return Err(match error {
            CheckoutFailure::Case(collision) => collision.into(),
            CheckoutFailure::Git(error) => classify(error, false, cancel),
        });
    }

    let submodule_failures = if cancel() {
        None
    } else if opts.recurse_submodules {
        update_all_submodules(&repo, cancel)
    } else {
        Some(Vec::new())
    };
    let Some(submodule_failures) = submodule_failures else {
        drop(repo);
        cleanup(&opts.dest, state);
        return Err(CloneError::Cancelled);
    };
    Ok(CloneOutcome {
        path: opts.dest.clone(),
        submodule_failures,
    })
}

enum CheckoutFailure {
    Case(crate::CaseCollision),
    Git(git2::Error),
}

impl From<git2::Error> for CheckoutFailure {
    fn from(error: git2::Error) -> Self {
        Self::Git(error)
    }
}

/// Checks the fresh clone's HEAD tree for a case collision, then checks it out into the empty
/// work tree as the clone would have (forced), with progress and cancel. An unborn HEAD (empty
/// remote) has nothing to check out.
fn checkout_clone(
    repo: &git2::Repository,
    reporter: &RefCell<&mut dyn TransferReporter>,
    cancel: &dyn Fn() -> bool,
    policy: CasePolicy,
) -> Result<(), CheckoutFailure> {
    let Ok(tree) = repo.head().and_then(|head| head.peel_to_tree()) else {
        return Ok(());
    };
    if policy.applies(repo) {
        const ALL: Option<&[&str]> = None;
        if let Some(collision) = find_case_collision(&GuardSource::Tree(&tree), ALL)? {
            return Err(CheckoutFailure::Case(collision));
        }
    }
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    // Every written file is an `UPDATED` notification; returning false aborts the checkout.
    checkout.notify_on(CheckoutNotificationType::UPDATED);
    checkout.notify(|_kind, _path, _baseline, _target, _workdir| !cancel());
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
    repo.checkout_head(Some(&mut checkout))?;
    Ok(())
}

fn classify(error: git2::Error, branch_requested: bool, cancel: &dyn Fn() -> bool) -> CloneError {
    if cancel() {
        return CloneError::Cancelled;
    }
    if branch_requested
        && error.code() == ErrorCode::NotFound
        && error.class() == ErrorClass::Reference
    {
        return CloneError::BranchNotFound;
    }
    let message = error.message().to_string();
    let kind = RemoteError::Git(error).transfer_error_kind();
    CloneError::Transfer { kind, message }
}

/// Best-effort: a submodule that can't be fetched (private URL, refused scheme) must not undo
/// an otherwise good clone. Failures are reported by path. `None` means the user cancelled
/// between submodules.
fn update_all_submodules(
    repo: &git2::Repository,
    cancel: &dyn Fn() -> bool,
) -> Option<Vec<String>> {
    let submodules = match list_submodules(repo) {
        Ok(submodules) => submodules,
        Err(_) => return Some(vec![".gitmodules".to_string()]),
    };
    let mut failures = Vec::new();
    for submodule in submodules {
        if cancel() {
            return None;
        }
        if init_submodule(repo, &submodule.path)
            .and_then(|()| update_submodule(repo, &submodule.path, true))
            .is_err()
        {
            failures.push(submodule.path);
        }
    }
    Some(failures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Repository;
    use tempfile::TempDir;

    struct NoCredentials;
    impl CredentialProvider for NoCredentials {
        fn credential(
            &mut self,
            _url: &str,
            _username: Option<&str>,
            _allowed: git2::CredentialType,
        ) -> Result<git2::Cred, git2::Error> {
            Err(git2::Error::from_str("no credentials"))
        }
    }

    #[derive(Default)]
    struct Phases(Vec<TransferPhase>);
    impl TransferReporter for Phases {
        fn report(&mut self, event: TransferProgress) {
            self.0.push(event.phase);
        }
    }

    /// A source repository whose `main` holds `a.txt`, plus `D -> /x` and `d/x` when `pair`.
    fn source(pair: bool) -> TempDir {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut root = repo.treebuilder(None).unwrap();
        root.insert("a.txt", repo.blob(b"a\n").unwrap(), 0o100644)
            .unwrap();
        if pair {
            let mut inner = repo.treebuilder(None).unwrap();
            inner
                .insert("x", repo.blob(b"PWNED\n").unwrap(), 0o100644)
                .unwrap();
            root.insert("d", inner.write().unwrap(), 0o040000).unwrap();
            root.insert("D", repo.blob(b"/x").unwrap(), 0o120000)
                .unwrap();
        }
        let tree = repo.find_tree(root.write().unwrap()).unwrap();
        let sig = git2::Signature::now("T", "t@example.com").unwrap();
        repo.commit(Some("refs/heads/main"), &sig, &sig, "c", &tree, &[])
            .unwrap();
        repo.set_head("refs/heads/main").unwrap();
        dir
    }

    fn clone(
        src: &Path,
        dest: PathBuf,
        reporter: &mut dyn TransferReporter,
        cancel: &dyn Fn() -> bool,
    ) -> Result<CloneOutcome, CloneError> {
        let opts = CloneOptions {
            url: src.to_string_lossy().into_owned(),
            dest,
            branch: None,
            recurse_submodules: false,
        };
        clone_repo_with(
            &opts,
            &mut NoCredentials,
            reporter,
            cancel,
            CasePolicy::Always,
        )
    }

    #[test]
    fn cloning_a_repo_with_a_case_collision_fails_and_cleans_up() {
        let src = source(true);
        let parent = TempDir::new().unwrap();
        let created = parent.path().join("created");
        let error = clone(src.path(), created.clone(), &mut Phases::default(), &|| {
            false
        })
        .unwrap_err();
        assert!(matches!(error, CloneError::CaseCollision(_)), "{error}");
        assert!(!created.exists());

        let existing = parent.path().join("existing");
        std::fs::create_dir(&existing).unwrap();
        let error = clone(
            src.path(),
            existing.clone(),
            &mut Phases::default(),
            &|| false,
        )
        .unwrap_err();
        assert!(matches!(error, CloneError::CaseCollision(_)));
        assert_eq!(std::fs::read_dir(&existing).unwrap().count(), 0);
    }

    #[test]
    fn cloning_a_clean_repo_still_checks_out_and_reports_progress() {
        let src = source(false);
        let parent = TempDir::new().unwrap();
        let dest = parent.path().join("clone");
        let mut phases = Phases::default();
        let outcome = clone(src.path(), dest.clone(), &mut phases, &|| false).unwrap();
        assert_eq!(outcome.path, dest);
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "a\n");
        assert!(phases.0.contains(&TransferPhase::Updating));
        let repo = Repository::open(&dest).unwrap();
        assert!(crate::status::status(&repo).unwrap().is_empty());
    }

    #[test]
    fn cancel_during_checkout_still_cancels() {
        let src = source(false);
        let parent = TempDir::new().unwrap();
        let dest = parent.path().join("clone");
        let polls = std::cell::Cell::new(0);
        // Allows the pre-start check, then cancels on every later poll.
        let cancel = || {
            polls.set(polls.get() + 1);
            polls.get() > 1
        };
        let error = clone(src.path(), dest.clone(), &mut Phases::default(), &cancel).unwrap_err();
        assert!(matches!(error, CloneError::Cancelled), "{error}");
        assert!(!dest.exists());
    }
}
