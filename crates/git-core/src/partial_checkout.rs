//! A libgit2 checkout that stopped part-way (#321): on Windows at a file another program holds
//! open, on any platform at a file or folder it may not write, or while writing a file (a full
//! disk). libgit2 has then written (or removed) the files before that one, and changed their
//! entries in the repository's in-memory index, but not the index on disk, which it writes only
//! once the checkout has finished. Trying the operation again doesn't finish it: the files it
//! wrote now look like local changes.
//!
//! [`Written`] records, through libgit2's progress callback, each path the checkout wrote or
//! removed, and through its notify callback each path it planned to write, as it was before
//! (#329): a file libgit2 stopped *while* writing (truncated, as it opens files with
//! `O_TRUNC`) is never reported done, but is no longer as it was. It also records which folders
//! were there before, and the names libgit2 may give a conflict's sides (`f.txt~HEAD`).
//! [`roll_back`] puts those paths back as the index on disk has them, so the operation can be
//! tried again; [`roll_forward`] does the opposite for an abort, putting them in the index as
//! HEAD has them, so the abort can be tried again.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use git2::Repository;

use crate::worktree_guard::GuardSource;

/// The paths a libgit2 checkout wrote or removed, in git's spelling (`/`-separated bytes).
#[derive(Clone, Default)]
pub(crate) struct Written(Rc<RefCell<Record>>);

#[derive(Default)]
struct Record {
    /// Reported done by libgit2, or pushed by the caller.
    done: Vec<Vec<u8>>,
    /// The work tree, once [`Written::record`] ran.
    workdir: Option<PathBuf>,
    /// Each path the checkout planned to write, as it was before the checkout started.
    planned: Vec<(Vec<u8>, Before)>,
    /// Each path of a conflict the checkout writes, with the entries beside it that
    /// [`side_names`] matches before the checkout started: libgit2 writes a side of some
    /// conflicts (a file where the other side has a folder, two files renamed to one name) as
    /// `name~<label>`, never over an existing entry, and leaves `name.lock` behind when writing
    /// a conflict's merged content fails.
    conflicts: Vec<(Vec<u8>, Before, Vec<OsString>)>,
}

/// A work-tree path before the checkout.
struct Before {
    stamp: Option<Stamp>,
    /// How many of the path's folders were there, from the top.
    folders: usize,
}

/// What tells a file libgit2 opened (and truncated) from one it never reached.
#[derive(PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
    dir: bool,
    link: bool,
}

fn stamp(full: &Path) -> Option<Stamp> {
    let meta = std::fs::symlink_metadata(full).ok()?;
    Some(Stamp {
        len: meta.len(),
        modified: meta.modified().ok(),
        dir: meta.is_dir(),
        link: meta.file_type().is_symlink(),
    })
}

/// `rel` below `workdir` as it is now.
fn before(workdir: &Path, rel: &Path) -> Before {
    let mut folders = 0;
    let mut at = workdir.to_path_buf();
    if let Some(parent) = rel.parent() {
        for part in parent.components() {
            at.push(part);
            match std::fs::symlink_metadata(&at) {
                Ok(meta) if meta.is_dir() => folders += 1,
                _ => break,
            }
        }
    }
    Before {
        stamp: stamp(&workdir.join(rel)),
        folders,
    }
}

impl Written {
    /// Records into `self` each path the checkout `builder` configures writes or removes in
    /// `repo`'s work tree. libgit2 reports a path once it is done with it, so the file it
    /// stopped at is never among them; that one is found by comparing the planned paths with
    /// how they were.
    pub(crate) fn record(&self, repo: &Repository, builder: &mut git2::build::CheckoutBuilder<'_>) {
        let written = self.clone();
        builder.progress(move |path, _, _| {
            if let Some(path) = path {
                written.push(path_bytes(path));
            }
        });
        let Some(workdir) = repo.workdir() else {
            return;
        };
        self.0.borrow_mut().workdir = Some(workdir.to_path_buf());
        // Called while libgit2 plans the checkout, before it writes anything.
        let planned = self.clone();
        let root = workdir.to_path_buf();
        builder.notify_on(git2::CheckoutNotificationType::UPDATED);
        builder.notify(move |_, path, _, _, _| {
            if let Some(path) = path {
                let entry = (path_bytes(path), before(&root, path));
                planned.0.borrow_mut().planned.push(entry);
            }
            true
        });
    }

    /// Before a checkout of `index` (a merge's result, conflicts included): remembers what is
    /// beside each conflict's paths, so a rollback removes the sides libgit2 wrote under
    /// altered names.
    pub(crate) fn expect_conflicts(&self, repo: &Repository, index: &git2::Index) {
        let (Some(workdir), Ok(conflicts)) = (repo.workdir(), index.conflicts()) else {
            return;
        };
        let mut record = self.0.borrow_mut();
        let mut listings = Listings::default();
        for conflict in conflicts.flatten() {
            for entry in [conflict.ancestor, conflict.our, conflict.their]
                .into_iter()
                .flatten()
            {
                let rel = path_from_bytes(&entry.path);
                let siblings = listings.side_names(workdir, &rel);
                let before = before(workdir, &rel);
                record.conflicts.push((entry.path, before, siblings));
            }
        }
    }

    /// Records `path` (git's spelling), which the caller wrote or removed itself.
    pub(crate) fn push(&self, path: Vec<u8>) {
        let mut record = self.0.borrow_mut();
        // A folder libgit2 removed is reported with a trailing `/`.
        let path = match path.strip_suffix(b"/") {
            Some(folder) => folder.to_vec(),
            None => path,
        };
        if !path.is_empty() {
            record.done.push(path);
        }
    }

    /// Adds each planned path that isn't as it was, though never reported done: the file
    /// libgit2 stopped while writing it.
    fn settle(&self) {
        let mut record = self.0.borrow_mut();
        let Some(workdir) = record.workdir.clone() else {
            return;
        };
        // A set, not `done.contains`: a checkout that stops late in a large repository has
        // tens of thousands of both.
        let done: HashSet<&[u8]> = record.done.iter().map(Vec::as_slice).collect();
        let changed: Vec<Vec<u8>> = record
            .planned
            .iter()
            .filter(|(path, before)| {
                !done.contains(path.as_slice())
                    && stamp(&workdir.join(path_from_bytes(path))) != before.stamp
            })
            .map(|(path, _)| path.clone())
            .collect();
        record.done.extend(changed);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.settle();
        self.0.borrow().done.is_empty() && self.side_files().is_empty()
    }

    /// The recorded paths, sorted, each once.
    fn paths(&self) -> Vec<Vec<u8>> {
        self.settle();
        let mut paths = self.0.borrow().done.clone();
        paths.sort();
        paths.dedup();
        paths
    }

    /// How many folders of each planned path were there before the checkout (the first time
    /// it was planned).
    fn folders_before(&self) -> HashMap<Vec<u8>, usize> {
        let mut folders = HashMap::new();
        for (path, before) in &self.0.borrow().planned {
            folders.entry(path.clone()).or_insert(before.folders);
        }
        folders
    }

    /// The `name~*` entries beside a conflict's paths that weren't there before the checkout,
    /// relative, each with how many of its folders were there.
    fn side_files(&self) -> Vec<(PathBuf, usize)> {
        let record = self.0.borrow();
        let Some(workdir) = record.workdir.as_deref() else {
            return Vec::new();
        };
        let mut listings = Listings::default();
        let mut seen = HashSet::new();
        let mut found: Vec<(PathBuf, usize)> = Vec::new();
        for (path, before, siblings) in &record.conflicts {
            let rel = path_from_bytes(path);
            for name in listings.side_names(workdir, &rel) {
                // `siblings` holds only the few names that matched before.
                if siblings.contains(&name) {
                    continue;
                }
                let side = rel.with_file_name(&name);
                if seen.insert(side.clone()) {
                    found.push((side, before.folders));
                }
            }
        }
        found
    }
}

/// Each folder's entries, read once and grouped by the conflict name they may belong to, so
/// many conflicts in one large folder don't read it again for each.
#[derive(Default)]
struct Listings(HashMap<PathBuf, HashMap<Vec<u8>, Vec<OsString>>>);

impl Listings {
    /// The names of the entries beside `rel` (relative to `workdir`) that libgit2 may write
    /// while it writes a conflict at `rel`: `<name>~<label>`, and `<name>.lock`, the temporary
    /// file a conflict's merged content goes through, which libgit2 leaves behind when writing
    /// it fails.
    fn side_names(&mut self, workdir: &Path, rel: &Path) -> Vec<OsString> {
        let Some(name) = rel.file_name() else {
            return Vec::new();
        };
        let folder = match rel.parent() {
            Some(parent) => workdir.join(parent),
            None => workdir.to_path_buf(),
        };
        let groups = self.0.entry(folder).or_insert_with_key(|folder| {
            let mut groups: HashMap<Vec<u8>, Vec<OsString>> = HashMap::new();
            let Ok(entries) = std::fs::read_dir(folder) else {
                return groups;
            };
            for entry in entries.flatten() {
                let entry = entry.file_name();
                let bytes = entry.as_encoded_bytes();
                // Under each name it may be a side of: the part before any `~`, and the
                // name without `.lock`.
                let mut keys: Vec<&[u8]> = bytes
                    .iter()
                    .enumerate()
                    .filter(|(_, byte)| **byte == b'~')
                    .map(|(at, _)| &bytes[..at])
                    .collect();
                if let Some(stem) = bytes.strip_suffix(b".lock") {
                    keys.push(stem);
                }
                keys.sort();
                keys.dedup();
                for key in keys {
                    groups.entry(key.to_vec()).or_default().push(entry.clone());
                }
            }
            groups
        });
        groups
            .get(name.as_encoded_bytes())
            .cloned()
            .unwrap_or_default()
    }
}

/// Why a roll back or roll forward stopped. The caller then reports the operation's own error,
/// libgit2's, unchanged: the files are not as the friendly message would say.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum Unfinished {
    Git(git2::Error),
    Io(std::io::Error),
    CaseCollision(crate::CaseCollision),
    CaseConfig(crate::CaseConfigUnreadable),
}

impl From<crate::CaseConfigUnreadable> for Unfinished {
    fn from(error: crate::CaseConfigUnreadable) -> Self {
        Self::CaseConfig(error)
    }
}

impl From<git2::Error> for Unfinished {
    fn from(error: git2::Error) -> Self {
        Self::Git(error)
    }
}

impl From<std::io::Error> for Unfinished {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<crate::CaseCollision> for Unfinished {
    fn from(error: crate::CaseCollision) -> Self {
        Self::CaseCollision(error)
    }
}

/// After a checkout that stopped part-way: puts each path in `written` back as the index on
/// disk has it, in the work tree and in the repository's in-memory index (re-read from disk).
/// Only for an operation whose checkout started from that index, i.e. a safe checkout, which
/// writes over a tracked file only when its content matches the index, so no uncommitted
/// change to a tracked file is lost; the file it stopped while writing is put back too. A path
/// the index doesn't have (a file the checkout created, or a conflict's side it wrote under an
/// altered name) is removed, with any folders that emptied, but not a folder that was there
/// before. libgit2 also writes over an ignored file in the way of a file it creates (git treats
/// ignored files as expendable too); that file is then removed, not brought back (an ignored
/// file the checkout hadn't reached yet stays). Nothing is written through a symlink: links in the way are
/// removed first, and the files are checked out from the index by libgit2 (so `core.autocrlf`
/// and other filters apply).
///
/// Every path is tried, also after one failed: the tracked files first, then the created ones
/// (deepest first), then the tracked files once more if a created one was in their way. The
/// first error is returned at the end. `Ok` when nothing was written. The caller holds the
/// operation's locks.
pub(crate) fn roll_back(repo: &Repository, written: &Written) -> Result<(), Unfinished> {
    let paths = written.paths();
    if paths.is_empty() && written.side_files().is_empty() {
        return Ok(());
    }
    let workdir = repo.workdir().ok_or_else(bare)?;
    let mut index = repo.index()?;
    // libgit2 writes the index only once the checkout finished; its in-memory copy holds the
    // entries of the files written before the stop.
    index.read(true)?;
    let (tracked, created): (Vec<Vec<u8>>, Vec<Vec<u8>>) = paths
        .into_iter()
        .partition(|path| index.get_path(&path_from_bytes(path), 0).is_some());
    let restore = || -> Result<(), Unfinished> {
        if tracked.is_empty() {
            return Ok(());
        }
        check_out_paths(repo, &GuardSource::Index(&index), &tracked, |checkout| {
            repo.checkout_index(None, Some(checkout))
        })
    };
    let restored = restore();
    let mut first_error = None;
    let mut removed_any = false;
    // A folder that still holds tracked files is left alone; so is a folder that was there
    // before the checkout, also when it empties (#329).
    let mut to_remove: Vec<(PathBuf, usize)> = Vec::new();
    let folders_before = written.folders_before();
    for path in created.iter().rev() {
        let mut below = path.clone();
        below.push(b'/');
        if index.find_prefix(&below).is_ok() {
            continue;
        }
        let keep = folders_before.get(path).copied().unwrap_or(0);
        to_remove.push((path_from_bytes(path), keep));
    }
    // The sides of a conflict libgit2 wrote under altered names (`f.txt~HEAD`).
    to_remove.extend(written.side_files());
    for (path, keep) in to_remove {
        match crate::worktree_guard::remove_and_prune_beneath_keeping(workdir, &path, keep) {
            Ok(()) => removed_any = true,
            Err(error) => {
                first_error.get_or_insert(Unfinished::Io(error));
            }
        }
    }
    // A created file (or folder) may have stood where a tracked one comes back.
    let restored = match restored {
        Err(_) if removed_any => restore(),
        other => other,
    };
    match (restored, first_error) {
        (Err(error), _) | (Ok(()), Some(error)) => Err(error),
        (Ok(()), None) => Ok(()),
    }
}

/// After an abort's forced checkout of `head_tree` that stopped part-way: finishes each path in
/// `written` (the checkout's, and those the abort removed itself), so that it is as HEAD has it
/// in the work tree and in the index on disk. libgit2 may have removed a file it had yet to
/// write again, so the files are checked out once more (they are not the one the checkout
/// stopped at). The rest of the operation, the paths not yet reached and git's state files,
/// stays for the abort to be tried again. The caller holds the operation's locks.
pub(crate) fn roll_forward(
    repo: &Repository,
    head_tree: &git2::Tree,
    written: &Written,
) -> Result<(), Unfinished> {
    let paths = written.paths();
    if paths.is_empty() {
        return Ok(());
    }
    let (in_head, _): (Vec<Vec<u8>>, Vec<Vec<u8>>) = paths
        .iter()
        .cloned()
        .partition(|path| head_tree.get_path(&path_from_bytes(path)).is_ok());
    if !in_head.is_empty() {
        check_out_paths(repo, &GuardSource::Tree(head_tree), &in_head, |checkout| {
            // The index is set below, path by path: libgit2's forced checkout of a tree would
            // drop every conflict of the index, also of the paths it doesn't check out.
            checkout.update_index(false);
            repo.checkout_tree(head_tree.as_object(), Some(checkout))
        })?;
    }
    let mut head_index = git2::Index::new()?;
    head_index.read_tree(head_tree)?;
    let mut index = repo.index()?;
    index.read(true)?;
    for path in &paths {
        let rel = path_from_bytes(path);
        // Every stage: a conflict on the path ends here too.
        index.remove_path(&rel)?;
        if let Some(entry) = head_index.get_path(&rel, 0) {
            index.add(&entry)?;
        }
    }
    index.write()?;
    Ok(())
}

/// A forced checkout of just `paths` (literal) by `checkout`, from `source`, with the links a
/// forced checkout would write through removed first (#114).
fn check_out_paths(
    repo: &Repository,
    source: &GuardSource,
    paths: &[Vec<u8>],
    checkout: impl FnOnce(&mut git2::build::CheckoutBuilder<'_>) -> Result<(), git2::Error>,
) -> Result<(), Unfinished> {
    crate::worktree_guard::prepare_forced_checkout::<Unfinished, _>(
        repo,
        Some(source),
        Some(paths),
    )?;
    let mut builder = git2::build::CheckoutBuilder::new();
    builder.force().disable_pathspec_match(true);
    for path in paths {
        builder.path(path.as_slice());
    }
    checkout(&mut builder)?;
    Ok(())
}

fn bare() -> Unfinished {
    Unfinished::Git(git2::Error::from_str(
        "repository has no working directory (bare repository)",
    ))
}

/// What a failed checkout's caller reports once [`roll_back`] (or [`roll_forward`]) ran:
/// with `restored`, the file another program holds open, as `HeldOpen` with `during`, else
/// libgit2's `error` unchanged. Without `restored`, always libgit2's error.
pub(crate) fn report<E>(
    repo: &Repository,
    restored: Result<(), Unfinished>,
    during: crate::HeldDuring,
    error: git2::Error,
) -> E
where
    E: From<git2::Error> + From<crate::HeldOpen>,
{
    match restored {
        Ok(()) => crate::worktree_guard::held_open::<E>(repo.workdir(), during)(error),
        Err(_) => error.into(),
    }
}

#[cfg(unix)]
fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(path: &Path) -> Vec<u8> {
    path.to_string_lossy().replace('\\', "/").into_bytes()
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A repository with `c.txt` committed and staged; `c.txt` and `n/new.txt` as a stopped
    /// checkout left them: `c.txt` removed, `n/new.txt` created in the folder `n`, which is
    /// then read-only so `n/new.txt` can't be removed. Both are recorded as written.
    /// `None` as root, who ignores the mode (loud, and fails on CI).
    fn stopped_with_an_unremovable_created_file() -> Option<(tempfile::TempDir, Repository, Written)>
    {
        if unsafe { libc::geteuid() } == 0 {
            assert!(
                std::env::var_os("CI").is_none(),
                "running as root on CI: the read-only test would prove nothing"
            );
            eprintln!("SKIP: running as root, who ignores read-only folders");
            return None;
        }
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("c.txt"), "c\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("c.txt")).unwrap();
        index.write().unwrap();
        std::fs::remove_file(dir.path().join("c.txt")).unwrap();
        std::fs::create_dir(dir.path().join("n")).unwrap();
        std::fs::write(dir.path().join("n/new.txt"), "new\n").unwrap();
        std::fs::set_permissions(dir.path().join("n"), std::fs::Permissions::from_mode(0o555))
            .unwrap();
        let written = Written::default();
        written.push(b"c.txt".to_vec());
        written.push(b"n/new.txt".to_vec());
        Some((dir, repo, written))
    }

    /// A created file that can't be removed doesn't stop the tracked files from coming back
    /// (#321 review): every path is tried, and the first error is returned at the end.
    #[test]
    fn roll_back_restores_tracked_files_even_when_a_created_one_cant_be_removed() {
        let Some((dir, repo, written)) = stopped_with_an_unremovable_created_file() else {
            return;
        };

        let result = roll_back(&repo, &written);

        std::fs::set_permissions(dir.path().join("n"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        assert!(result.is_err(), "{result:?}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("c.txt")).unwrap(),
            "c\n"
        );
        assert!(dir.path().join("n/new.txt").exists());
    }
}
