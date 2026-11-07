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
    /// how they were. Installs the builder's notify and progress callbacks, which also give a
    /// file with other hard links a name of its own before libgit2 writes it (#420, see
    /// `hardlink`).
    pub(crate) fn record(&self, repo: &Repository, builder: &mut git2::build::CheckoutBuilder<'_>) {
        crate::hardlink::install(repo, builder, Some(self.clone()));
    }

    pub(crate) fn set_workdir(&self, workdir: &Path) {
        self.0.borrow_mut().workdir = Some(workdir.to_path_buf());
    }

    /// Records `path`, which the checkout plans to write, as it is now below `workdir`.
    pub(crate) fn plan(&self, workdir: &Path, path: &Path) {
        let entry = (path_bytes(path), before(workdir, path));
        self.0.borrow_mut().planned.push(entry);
    }

    /// Takes the planned `path` as it is now: it was replaced by a copy of itself before the
    /// checkout wrote anything (#420), which is no write of the checkout's.
    pub(crate) fn restamp(&self, path: &Path) {
        let mut record = self.0.borrow_mut();
        let Some(workdir) = record.workdir.clone() else {
            return;
        };
        let key = path_bytes(path);
        let full = workdir.join(path);
        for (_, before) in record.planned.iter_mut().filter(|(p, _)| *p == key) {
            before.stamp = stamp(&full);
        }
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
    crate::hardlink::on_checkout(repo, &mut builder);
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
pub(crate) fn path_bytes(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
pub(crate) fn path_bytes(path: &Path) -> Vec<u8> {
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

    /// #420 with #329: a planned file with another hard link, given a name of its own before
    /// the checkout wrote anything and then not reached, is not taken for a file the checkout
    /// wrote; the file written before the stop is. The rollback leaves the other name as it was.
    #[test]
    fn a_hardlinked_file_the_stopped_checkout_did_not_reach_is_not_recorded_as_written() {
        if unsafe { libc::geteuid() } == 0 {
            assert!(std::env::var_os("CI").is_none(), "running as root on CI");
            eprintln!("SKIP: running as root, who ignores read-only folders");
            return;
        }
        let dir = tempfile::TempDir::new().unwrap();
        let out = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let commit = |files: &[(&str, &str)]| {
            for (rel, content) in files {
                let full = dir.path().join(rel);
                std::fs::create_dir_all(full.parent().unwrap()).unwrap();
                std::fs::write(full, content).unwrap();
            }
            let mut index = repo.index().unwrap();
            index
                .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                .unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = git2::Signature::now("t", "t@example.com").unwrap();
            let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();
            let id = repo
                .commit(Some("HEAD"), &sig, &sig, "c", &tree, &parents)
                .unwrap();
            repo.find_commit(id).unwrap()
        };
        let base = commit(&[("a.txt", "a\n"), ("d/m.txt", "m\n"), ("z.txt", "z\n")]);
        let side = commit(&[
            ("a.txt", "side a\n"),
            ("d/m.txt", "side m\n"),
            ("z.txt", "side z\n"),
        ]);
        repo.reset(base.as_object(), git2::ResetType::Hard, None)
            .unwrap();
        std::fs::copy(dir.path().join("z.txt"), out.path().join("z")).unwrap();
        // An hour old, so the copy made before the checkout differs from it in any case.
        std::fs::File::options()
            .write(true)
            .open(out.path().join("z"))
            .unwrap()
            .set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
            .unwrap();
        std::fs::remove_file(dir.path().join("z.txt")).unwrap();
        std::fs::hard_link(out.path().join("z"), dir.path().join("z.txt")).unwrap();
        let read_only = |mode| {
            for rel in ["d/m.txt", "d"] {
                let perms =
                    std::fs::Permissions::from_mode(if rel == "d" { mode | 0o111 } else { mode });
                std::fs::set_permissions(dir.path().join(rel), perms).unwrap();
            }
        };
        read_only(0o444);

        let written = Written::default();
        let mut checkout = git2::build::CheckoutBuilder::new();
        written.record(&repo, &mut checkout);
        let result = repo.checkout_tree(side.as_object(), Some(&mut checkout));
        read_only(0o644);

        assert!(result.is_err());
        let paths = written.paths();
        assert!(paths.contains(&b"a.txt".to_vec()), "{paths:?}");
        assert!(!paths.contains(&b"z.txt".to_vec()), "{paths:?}");
        roll_back(&repo, &written).unwrap();
        let read = |path: &Path| std::fs::read_to_string(path).unwrap();
        assert_eq!(read(&dir.path().join("a.txt")), "a\n");
        assert_eq!(read(&dir.path().join("z.txt")), "z\n");
        assert_eq!(read(&out.path().join("z")), "z\n");
        std::fs::write(dir.path().join("z.txt"), "mine\n").unwrap();
        assert_eq!(read(&out.path().join("z")), "z\n");
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
