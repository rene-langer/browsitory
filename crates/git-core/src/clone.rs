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
use crate::submodule::{init_submodule, list_submodules, update_submodule};

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
            let Ok(entries) = std::fs::read_dir(dest) else {
                return;
            };
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
    Ok(CloneOutcome {
        path: opts.dest.clone(),
        submodule_failures,
    })
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
