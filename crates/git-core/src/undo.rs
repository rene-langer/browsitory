//! Repository snapshots for the undo stack: capture the state an operation changes, and restore
//! one snapshot over another after checking nothing else touched the same state. See
//! `docs/superpowers/specs/2026-10-18-parity-batch3b-design.md`.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use git2::{
    ErrorCode, Index, IndexEntry, IndexEntryExtendedFlag, IndexTime, ObjectType, Oid, Repository,
    RepositoryState, Status, StatusOptions, TreeWalkMode, TreeWalkResult,
};
use thiserror::Error;

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
pub(crate) fn read_worktree_file(
    full: &Path,
    config: &WorktreeConfig,
    previous_mode: Option<u32>,
) -> io::Result<Option<(Vec<u8>, u32)>> {
    let meta = match fs::symlink_metadata(full) {
        Ok(meta) => meta,
        Err(error) if is_absent(&error) => return Ok(None),
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

/// Missing, or a parent is a file (`d/a` while `d` is a file, as in a file <-> dir swap).
fn is_absent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
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
    if repo.state() != RepositoryState::Clean {
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
    let index_differs = expected.index_tree != target.index_tree;
    let worktree = match (&expected.worktree_tree, &target.worktree_tree) {
        (Some(from), Some(to)) if from != to => {
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
    if index_differs {
        let mut index = repo.index()?;
        index.read(false)?;
        if index.has_conflicts() || index.write_tree()?.to_string() != expected.index_tree {
            return Err(UndoError::RepositoryChanged);
        }
        // Rewriting the index from a tree would silently drop these per-entry flags.
        let flags =
            (IndexEntryExtendedFlag::SKIP_WORKTREE | IndexEntryExtendedFlag::INTENT_TO_ADD).bits();
        if index.iter().any(|entry| entry.flags_extended & flags != 0) {
            return Err(UndoError::RepositoryChanged);
        }
    }
    if let Some(change) = &worktree {
        change.verify(repo)?;
    }

    // 2. Write: worktree first (most likely to fail), then index, refs, HEAD.
    if let Some(change) = &worktree {
        change.apply(repo)?;
    }
    if index_differs {
        let mut index = repo.index()?;
        index.read_tree(&repo.find_tree(Oid::from_str(&target.index_tree)?)?)?;
        index.write()?;
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
    let elsewhere = branches_checked_out_elsewhere(repo)?;
    if target_branch.is_some_and(|name| elsewhere.contains(name))
        || branches.iter().any(|name| elsewhere.contains(*name))
    {
        return Err(UndoError::RepositoryChanged);
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

/// HEAD branches of every other worktree of this repository (main and linked; prunable or
/// unopenable ones skipped).
fn branches_checked_out_elsewhere(repo: &Repository) -> Result<BTreeSet<String>, UndoError> {
    let this = fs::canonicalize(repo.path())?;
    let main = Repository::open(repo.commondir())?;
    let mut others = Vec::new();
    for name in main.worktrees()?.iter().flatten().flatten() {
        let Ok(worktree) = main.find_worktree(name) else {
            continue;
        };
        if worktree.is_prunable(None).unwrap_or(true) {
            continue;
        }
        if let Ok(linked) = Repository::open_from_worktree(&worktree) {
            others.push(linked);
        }
    }
    if !main.is_bare() {
        others.push(main);
    }
    let mut branches = BTreeSet::new();
    for other in others {
        if fs::canonicalize(other.path()).is_ok_and(|path| path == this) {
            continue;
        }
        if let Ok(HeadState::Branch(name)) = head_state(&other) {
            branches.insert(name);
        }
    }
    Ok(branches)
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
        let paths: Vec<String> = from
            .keys()
            .chain(to.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|path| from.get(*path) != to.get(*path))
            .cloned()
            .collect();
        // Ignore files stay raw writes: apply needs them on disk before the index is written.
        // When the index isn't rewritten, the entry checked out is the current one, which may
        // have been restaged since; only matching entries qualify.
        let index_differs = expected.index_tree != target.index_tree;
        let mut index = repo.index()?;
        index.read(false)?;
        let from_index = paths
            .iter()
            .filter(|path| !is_ignore_file(path))
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
        })
    }

    fn verify(&self, repo: &Repository) -> Result<(), UndoError> {
        let workdir = repo.workdir().ok_or(UndoError::Bare)?;
        let config = worktree_config(repo)?;
        let mut index = repo.index()?;
        index.read(false)?;
        for path in &self.paths {
            let expected = self.from.get(path).copied();
            let previous_mode = expected.map(|(_, mode)| mode);
            let full = workdir.join(path);
            let disk = read_worktree_file(&full, &config, previous_mode)?;
            let current = disk
                .as_ref()
                .map(|(bytes, mode)| {
                    Ok::<_, git2::Error>((Oid::hash_object(ObjectType::Blob, bytes)?, *mode))
                })
                .transpose()?;
            if current != expected && !clean_against_index(repo, &index, path, expected) {
                // Ignored files are not snapshotted: absent from the tree but present on disk
                // is fine as long as the file is ignored and the target would not change it.
                let ignored_untouched = expected.is_none()
                    && disk.is_some()
                    && repo.is_path_ignored(path)?
                    && match (self.to.get(path), &disk) {
                        (Some((id, _)), Some((bytes, _))) => {
                            Oid::hash_object(ObjectType::Blob, bytes)? == *id
                        }
                        _ => true,
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
            let full = workdir.join(path);
            match self.to.get(path) {
                Some((id, mode)) => write_worktree_file(repo, &full, *id, *mode, &config)?,
                None => {
                    remove_existing(&full)?;
                    prune_empty_parents(workdir, &full);
                }
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
            remove_existing(&full)?;
            prune_empty_parents(workdir, &full);
        }
        for path in others().filter(|path| !self.from_index.contains(*path)) {
            if let Some((id, mode)) = self.to.get(path) {
                write_worktree_file(repo, &workdir.join(path), *id, *mode, &config)?;
            }
        }
        Ok(())
    }

    /// Runs after the index is written: materialises `from_index` paths through git's checkout.
    fn checkout_from_index(&self, repo: &Repository) -> Result<(), UndoError> {
        if self.from_index.is_empty() {
            return Ok(());
        }
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().disable_pathspec_match(true);
        for path in &self.from_index {
            checkout.path(path);
        }
        repo.checkout_index(None, Some(&mut checkout))?;
        // libgit2 skips a file whose HEAD (its checkout baseline) still has a directory there,
        // as in a dir -> file swap; fall back to the raw blob for anything still missing.
        let workdir = repo.workdir().ok_or(UndoError::Bare)?;
        let config = worktree_config(repo)?;
        for path in &self.from_index {
            let full = workdir.join(path);
            if fs::symlink_metadata(&full).is_err() {
                if let Some((id, mode)) = self.to.get(path) {
                    write_worktree_file(repo, &full, *id, *mode, &config)?;
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

fn remove_existing(full: &Path) -> io::Result<()> {
    match fs::symlink_metadata(full) {
        Ok(meta) if meta.is_dir() => fs::remove_dir(full),
        Ok(_) => fs::remove_file(full),
        Err(error) if is_absent(&error) => Ok(()),
        Err(error) => Err(error),
    }
}

fn prune_empty_parents(workdir: &Path, full: &Path) {
    let mut dir = full.parent();
    while let Some(current) = dir {
        if current == workdir || !current.starts_with(workdir) || fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

fn write_worktree_file(
    repo: &Repository,
    full: &Path,
    id: Oid,
    mode: u32,
    config: &WorktreeConfig,
) -> Result<(), UndoError> {
    let blob = repo.find_blob(id)?;
    remove_existing(full)?;
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)?;
    }
    if mode == MODE_LINK && config.symlinks {
        create_symlink(blob.content(), full)?;
        return Ok(());
    }
    fs::write(full, blob.content())?;
    set_executable(full, mode == MODE_EXEC)?;
    Ok(())
}

#[cfg(unix)]
fn create_symlink(target: &[u8], full: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target), full)
}

#[cfg(not(unix))]
fn create_symlink(target: &[u8], full: &Path) -> io::Result<()> {
    fs::write(full, target)
}

#[cfg(unix)]
fn set_executable(full: &Path, executable: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(full)?.permissions();
    let mode = permissions.mode();
    permissions.set_mode(if executable {
        mode | 0o111
    } else {
        mode & !0o111
    });
    fs::set_permissions(full, permissions)
}

#[cfg(not(unix))]
fn set_executable(_full: &Path, _executable: bool) -> io::Result<()> {
    Ok(())
}
