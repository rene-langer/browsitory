//! Repository snapshots for the undo stack: capture the state an operation changes, and restore
//! one snapshot over another after checking nothing else touched the same state. See
//! `docs/superpowers/specs/2026-10-18-parity-batch3b-design.md`.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use git2::{
    ErrorCode, Index, IndexEntry, IndexTime, ObjectType, Oid, Repository, Status, StatusOptions,
    TreeWalkMode, TreeWalkResult,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::worktree_guard::{Content, Leaf};

const MODE_FILE: u32 = 0o100644;
const MODE_EXEC: u32 = 0o100755;
pub(crate) const MODE_LINK: u32 = 0o120000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadState {
    /// Full ref name, e.g. `refs/heads/main`. May be unborn.
    Branch(String),
    /// Commit oid.
    Detached(String),
}

// No serde here; repo-service persists a mirror type (`StoredSnapshot`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSnapshot {
    pub head: HeadState,
    /// `refs/heads/*` and `refs/tags/*` -> oid.
    pub refs: BTreeMap<String, String>,
    pub index_tree: String,
    /// The index's intent-to-add paths (`git add -N`, #1174): `index_tree` holds them as empty
    /// files, and `restore` puts the mark back.
    pub intent_to_add: BTreeSet<String>,
    /// Each local branch's settings (`branch.<name>.*` in the repository's own config file, #1151)
    /// by branch name: `(variable, value)` in file order. Only branches with settings.
    pub branch_config: BTreeMap<String, Vec<(String, String)>>,
    /// HEAD tree with every modified/deleted/untracked (not ignored) file overlaid. `None` for
    /// `SnapshotScope::RefsAndIndex`.
    pub worktree_tree: Option<String>,
    /// Work-tree files over the size limit, by path (#1065): left out of `worktree_tree` (not
    /// stored), only identified by their stat data.
    pub skipped: BTreeMap<String, SkippedFile>,
    /// Blob listing the untracked files a snapshot with more than `SNAPSHOT_UNTRACKED_LIMIT` of
    /// them left out of `worktree_tree` (#1090), each identified like a [`SkippedFile`]; read
    /// with [`unsaved_untracked`]. One blob, not a map, so a folder of tens of thousands of
    /// files costs each snapshot an object id in memory and in `undo.json`, and identical lists
    /// share one object.
    pub unsaved_untracked: Option<String>,
}

/// The largest work-tree file a `SnapshotScope::Full` snapshot stores (#1065).
pub const SNAPSHOT_FILE_LIMIT: u64 = 50 * 1024 * 1024;

/// The most untracked files a `SnapshotScope::Full` snapshot stores (#1090): with more, it stores
/// none of them, only identifies them (`RepoSnapshot::unsaved_untracked`).
pub const SNAPSHOT_UNTRACKED_LIMIT: usize = 10_000;

/// A work-tree file a snapshot did not store for its size (#1065): what `restore` compares the
/// file on disk with to tell whether it is still the one the snapshot saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkippedFile {
    pub size: u64,
    /// Seconds and nanoseconds since the epoch.
    pub mtime: (i64, u32),
    /// 0 where the platform has none (Windows).
    pub ino: u64,
    /// Its git mode, as the snapshot would have recorded it.
    pub mode: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotScope {
    RefsAndIndex,
    /// Refs, index and work tree; a work-tree file over `SNAPSHOT_FILE_LIMIT` is not stored, only
    /// identified (`RepoSnapshot::skipped`, #1065), and neither is any untracked file when there
    /// are more than `SNAPSHOT_UNTRACKED_LIMIT` (`RepoSnapshot::unsaved_untracked`, #1090).
    Full,
    /// `Full` with every file stored whatever its size or their number: for the commands that
    /// destroy work-tree content with no other way back (discards, a hard reset, #1065, #1090).
    FullUncapped,
}

#[derive(Debug, Error)]
pub enum UndoError {
    #[error("the repository changed since this operation")]
    RepositoryChanged,
    /// A file the snapshot to restore did not store for its size (#1065) is no longer as it was
    /// then, so it can't be brought back; nothing was changed.
    #[error(
        "{0} is larger than {limit} MB, so its contents weren't saved for undo, and it has changed since.",
        limit = SNAPSHOT_FILE_LIMIT / (1024 * 1024)
    )]
    NotSaved(String),
    /// Like `NotSaved`, for an untracked file the snapshot did not store because there were too
    /// many (#1090).
    #[error(
        "{0} is one of more than {limit} untracked files, so its contents weren't saved for undo, and it has changed since.",
        limit = grouped(SNAPSHOT_UNTRACKED_LIMIT)
    )]
    NotSavedUntracked(String),
    /// A branch to move, delete or check out is held by another worktree: checked out, being
    /// rebased or bisected, or used by a paused cherry-pick or revert there (#291, #293).
    #[error(transparent)]
    Held(#[from] crate::repo::BranchHeld),
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
    /// Windows: another program keeps a file to restore open so it can't be replaced; that
    /// file was left as it was (#251, #268). Other files and the index may already be
    /// restored; trying again once the file is closed finishes the restore.
    #[error("'{0}' is open in another program. Close it, then try again.")]
    FileInUse(String),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
    /// `core.ignorecase` is on and the checkout includes two paths that differ only in case
    /// (#403); nothing was changed.
    #[error(transparent)]
    CasePair(#[from] crate::CasePair),
    /// `core.ignorecase` can't be read, so the case-collision check can't run (#225).
    #[error(transparent)]
    CaseConfig(#[from] crate::CaseConfigUnreadable),
    /// A file to restore uses an external filter driver (#1063); nothing changed.
    #[error(transparent)]
    ExternalFilter(#[from] crate::ExternalFilter),
}

pub fn capture(repo: &Repository, scope: SnapshotScope) -> Result<RepoSnapshot, UndoError> {
    match scope {
        SnapshotScope::RefsAndIndex => capture_with(repo, None),
        SnapshotScope::Full => capture_with(repo, Some(Limits::FULL)),
        SnapshotScope::FullUncapped => capture_with(repo, Some(Limits::NONE)),
    }
}

/// [`capture`] with `SnapshotScope::Full` and a per-file `limit` other than
/// `SNAPSHOT_FILE_LIMIT`: a test seam, so tests need no 50 MB files.
#[doc(hidden)]
pub fn capture_limited(repo: &Repository, limit: u64) -> Result<RepoSnapshot, UndoError> {
    let limits = Limits {
        file_size: Some(limit),
        ..Limits::FULL
    };
    capture_with(repo, Some(limits))
}

/// [`capture`] with `SnapshotScope::Full` and an untracked-file limit other than
/// `SNAPSHOT_UNTRACKED_LIMIT`: a test seam, so tests need no 10,000 files.
#[doc(hidden)]
pub fn capture_counted(
    repo: &Repository,
    untracked_limit: usize,
) -> Result<RepoSnapshot, UndoError> {
    let limits = Limits {
        untracked: Some(untracked_limit),
        ..Limits::FULL
    };
    capture_with(repo, Some(limits))
}

/// [`capture`] for a command that destroys work-tree content (#1104): `whole` (files, and
/// everything below folders, matched ignoring case) are stored whatever their size or number,
/// every other file under `SnapshotScope::Full`'s limits.
pub fn capture_keeping(repo: &Repository, whole: &[String]) -> Result<RepoSnapshot, UndoError> {
    let limits = Limits {
        whole,
        ..Limits::FULL
    };
    capture_with(repo, Some(limits))
}

/// [`capture_keeping`] with limits other than `SNAPSHOT_FILE_LIMIT` and
/// `SNAPSHOT_UNTRACKED_LIMIT`: a test seam.
#[doc(hidden)]
pub fn capture_limited_except(
    repo: &Repository,
    file_limit: u64,
    untracked_limit: usize,
    whole: &[String],
) -> Result<RepoSnapshot, UndoError> {
    let limits = Limits {
        file_size: Some(file_limit),
        untracked: Some(untracked_limit),
        whole,
    };
    capture_with(repo, Some(limits))
}

/// What a work-tree snapshot leaves unsaved: files over `file_size`, and every untracked file
/// when there are more than `untracked` of them; never a file `whole` names (#1104).
#[derive(Clone, Copy)]
struct Limits<'a> {
    file_size: Option<u64>,
    untracked: Option<usize>,
    /// Files, and folders (everything below them), stored whatever the limits say; matched
    /// ignoring case, so a spelling that differs only in case can only store more.
    whole: &'a [String],
}

impl Limits<'_> {
    const FULL: Limits<'static> = Limits {
        file_size: Some(SNAPSHOT_FILE_LIMIT),
        untracked: Some(SNAPSHOT_UNTRACKED_LIMIT),
        whole: &[],
    };
    const NONE: Limits<'static> = Limits {
        file_size: None,
        untracked: None,
        whole: &[],
    };
}

/// Whether the work-tree path `path` is one of `whole` (lower-cased, without a trailing `/`),
/// ignoring case, or lies below one of them.
fn named_whole(whole: &[String], path: &str) -> bool {
    let path = path.to_lowercase();
    whole.iter().any(|name| {
        path.strip_prefix(name.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// The blob behind `RepoSnapshot::unsaved_untracked`: each file as
/// `[size, mtime seconds, mtime nanoseconds, inode, git mode]`.
#[derive(Serialize, Deserialize)]
struct UnsavedList {
    files: BTreeMap<String, (u64, i64, u32, u64, u32)>,
}

/// The untracked files `snapshot` identified instead of storing (#1090), by path.
pub fn unsaved_untracked(
    repo: &Repository,
    snapshot: &RepoSnapshot,
) -> Result<BTreeMap<String, SkippedFile>, UndoError> {
    let Some(id) = &snapshot.unsaved_untracked else {
        return Ok(BTreeMap::new());
    };
    let blob = repo.find_blob(Oid::from_str(id)?)?;
    let list: UnsavedList = serde_json::from_slice(blob.content())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(list
        .files
        .into_iter()
        .map(|(path, (size, secs, nanos, ino, mode))| {
            let file = SkippedFile {
                size,
                mtime: (secs, nanos),
                ino,
                mode,
            };
            (path, file)
        })
        .collect())
}

fn write_unsaved_list(
    repo: &Repository,
    files: &BTreeMap<String, SkippedFile>,
) -> Result<Option<String>, UndoError> {
    if files.is_empty() {
        return Ok(None);
    }
    let list = UnsavedList {
        files: files
            .iter()
            .map(|(path, file)| {
                let fields = (file.size, file.mtime.0, file.mtime.1, file.ino, file.mode);
                (path.clone(), fields)
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&list).map_err(io::Error::other)?;
    Ok(Some(repo.blob(&bytes)?.to_string()))
}

/// `n` with a comma between each group of three digits.
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `worktree`: `None` for refs and index only, else the work tree too, with its limits.
fn capture_with(repo: &Repository, worktree: Option<Limits>) -> Result<RepoSnapshot, UndoError> {
    let mut index = repo.index()?;
    index.read(false)?;
    if index.has_conflicts() {
        return Err(UndoError::ConflictedIndex);
    }
    let index_tree = index.write_tree()?.to_string();
    let intent_to_add = intent_to_add(&index);
    let (worktree_tree, skipped, unsaved_untracked) = match worktree {
        None => (None, BTreeMap::new(), None),
        Some(limits) => {
            let worktree = build_worktree_tree(repo, limits)?;
            let list = write_unsaved_list(repo, &worktree.unsaved_untracked)?;
            (Some(worktree.tree.to_string()), worktree.skipped, list)
        }
    };
    Ok(RepoSnapshot {
        head: head_state(repo)?,
        refs: local_refs(repo)?,
        index_tree,
        intent_to_add,
        branch_config: branch_settings(repo)?,
        worktree_tree,
        skipped,
        unsaved_untracked,
    })
}

/// The intent-to-add paths of `index` (#1174).
fn intent_to_add(index: &Index) -> BTreeSet<String> {
    crate::index_marks::intent_to_add_paths(index)
        .into_iter()
        .map(|path| String::from_utf8_lossy(&path).into_owned())
        .collect()
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

/// A branch's settings: `(variable, value)` in file order.
type BranchSettings = Vec<(String, String)>;

/// The repository's own config file (`.git/config`), where git keeps branch settings.
fn local_config(repo: &Repository) -> Result<git2::Config, UndoError> {
    Ok(repo.config()?.open_level(git2::ConfigLevel::Local)?)
}

/// Every `branch.<name>.<variable>` setting of the repository's own config file, by branch name
/// (#1151). A branch name may hold dots; the variable never does.
fn branch_settings(repo: &Repository) -> Result<BTreeMap<String, BranchSettings>, UndoError> {
    let mut settings: BTreeMap<String, BranchSettings> = BTreeMap::new();
    let config = match local_config(repo) {
        Ok(config) => config,
        Err(UndoError::Git(error)) if error.code() == ErrorCode::NotFound => return Ok(settings),
        Err(error) => return Err(error),
    };
    let mut entries = config.entries(Some(r"^branch\..+\.[^.]+$"))?;
    while let Some(entry) = entries.next() {
        let entry = entry?;
        let name = String::from_utf8_lossy(entry.name_bytes());
        let Some((branch, variable)) = name
            .strip_prefix("branch.")
            .and_then(|rest| rest.rsplit_once('.'))
        else {
            continue;
        };
        // A variable without `=` is boolean true.
        let value = if entry.has_value() {
            String::from_utf8_lossy(entry.value_bytes()).into_owned()
        } else {
            "true".to_string()
        };
        settings
            .entry(branch.to_string())
            .or_default()
            .push((variable.to_string(), value));
    }
    Ok(settings)
}

/// Replaces `branch`'s settings `current` with `wanted`, in `wanted`'s order.
fn write_branch_settings(
    config: &mut git2::Config,
    branch: &str,
    current: &[(String, String)],
    wanted: &[(String, String)],
) -> Result<(), UndoError> {
    let variables: BTreeSet<&str> = current.iter().map(|(name, _)| name.as_str()).collect();
    for variable in variables {
        config.remove_multivar(&format!("branch.{branch}.{variable}"), ".*")?;
    }
    let mut written = BTreeSet::new();
    for (variable, value) in wanted {
        let name = format!("branch.{branch}.{variable}");
        if written.insert(variable) {
            config.set_str(&name, value)?;
        } else {
            // A further value of a multi-valued variable: a pattern no value matches adds it.
            config.set_multivar(&name, "a^", value)?;
        }
    }
    Ok(())
}

pub(crate) struct WorktreeConfig {
    filemode: bool,
    symlinks: bool,
}

pub(crate) fn worktree_config(repo: &Repository) -> Result<WorktreeConfig, UndoError> {
    let config = repo.config()?;
    Ok(WorktreeConfig {
        filemode: config.get_bool("core.filemode").unwrap_or(cfg!(unix)),
        symlinks: config.get_bool("core.symlinks").unwrap_or(cfg!(unix)),
    })
}

struct WorktreeSnapshot {
    tree: Oid,
    skipped: BTreeMap<String, SkippedFile>,
    unsaved_untracked: BTreeMap<String, SkippedFile>,
}

/// The work tree as a tree, and the files it leaves out: those over the size limit (#1065), and
/// every untracked one when there are more than the untracked limit (#1090). Each stored file
/// streams from its opened handle into the object database (raw bytes, no filters), so a big
/// one is never held in memory; one left out is not read at all.
fn build_worktree_tree(repo: &Repository, limits: Limits) -> Result<WorktreeSnapshot, UndoError> {
    use crate::worktree_guard::Opened;
    let workdir = repo.workdir().ok_or(UndoError::Bare)?;
    let config = worktree_config(repo)?;
    let mut overlay = Index::new()?;
    let mut skipped = BTreeMap::new();
    let mut unsaved_untracked = BTreeMap::new();
    if let Ok(head) = repo.head() {
        overlay.read_tree(&head.peel_to_tree()?)?;
    }
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true);
    let statuses = repo.statuses(Some(&mut options))?;
    // An untracked nested repository is reported as "dir/"; it is not ours to snapshot.
    let entries = || {
        statuses.iter().filter(|entry| {
            !entry.status().contains(Status::IGNORED) && !entry.path_bytes().ends_with(b"/")
        })
    };
    let untracked = |entry: &git2::StatusEntry| entry.status().contains(Status::WT_NEW);
    let whole: Vec<String> = limits
        .whole
        .iter()
        .map(|name| name.trim_end_matches('/').to_lowercase())
        .collect();
    let kept = |entry: &git2::StatusEntry| {
        !whole.is_empty() && named_whole(&whole, &String::from_utf8_lossy(entry.path_bytes()))
    };
    let list_untracked = limits.untracked.is_some_and(|limit| {
        entries()
            .filter(|entry| untracked(entry) && !kept(entry))
            .count()
            > limit
    });
    for entry in entries() {
        let path = entry.path()?.to_string();
        let kept = kept(&entry);
        let previous_mode = overlay.get_path(Path::new(&path), 0).map(|e| e.mode);
        match crate::worktree_guard::open_leaf_beneath(workdir, &path)? {
            Some(Opened::Link { target, .. }) => {
                let id = repo.blob(&target)?;
                overlay.add(&index_entry(&path, id, MODE_LINK))?;
            }
            Some(Opened::File {
                mut file,
                executable,
                stat,
            }) => {
                let mode = file_mode(&config, executable, previous_mode);
                let left_out = if kept {
                    None
                } else if list_untracked && untracked(&entry) {
                    Some(&mut unsaved_untracked)
                } else if limits.file_size.is_some_and(|limit| stat.size > limit) {
                    Some(&mut skipped)
                } else {
                    None
                };
                if let Some(left_out) = left_out {
                    left_out.insert(path.clone(), SkippedFile::of(&stat, mode));
                    let _ = overlay.remove_path(Path::new(&path));
                    continue;
                }
                let mut writer = repo.blob_writer(None)?;
                io::copy(&mut file, &mut writer)?;
                overlay.add(&index_entry(&path, writer.commit()?, mode))?;
            }
            None => {
                let _ = overlay.remove_path(Path::new(&path));
            }
        }
    }
    Ok(WorktreeSnapshot {
        tree: overlay.write_tree_to(repo)?,
        skipped,
        unsaved_untracked,
    })
}

impl SkippedFile {
    fn of(stat: &crate::worktree_guard::FileStat, mode: u32) -> Self {
        SkippedFile {
            size: stat.size,
            mtime: stat.mtime,
            ino: stat.ino,
            mode,
        }
    }
}

/// The work-tree file `rel` as a [`SkippedFile`] would identify it, `None` when it is missing or
/// not a regular file; its mode as [`file_mode`] gives it with `previous_mode`. Nothing is read.
fn identify_worktree_file(
    workdir: &Path,
    rel: &str,
    config: &WorktreeConfig,
    previous_mode: Option<u32>,
) -> io::Result<Option<SkippedFile>> {
    use crate::worktree_guard::Opened;
    Ok(
        match crate::worktree_guard::open_leaf_beneath(workdir, rel)? {
            Some(Opened::File {
                executable, stat, ..
            }) => Some(SkippedFile::of(
                &stat,
                file_mode(config, executable, previous_mode),
            )),
            _ => None,
        },
    )
}

/// Raw bytes (no clean filters) and git mode of the worktree path `rel`; `None` when absent, a
/// directory, not a regular file (a FIFO never blocks the snapshot), or below a symlinked
/// directory (git sees no file there, and reading would follow the link, possibly out of the
/// repository). Read through `worktree_guard::read_beneath`, so a folder swapped for a link
/// meanwhile is never followed (#149). `previous_mode` keeps the exec bit when `core.filemode`
/// is off and the symlink mode when `core.symlinks` is off, matching what git itself records.
pub(crate) fn read_worktree_file(
    workdir: &Path,
    rel: &str,
    config: &WorktreeConfig,
    previous_mode: Option<u32>,
) -> io::Result<Option<(Vec<u8>, u32)>> {
    let (bytes, executable) = match crate::worktree_guard::read_beneath(workdir, rel)? {
        None => return Ok(None),
        Some(Leaf::Link { target, .. }) => return Ok(Some((target, MODE_LINK))),
        Some(Leaf::File {
            bytes, executable, ..
        }) => (bytes, executable),
    };
    Ok(Some((bytes, file_mode(config, executable, previous_mode))))
}

/// The git mode of a regular work-tree file, as git records it: the exec bit unless
/// `core.filemode` is off (then `previous_mode`'s), and `previous_mode`'s link mode when
/// `core.symlinks` is off.
pub(crate) fn file_mode(
    config: &WorktreeConfig,
    executable: bool,
    previous_mode: Option<u32>,
) -> u32 {
    if !config.symlinks && previous_mode == Some(MODE_LINK) {
        MODE_LINK
    } else if config.filemode {
        if executable {
            MODE_EXEC
        } else {
            MODE_FILE
        }
    } else if previous_mode == Some(MODE_EXEC) {
        MODE_EXEC
    } else {
        MODE_FILE
    }
}

/// Writes `bytes`, read from the work-tree path `path`, as a blob through git's clean filters
/// (`core.autocrlf`, `.gitattributes`), as `git add` and `Index::add_path` would. `path` is only
/// the filter hint; nothing is read from it.
pub(crate) fn filtered_blob<E>(repo: &Repository, path: &str, bytes: &[u8]) -> Result<Oid, E>
where
    E: From<git2::Error> + From<io::Error>,
{
    let mut writer = repo.blob_writer(Some(Path::new(path)))?;
    io::Write::write_all(&mut writer, bytes)?;
    Ok(writer.commit()?)
}

pub(crate) fn index_entry(path: &str, id: Oid, mode: u32) -> IndexEntry {
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

pub fn restore(
    repo: &Repository,
    expected: &RepoSnapshot,
    target: &RepoSnapshot,
    reflog_message: &str,
) -> Result<(), UndoError> {
    if !crate::repo::no_operation(repo) {
        return Err(UndoError::OperationInProgress);
    }

    // Only the components that differ between the two snapshots are checked and written, so
    // unrelated changes elsewhere never block an undo.
    let head_differs = expected.head != target.head;
    let changed_refs: Vec<&String> = expected
        .refs
        .keys()
        .chain(target.refs.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|name| expected.refs.get(*name) != target.refs.get(*name))
        .collect();
    let index_differs =
        expected.index_tree != target.index_tree || expected.intent_to_add != target.intent_to_add;
    let worktree = match (&expected.worktree_tree, &target.worktree_tree) {
        (Some(from), Some(to))
            if from != to
                || expected.skipped != target.skipped
                || expected.unsaved_untracked != target.unsaved_untracked =>
        {
            Some(WorktreeChange::new(repo, expected, target, from, to)?)
        }
        _ => None,
    };

    // 1. Verify, writing nothing.
    if head_differs && head_state(repo)? != expected.head {
        return Err(UndoError::RepositoryChanged);
    }
    for name in &changed_refs {
        let current = match repo.find_reference(name) {
            Ok(reference) => reference.target().map(|oid| oid.to_string()),
            Err(error) if error.code() == ErrorCode::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if current.as_ref() != expected.refs.get(*name) {
            return Err(UndoError::RepositoryChanged);
        }
    }
    verify_checked_out_branches(repo, head_differs, &changed_refs, target)?;
    let settings = changed_branch_settings(expected, target);
    let mut config = None;
    if !settings.is_empty() {
        let current = branch_settings(repo)?;
        // Like a ref: still the expected side's settings, or the restore would overwrite others.
        if settings
            .iter()
            .any(|branch| current.get(*branch) != expected.branch_config.get(*branch))
        {
            return Err(UndoError::RepositoryChanged);
        }
        config = Some((local_config(repo)?, current));
    }
    if index_differs {
        let mut index = repo.index()?;
        index.read(false)?;
        if index.has_conflicts() {
            return Err(UndoError::RepositoryChanged);
        }
        // The target's already: a restore that stopped after writing the index (in the checkout
        // from it) is being tried again (#251); the work tree is still checked below. An
        // intent-to-add entry is an empty file in the tree, so its mark counts too (#1174).
        let current = (index.write_tree()?.to_string(), intent_to_add(&index));
        let side = |snapshot: &RepoSnapshot| {
            current.0 == snapshot.index_tree && current.1 == snapshot.intent_to_add
        };
        if !side(expected) && !side(target) {
            return Err(UndoError::RepositoryChanged);
        }
    }
    if let Some(change) = &worktree {
        change.verify(repo)?;
        // The raw writes below come before the forced checkout's own guard, so a target that
        // brings in a case-colliding link and folder (#156) is refused here, writing nothing.
        if let Some(to) = &target.worktree_tree {
            let tree = repo.find_tree(Oid::from_str(to)?)?;
            crate::worktree_guard::refuse_case_collision::<UndoError, _>(
                repo,
                &crate::worktree_guard::GuardSource::Tree(&tree),
                Some(&change.paths),
            )?;
        }
    }

    // 2. Write: worktree first (most likely to fail), then index, refs, HEAD.
    if let Some(change) = &worktree {
        change.apply(repo)?;
    }
    if index_differs {
        let mut index = repo.index()?;
        let tree = repo.find_tree(Oid::from_str(&target.index_tree)?)?;
        // The skip-worktree marks stay as they are now, as git's reset keeps them (#1145); the
        // intent-to-add ones are the target's (#1174).
        let skip_worktree = crate::index_marks::skip_worktree_paths(&index);
        let intent_to_add: Vec<Vec<u8>> = target
            .intent_to_add
            .iter()
            .map(|path| path.as_bytes().to_vec())
            .collect();
        crate::index_marks::reset_index_marking(&mut index, &tree, &skip_worktree, &intent_to_add)?;
    }
    if let Some(change) = &worktree {
        change.checkout_from_index(repo)?;
    }
    for name in &changed_refs {
        match target.refs.get(*name) {
            Some(oid) => {
                repo.reference(name, Oid::from_str(oid)?, true, reflog_message)?;
            }
            None => repo.find_reference(name)?.delete()?,
        }
    }
    if let Some((mut config, current)) = config {
        for branch in settings {
            write_branch_settings(
                &mut config,
                branch,
                current.get(branch).map_or(&[], Vec::as_slice),
                target.branch_config.get(branch).map_or(&[], Vec::as_slice),
            )?;
        }
    }
    if head_differs {
        match &target.head {
            HeadState::Branch(name) => {
                repo.reference_symbolic("HEAD", name, true, reflog_message)?;
            }
            HeadState::Detached(oid) => {
                repo.reference("HEAD", Oid::from_str(oid)?, true, reflog_message)?;
            }
        }
    }
    // Undoing (or redoing) moves away from whatever a pending squash merge staged.
    crate::merge::clear_squash_message(repo)?;
    Ok(())
}

/// The branches whose settings a restore from `expected` to `target` writes (#1151): those it
/// creates or deletes (the ref is on one side only) whose settings differ between the sides.
/// The settings of a branch that stays are never touched: an entry doesn't change them, and one
/// changed since by an unrecorded command (Set upstream) is the user's.
fn changed_branch_settings<'a>(
    expected: &'a RepoSnapshot,
    target: &'a RepoSnapshot,
) -> Vec<&'a str> {
    expected
        .branch_config
        .keys()
        .chain(target.branch_config.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|branch| {
            let name = format!("refs/heads/{branch}");
            expected.refs.contains_key(&name) != target.refs.contains_key(&name)
                && expected.branch_config.get(*branch) != target.branch_config.get(*branch)
        })
        .map(String::as_str)
        .collect()
}

/// A branch that is some worktree's HEAD must not be deleted or moved under it, and no two
/// worktrees may end up on the same branch.
fn verify_checked_out_branches(
    repo: &Repository,
    head_differs: bool,
    changed_refs: &[&String],
    target: &RepoSnapshot,
) -> Result<(), UndoError> {
    let target_branch = match &target.head {
        HeadState::Branch(name) if head_differs => Some(name),
        _ => None,
    };
    let branches: Vec<&String> = changed_refs
        .iter()
        .copied()
        .filter(|name| name.starts_with("refs/heads/"))
        .collect();
    if branches.is_empty() && target_branch.is_none() {
        return Ok(());
    }
    let moves = |name: &str| branches.iter().any(|branch| *branch == name);
    // Moving or deleting a branch follows `git branch -f`/`-D`; putting HEAD on one follows
    // `git switch` (#291).
    let moved = crate::repo::branches_held(repo, crate::repo::HoldRule::MoveOrDelete)?
        .into_iter()
        .find(|held| moves(&format!("refs/heads/{}", held.branch)));
    let checked_out = match target_branch {
        Some(target) => crate::repo::branches_held(
            repo,
            crate::repo::HoldRule::CheckOut {
                include_this: false,
            },
        )?
        .into_iter()
        .find(|held| *target == format!("refs/heads/{}", held.branch)),
        None => None,
    };
    if let Some(held) = moved.or(checked_out) {
        return Err(held.into());
    }
    // Deleting this worktree's HEAD branch is only fine when HEAD moves off it, or when the
    // target itself has HEAD on that (then unborn) branch, e.g. undoing the first commit.
    if !head_differs {
        if let HeadState::Branch(current) = head_state(repo)? {
            let deleted = branches
                .iter()
                .any(|name| **name == current && !target.refs.contains_key(*name));
            if deleted && target.head != HeadState::Branch(current) {
                return Err(UndoError::RepositoryChanged);
            }
        }
    }
    Ok(())
}

type TreeFiles = BTreeMap<String, (Oid, u32)>;

struct WorktreeChange {
    from: TreeFiles,
    to: TreeFiles,
    paths: Vec<String>,
    /// Paths in either snapshot's index tree; these are never treated as ignored/unsnapshotted.
    tracked: BTreeSet<String>,
    /// Target files equal to their index entry once the index is written: checked out from the
    /// index (so smudge filters and `core.autocrlf` apply) rather than written as raw bytes.
    from_index: BTreeSet<String>,
    /// Files the target didn't store (#1065, #1090), unless the expected side left them out
    /// alike (the command didn't touch them): they must already be as the target saw them,
    /// else `NotSaved` or `NotSavedUntracked`. Never written.
    keep: Vec<(String, SkippedFile, LeftOut)>,
    /// Files only the expected side didn't store (in `paths`): checked by their stat data, not
    /// their contents, then written or removed like any other path.
    from_skipped: BTreeMap<String, SkippedFile>,
}

/// Why a snapshot left a file out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeftOut {
    TooBig,
    TooManyUntracked,
}

/// Every file `snapshot` left out, by path; its untracked list only when `with_list` (when both
/// sides share one list, its files are left out alike and need no reading).
fn left_out(
    repo: &Repository,
    snapshot: &RepoSnapshot,
    with_list: bool,
) -> Result<BTreeMap<String, (SkippedFile, LeftOut)>, UndoError> {
    let mut files: BTreeMap<String, (SkippedFile, LeftOut)> = snapshot
        .skipped
        .iter()
        .map(|(path, file)| (path.clone(), (*file, LeftOut::TooBig)))
        .collect();
    if with_list {
        for (path, file) in unsaved_untracked(repo, snapshot)? {
            files.insert(path, (file, LeftOut::TooManyUntracked));
        }
    }
    Ok(files)
}

/// Whether the work-tree file `rel` already has the blob `id` with `mode`; read only when its
/// size matches.
fn holds_blob(
    repo: &Repository,
    workdir: &Path,
    rel: &str,
    config: &WorktreeConfig,
    (id, mode): (Oid, u32),
) -> Result<bool, UndoError> {
    worktree_holds(repo, workdir, rel, config, id, Some(mode))
}

/// Whether the work-tree path `rel` holds the blob `id` (raw bytes, as [`read_worktree_file`]
/// reads them) and, when `mode` is given, has that git mode (as `read_worktree_file` with it as
/// the previous mode gives it). A file of another size isn't read; one of the same size is
/// compared with the blob in pieces, so neither is held in memory whole (#1089).
fn worktree_holds(
    repo: &Repository,
    workdir: &Path,
    rel: &str,
    config: &WorktreeConfig,
    id: Oid,
    mode: Option<u32>,
) -> Result<bool, UndoError> {
    use crate::worktree_guard::Opened;
    match crate::worktree_guard::open_leaf_beneath(workdir, rel)? {
        None => Ok(false),
        Some(Opened::Link { target, .. }) => Ok(mode.is_none_or(|mode| mode == MODE_LINK)
            && Oid::hash_object(ObjectType::Blob, &target)? == id),
        Some(Opened::File {
            mut file,
            executable,
            stat,
        }) => {
            if mode.is_some_and(|mode| file_mode(config, executable, Some(mode)) != mode) {
                return Ok(false);
            }
            if stat.size != repo.odb()?.read_header(id)?.0 as u64 {
                return Ok(false);
            }
            blob_equals(repo, id, &mut file)
        }
    }
}

/// Whether `reader` yields exactly the blob `id`'s bytes. The blob is read as a stream where the
/// object database can stream it (a loose object), else loaded (a packed one).
fn blob_equals(repo: &Repository, id: Oid, reader: &mut dyn io::Read) -> Result<bool, UndoError> {
    let odb = repo.odb()?;
    if let Ok((mut stream, _, _)) = odb.reader(id) {
        return Ok(same_bytes(&mut stream, reader)?);
    }
    let blob = repo.find_blob(id)?;
    Ok(same_bytes(&mut blob.content(), reader)?)
}

/// Whether two readers yield the same bytes, compared in pieces.
fn same_bytes(a: &mut dyn io::Read, b: &mut dyn io::Read) -> io::Result<bool> {
    const PIECE: usize = 64 * 1024;
    let mut left = vec![0; PIECE];
    let mut right = vec![0; PIECE];
    loop {
        let n = read_up_to(a, &mut left)?;
        let m = read_up_to(b, &mut right)?;
        if left[..n] != right[..m] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

/// Reads until `buffer` is full or the reader ends; how many bytes it got.
fn read_up_to(reader: &mut dyn io::Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

impl WorktreeChange {
    fn new(
        repo: &Repository,
        expected: &RepoSnapshot,
        target: &RepoSnapshot,
        from: &str,
        to: &str,
    ) -> Result<Self, UndoError> {
        let target_index = flatten_tree(repo, &target.index_tree)?;
        let tracked = flatten_tree(repo, &expected.index_tree)?
            .into_keys()
            .chain(target_index.keys().cloned())
            .collect();
        let from = flatten_tree(repo, from)?;
        let to = flatten_tree(repo, to)?;
        let mut paths: BTreeSet<String> = from
            .keys()
            .chain(to.keys())
            .filter(|path| from.get(*path) != to.get(*path))
            .cloned()
            .collect();
        let lists_differ = expected.unsaved_untracked != target.unsaved_untracked;
        let expected_left = left_out(repo, expected, lists_differ)?;
        let target_left = left_out(repo, target, lists_differ)?;
        let mut keep = Vec::new();
        for (path, (file, why)) in &target_left {
            if expected_left.get(path).map(|(file, _)| file) != Some(file) {
                keep.push((path.clone(), *file, *why));
            }
            paths.remove(path);
        }
        let workdir = repo.workdir().ok_or(UndoError::Bare)?;
        let config = worktree_config(repo)?;
        let mut from_skipped = BTreeMap::new();
        for (path, (file, _)) in &expected_left {
            if target_left.contains_key(path) {
                continue;
            }
            // Already what the target wants (a file untracked by the command, its count then
            // over the limit): left as it is, so it still matches what the other side noted.
            if let Some(&wanted) = to.get(path) {
                if holds_blob(repo, workdir, path, &config, wanted)? {
                    paths.remove(path);
                    continue;
                }
            }
            paths.insert(path.clone());
            from_skipped.insert(path.clone(), *file);
        }
        let paths: Vec<String> = paths.into_iter().collect();
        // Ignore files stay raw writes: apply needs them on disk before the index is written.
        // When the index isn't rewritten, the entry checked out is the current one, which may
        // have been restaged since; only matching entries qualify.
        let index_differs = expected.index_tree != target.index_tree;
        let mut index = repo.index()?;
        index.read(false)?;
        // A case pair (`A` and `a`) under `core.ignorecase` stays a raw write: libgit2's forced
        // checkout of one of them can drop the other from its ignore-case index or rename its
        // entry (#403), and `prepare_forced_checkout` refuses it.
        let in_pair: BTreeSet<Vec<u8>> = if crate::worktree_guard::core_ignorecase(repo)? {
            let index_tree = repo.find_tree(Oid::from_str(&target.index_tree)?)?;
            let head_tree = repo.head().and_then(|head| head.peel_to_tree()).ok();
            let mut sources = vec![
                crate::worktree_guard::GuardSource::Tree(&index_tree),
                crate::worktree_guard::GuardSource::Index(&index),
            ];
            sources.extend(
                head_tree
                    .as_ref()
                    .map(crate::worktree_guard::GuardSource::Tree),
            );
            crate::worktree_guard::case_pairs(&sources.iter().collect::<Vec<_>>())?
                .into_iter()
                .flatten()
                .collect()
        } else {
            BTreeSet::new()
        };
        let from_index = paths
            .iter()
            .filter(|path| !is_ignore_file(path))
            .filter(|path| !in_pair.contains(path.as_bytes()))
            .filter(|path| {
                let wanted = to.get(*path);
                wanted.is_some()
                    && target_index.get(*path) == wanted
                    && (index_differs || index_entry_of(&index, path) == wanted.copied())
            })
            .cloned()
            .collect();
        Ok(Self {
            from,
            to,
            paths,
            tracked,
            from_index,
            keep,
            from_skipped,
        })
    }

    fn verify(&self, repo: &Repository) -> Result<(), UndoError> {
        let workdir = repo.workdir().ok_or(UndoError::Bare)?;
        let config = worktree_config(repo)?;
        let mut index = repo.index()?;
        index.read(false)?;
        for (path, file, why) in &self.keep {
            if identify_worktree_file(workdir, path, &config, Some(file.mode))? != Some(*file) {
                return Err(match why {
                    LeftOut::TooBig => UndoError::NotSaved(path.clone()),
                    LeftOut::TooManyUntracked => UndoError::NotSavedUntracked(path.clone()),
                });
            }
        }
        for path in &self.paths {
            let full = workdir.join(path);
            if let Some(file) = self.from_skipped.get(path) {
                // Its contents were never stored: as the snapshot saw it, judged by its stat
                // data, without reading it.
                if identify_worktree_file(workdir, path, &config, Some(file.mode))? != Some(*file)
                    && !self.already_restored(repo, workdir, path, &config)?
                {
                    return Err(UndoError::RepositoryChanged);
                }
                continue;
            }
            let expected = self.from.get(path).copied();
            let as_expected = match expected {
                None => crate::worktree_guard::open_leaf_beneath(workdir, path)?.is_none(),
                Some((id, mode)) => worktree_holds(repo, workdir, path, &config, id, Some(mode))?,
            };
            // A path checked out from the index already, by a checkout that stopped part-way
            // (#268): filters (`core.autocrlf`) mean its bytes needn't be the target's raw ones.
            let checked_out = || {
                self.from_index.contains(path)
                    && clean_against_index(repo, &index, path, self.to.get(path).copied())
            };
            if !as_expected
                && !clean_against_index(repo, &index, path, expected)
                && !self.already_restored(repo, workdir, path, &config)?
                && !checked_out()
            {
                // Ignored files are not snapshotted: absent from the tree but present on disk
                // is fine as long as the file is ignored and the target would not change it.
                let ignored_untouched = expected.is_none()
                    && repo.is_path_ignored(path)?
                    && match self.to.get(path) {
                        Some(&(id, _)) => worktree_holds(repo, workdir, path, &config, id, None)?,
                        None => true,
                    };
                if !ignored_untouched {
                    return Err(UndoError::RepositoryChanged);
                }
            }
            // A directory about to be replaced by a file must hold only files apply deletes.
            if self.to.contains_key(path) && fs::symlink_metadata(&full).is_ok_and(|m| m.is_dir()) {
                self.verify_directory_is_removable(workdir, &full)?;
            }
        }
        Ok(())
    }

    /// Whether `path` already is what the target wants there (content, mode and kind, or
    /// nothing at all): a restore that stopped part-way (a full disk, a file open in another
    /// program) wrote it, and trying again finishes the restore instead of calling that a change
    /// since the operation (#251). Writing it again changes nothing.
    fn already_restored(
        &self,
        repo: &Repository,
        workdir: &Path,
        path: &str,
        config: &WorktreeConfig,
    ) -> Result<bool, UndoError> {
        let Some(&(id, mode)) = self.to.get(path) else {
            // Gone, or the folder the target's files below it need (written there already).
            return Ok(match fs::symlink_metadata(workdir.join(path)) {
                Err(_) => true,
                Ok(meta) => meta.is_dir() && self.has_target_files_below(path),
            });
        };
        worktree_holds(repo, workdir, path, config, id, Some(mode))
    }

    /// Whether the target has files below `path` (which then is, or becomes, their folder).
    fn has_target_files_below(&self, path: &str) -> bool {
        let below = format!("{path}/");
        self.to
            .range(below.clone()..)
            .next()
            .is_some_and(|(name, _)| name.starts_with(&below))
    }

    fn verify_directory_is_removable(&self, workdir: &Path, dir: &Path) -> Result<(), UndoError> {
        let mut entries = fs::read_dir(dir)?.peekable();
        if entries.peek().is_none() {
            return Err(UndoError::RepositoryChanged);
        }
        for entry in entries {
            let entry = entry?;
            let full = entry.path();
            if entry.file_type()?.is_dir() {
                self.verify_directory_is_removable(workdir, &full)?;
                continue;
            }
            let relative = full
                .strip_prefix(workdir)
                .ok()
                .and_then(|p| p.to_str())
                .map(|p| p.replace('\\', "/"));
            // Untracked deletions might turn out ignored under the target's rules (unknown yet).
            let deleted = relative.is_some_and(|p| {
                self.paths.contains(&p) && !self.to.contains_key(&p) && self.tracked.contains(&p)
            });
            if !deleted {
                return Err(UndoError::RepositoryChanged);
            }
        }
        Ok(())
    }

    fn apply(&self, repo: &Repository) -> Result<(), UndoError> {
        let workdir = repo.workdir().ok_or(UndoError::Bare)?;
        let config = worktree_config(repo)?;
        // Ignore files first, so the target's ignore rules are on disk before deciding which
        // other deletions would hit a now-ignored (unsnapshotted) file.
        for path in self.paths.iter().filter(|path| is_ignore_file(path)) {
            match self.to.get(path) {
                Some((id, mode)) => write_worktree_file(repo, workdir, path, *id, *mode, &config)?,
                None => remove_existing(workdir, path)?,
            }
        }
        let others = || self.paths.iter().filter(|path| !is_ignore_file(path));
        // Deletions before writes, so a file can replace a directory that only held deleted files.
        for path in others().filter(|path| !self.to.contains_key(*path)) {
            let full = workdir.join(path);
            if !self.tracked.contains(path)
                && repo.is_path_ignored(path)?
                && fs::symlink_metadata(&full).is_ok()
            {
                continue;
            }
            // Already the folder of the target's files below it: a restore that stopped after
            // writing them is being tried again (#251).
            if fs::symlink_metadata(&full).is_ok_and(|meta| meta.is_dir())
                && self.has_target_files_below(path)
            {
                continue;
            }
            remove_existing(workdir, path)?;
        }
        for path in others().filter(|path| !self.from_index.contains(*path)) {
            if let Some((id, mode)) = self.to.get(path) {
                write_worktree_file(repo, workdir, path, *id, *mode, &config)?;
            }
        }
        Ok(())
    }

    /// Runs after the index is written: materialises `from_index` paths through git's checkout.
    fn checkout_from_index(&self, repo: &Repository) -> Result<(), UndoError> {
        if self.from_index.is_empty() {
            return Ok(());
        }
        // A forced checkout writes through a symlink that replaced a file or folder (#114).
        let index = repo.index()?;
        let paths: Vec<String> = self.from_index.iter().cloned().collect();
        crate::worktree_guard::prepare_forced_checkout::<UndoError, _>(
            repo,
            Some(&crate::worktree_guard::GuardSource::Index(&index)),
            Some(&paths),
        )?;
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().disable_pathspec_match(true);
        for path in &self.from_index {
            checkout.path(path);
        }
        let filters = crate::hardlink::on_checkout(repo, &mut checkout);
        let workdir = repo.workdir().ok_or(UndoError::Bare)?;
        // A file another program holds open: the walker's message, not libgit2's raw error
        // (#268). Files checked out before it stay written; trying again finishes the restore.
        repo.checkout_index(None, Some(&mut checkout))
            .map_err(|error| {
                match crate::worktree_guard::held_open_after_checkout(
                    workdir,
                    &error,
                    self.from_index.iter().map(String::as_str),
                ) {
                    Some(path) => UndoError::FileInUse(path.to_string()),
                    None => UndoError::Git(error),
                }
            })?;
        filters.check()?;
        // libgit2 skips a file whose HEAD (its checkout baseline) still has a directory there,
        // as in a dir -> file swap; fall back to the raw blob for anything still missing.
        let config = worktree_config(repo)?;
        for path in &self.from_index {
            let full = workdir.join(path);
            if fs::symlink_metadata(&full).is_err() {
                if let Some((id, mode)) = self.to.get(path) {
                    write_worktree_file(repo, workdir, path, *id, *mode, &config)?;
                }
            }
        }
        Ok(())
    }
}

fn is_ignore_file(path: &str) -> bool {
    path.rsplit('/').next() == Some(".gitignore")
}

fn index_entry_of(index: &Index, path: &str) -> Option<(Oid, u32)> {
    index
        .get_path(Path::new(path), 0)
        .map(|entry| (entry.id, entry.mode))
}

/// A path whose expected content is the index's and that git reports unmodified in the
/// worktree: its raw bytes may still differ (`core.autocrlf`, smudge filters), but they are
/// what git itself would check out, so nothing of the user's is lost by overwriting them.
fn clean_against_index(
    repo: &Repository,
    index: &Index,
    path: &str,
    expected: Option<(Oid, u32)>,
) -> bool {
    expected.is_some()
        && index_entry_of(index, path) == expected
        && repo.status_file(Path::new(path)).is_ok_and(|status| {
            !status.intersects(
                Status::WT_NEW
                    | Status::WT_MODIFIED
                    | Status::WT_DELETED
                    | Status::WT_TYPECHANGE
                    | Status::WT_RENAMED
                    | Status::WT_UNREADABLE,
            )
        })
}

fn flatten_tree(repo: &Repository, id: &str) -> Result<TreeFiles, UndoError> {
    let tree = repo.find_tree(Oid::from_str(id)?)?;
    let mut files = TreeFiles::new();
    tree.walk(TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(ObjectType::Blob) {
            if let Ok(name) = entry.name() {
                files.insert(
                    format!("{root}{name}"),
                    (entry.id(), entry.filemode() as u32),
                );
            }
        }
        TreeWalkResult::Ok
    })?;
    Ok(files)
}

/// Removes the file, link or empty directory `rel` and then the directories that emptied. Never
/// acts through a symlinked directory: a path below one does not exist for git (#114), and the
/// folders are held open while acting, so a swap meanwhile can't redirect it (#149).
fn remove_existing(workdir: &Path, rel: &str) -> io::Result<()> {
    crate::worktree_guard::remove_and_prune_beneath(workdir, Path::new(rel))
}

/// Writes the blob as `rel`, replacing what is there whole (#251). A symlink that replaced a
/// directory on the way is removed first (never written through, #114), through held folders
/// (#149). A file another program holds open is [`UndoError::FileInUse`].
fn write_worktree_file(
    repo: &Repository,
    workdir: &Path,
    rel: &str,
    id: Oid,
    mode: u32,
    config: &WorktreeConfig,
) -> Result<(), UndoError> {
    let written = if mode == MODE_LINK && config.symlinks {
        let blob = repo.find_blob(id)?;
        crate::worktree_guard::replace_beneath(workdir, rel, Content::Link(blob.content()))
    } else {
        let executable = mode == MODE_EXEC;
        // A loose object streams into the new file in pieces (#1089); a packed one is loaded.
        match repo.odb()?.reader(id) {
            Ok((mut stream, _, _)) => {
                crate::worktree_guard::replace_beneath_from(workdir, rel, executable, &mut stream)
            }
            Err(_) => {
                let blob = repo.find_blob(id)?;
                let content = Content::File {
                    bytes: blob.content(),
                    executable,
                };
                crate::worktree_guard::replace_beneath(workdir, rel, content)
            }
        }
    };
    written.map_err(|error| {
        if crate::worktree_guard::is_in_use(&error) {
            UndoError::FileInUse(rel.to_string())
        } else {
            UndoError::Io(error)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::grouped;

    #[test]
    fn grouped_puts_a_comma_between_groups_of_three_digits() {
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(10_000), "10,000");
        assert_eq!(grouped(100_000), "100,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }
}
