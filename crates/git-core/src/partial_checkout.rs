//! A libgit2 checkout that stopped part-way (#321): on Windows at a file another program holds
//! open, on any platform at a file or folder it may not write. libgit2 has then written (or
//! removed) the files before that one, and changed their entries in the repository's in-memory
//! index, but not the index on disk, which it writes only once the checkout has finished.
//! Trying the operation again doesn't finish it: the files it wrote now look like local
//! changes. (An error *while* writing a file, such as a full disk, leaves that file truncated;
//! it isn't recorded, so it isn't put back.)
//!
//! [`Written`] records, through libgit2's progress callback, each path the checkout wrote or
//! removed. [`roll_back`] puts those paths back as the index on disk has them, so the operation
//! can be tried again; [`roll_forward`] does the opposite for an abort, putting them in the
//! index as HEAD has them, so the abort can be tried again.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use git2::Repository;

use crate::worktree_guard::GuardSource;

/// The paths a libgit2 checkout wrote or removed, in git's spelling (`/`-separated bytes).
#[derive(Clone, Default)]
pub(crate) struct Written(Rc<RefCell<Vec<Vec<u8>>>>);

impl Written {
    /// Records into `self` each path the checkout `builder` configures writes or removes.
    /// libgit2 reports a path once it is done with it, so the file it stopped at is never
    /// among them.
    pub(crate) fn record(&self, builder: &mut git2::build::CheckoutBuilder<'_>) {
        let written = self.clone();
        builder.progress(move |path, _, _| {
            if let Some(path) = path {
                written.push(path_bytes(path));
            }
        });
    }

    /// Records `path` (git's spelling), which the caller wrote or removed itself.
    pub(crate) fn push(&self, path: Vec<u8>) {
        let mut paths = self.0.borrow_mut();
        // A folder libgit2 removed is reported with a trailing `/`.
        let path = match path.strip_suffix(b"/") {
            Some(folder) => folder.to_vec(),
            None => path,
        };
        if !path.is_empty() {
            paths.push(path);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.borrow().is_empty()
    }

    /// The recorded paths, sorted, each once.
    fn paths(&self) -> Vec<Vec<u8>> {
        let mut paths = self.0.borrow().clone();
        paths.sort();
        paths.dedup();
        paths
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
/// change to a tracked file is lost. A path the index doesn't have (a file the checkout
/// created) is removed, with any folders that emptied. libgit2 also writes over an ignored file
/// in the way of a file it creates (git treats ignored files as expendable too); that file is
/// then removed, not brought back. Nothing is written through a symlink: links in the way are
/// removed first, and the files are checked out from the index by libgit2 (so `core.autocrlf`
/// and other filters apply).
///
/// Every path is tried, also after one failed: the tracked files first, then the created ones
/// (deepest first), then the tracked files once more if a created one was in their way. The
/// first error is returned at the end. `Ok` when nothing was written. The caller holds the
/// operation's locks.
pub(crate) fn roll_back(repo: &Repository, written: &Written) -> Result<(), Unfinished> {
    let paths = written.paths();
    if paths.is_empty() {
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
    // A folder that still holds tracked files is left alone.
    for path in created.iter().rev() {
        let mut below = path.clone();
        below.push(b'/');
        if index.find_prefix(&below).is_ok() {
            continue;
        }
        match crate::worktree_guard::remove_and_prune_beneath(workdir, &path_from_bytes(path)) {
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
