//! Repository snapshots for the undo stack: capture the state an operation changes, and restore
//! one snapshot over another after checking nothing else touched the same state. See
//! `docs/superpowers/specs/2026-10-18-sm-parity-batch3b-design.md`.
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use git2::{Index, IndexEntry, IndexTime, Oid, Repository, Status, StatusOptions};
use thiserror::Error;

const MODE_FILE: u32 = 0o100644;
const MODE_EXEC: u32 = 0o100755;
const MODE_LINK: u32 = 0o120000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadState {
    /// Full ref name, e.g. `refs/heads/main`. May be unborn.
    Branch(String),
    /// Commit oid.
    Detached(String),
}

// git-core stays serde-free; repo-service persists a mirror type (`StoredSnapshot`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSnapshot {
    pub head: HeadState,
    /// `refs/heads/*` and `refs/tags/*` -> oid.
    pub refs: BTreeMap<String, String>,
    pub index_tree: String,
    /// HEAD tree with every modified/deleted/untracked (not ignored) file overlaid. `None` for
    /// `SnapshotScope::RefsAndIndex`.
    pub worktree_tree: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotScope {
    RefsAndIndex,
    Full,
}

#[derive(Debug, Error)]
pub enum UndoError {
    #[error("the repository changed since this operation")]
    RepositoryChanged,
    #[error("a merge, rebase, cherry-pick or revert is in progress")]
    OperationInProgress,
    #[error("the index has conflicts")]
    ConflictedIndex,
    #[error("the repository has no working directory")]
    Bare,
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub fn capture(repo: &Repository, scope: SnapshotScope) -> Result<RepoSnapshot, UndoError> {
    let mut index = repo.index()?;
    index.read(false)?;
    if index.has_conflicts() {
        return Err(UndoError::ConflictedIndex);
    }
    let index_tree = index.write_tree()?.to_string();
    let worktree_tree = match scope {
        SnapshotScope::RefsAndIndex => None,
        SnapshotScope::Full => Some(build_worktree_tree(repo)?.to_string()),
    };
    Ok(RepoSnapshot {
        head: head_state(repo)?,
        refs: local_refs(repo)?,
        index_tree,
        worktree_tree,
    })
}

fn head_state(repo: &Repository) -> Result<HeadState, UndoError> {
    let head = repo.find_reference("HEAD")?;
    if let Some(target) = head.symbolic_target()? {
        return Ok(HeadState::Branch(target.to_string()));
    }
    let oid = head
        .target()
        .ok_or_else(|| git2::Error::from_str("HEAD has no target"))?;
    Ok(HeadState::Detached(oid.to_string()))
}

fn local_refs(repo: &Repository) -> Result<BTreeMap<String, String>, UndoError> {
    let mut refs = BTreeMap::new();
    for reference in repo.references()? {
        let reference = reference?;
        let Ok(name) = reference.name() else { continue };
        let Some(target) = reference.target() else {
            continue;
        };
        if name.starts_with("refs/heads/") || name.starts_with("refs/tags/") {
            refs.insert(name.to_string(), target.to_string());
        }
    }
    Ok(refs)
}

struct WorktreeConfig {
    filemode: bool,
    symlinks: bool,
}

fn worktree_config(repo: &Repository) -> Result<WorktreeConfig, UndoError> {
    let config = repo.config()?;
    Ok(WorktreeConfig {
        filemode: config.get_bool("core.filemode").unwrap_or(cfg!(unix)),
        symlinks: config.get_bool("core.symlinks").unwrap_or(cfg!(unix)),
    })
}

fn build_worktree_tree(repo: &Repository) -> Result<Oid, UndoError> {
    let workdir = repo.workdir().ok_or(UndoError::Bare)?;
    let config = worktree_config(repo)?;
    let mut overlay = Index::new()?;
    if let Ok(head) = repo.head() {
        overlay.read_tree(&head.peel_to_tree()?)?;
    }
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true);
    for entry in repo.statuses(Some(&mut options))?.iter() {
        if entry.status().contains(Status::IGNORED) {
            continue;
        }
        let path = entry.path()?.to_string();
        // An untracked nested repository is reported as "dir/"; it is not ours to snapshot.
        if path.ends_with('/') {
            continue;
        }
        let previous_mode = overlay.get_path(Path::new(&path), 0).map(|e| e.mode);
        match read_worktree_file(&workdir.join(&path), &config, previous_mode)? {
            Some((bytes, mode)) => {
                let id = repo.blob(&bytes)?;
                overlay.add(&index_entry(&path, id, mode))?;
            }
            None => {
                let _ = overlay.remove_path(Path::new(&path));
            }
        }
    }
    Ok(overlay.write_tree_to(repo)?)
}

/// Raw bytes (no clean filters) and git mode of a worktree path; `None` when absent or a
/// directory. `previous_mode` keeps the exec bit when `core.filemode` is off and the symlink
/// mode when `core.symlinks` is off, matching what git itself records.
fn read_worktree_file(
    full: &Path,
    config: &WorktreeConfig,
    previous_mode: Option<u32>,
) -> io::Result<Option<(Vec<u8>, u32)>> {
    let meta = match fs::symlink_metadata(full) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if meta.file_type().is_symlink() {
        return Ok(Some((link_target_bytes(&fs::read_link(full)?), MODE_LINK)));
    }
    if meta.is_dir() {
        return Ok(None);
    }
    let bytes = fs::read(full)?;
    let mode = if !config.symlinks && previous_mode == Some(MODE_LINK) {
        MODE_LINK
    } else if config.filemode {
        if is_executable(&meta) {
            MODE_EXEC
        } else {
            MODE_FILE
        }
    } else if previous_mode == Some(MODE_EXEC) {
        MODE_EXEC
    } else {
        MODE_FILE
    };
    Ok(Some((bytes, mode)))
}

#[cfg(unix)]
fn is_executable(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn link_target_bytes(target: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    target.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn link_target_bytes(target: &Path) -> Vec<u8> {
    target.to_string_lossy().replace('\\', "/").into_bytes()
}

fn index_entry(path: &str, id: Oid, mode: u32) -> IndexEntry {
    IndexEntry {
        ctime: IndexTime::new(0, 0),
        mtime: IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode,
        uid: 0,
        gid: 0,
        file_size: 0,
        id,
        flags: path.len().min(0xfff) as u16,
        flags_extended: 0,
        path: path.as_bytes().to_vec(),
    }
}
