//! Refreshing the index's stat data on a status read (#841), as `git status` does.
//!
//! When a file's time stamps or size info in the index no longer match the disk but its content
//! does (a `touch`, a formatter rewriting a file unchanged, a copied folder), a status read hashes
//! the file to find it unchanged, and keeps doing so on every read until something writes the
//! index's stat data back. [`scan`] writes it back, so the next read trusts the stat data again.
//!
//! libgit2 can write the refresh itself (`update_index`), but it writes the index it read at the
//! start of the scan without checking that the file on disk is still that index: a `git add` in a
//! terminal that lands during the scan would be lost. So the scan reads a scratch copy of the
//! index (a hard link in a folder of its own in the git folder, so nothing is copied), libgit2
//! writes its refresh to the copy, and [`commit`] replaces the real index with it under
//! `index.lock`, only while the real index is still the one the copy was made from.
//!
//! The write is opportunistic: a held `index.lock`, an index that changed meanwhile or any error
//! on the way skips it, and the read answers all the same. It never waits for the lock.

use std::ffi::{c_int, c_void};
use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use git2::Binding;

/// How many bytes at the end of an index file identify its content: its trailer, the hash of
/// what precedes it (the last 20 bytes of a SHA-256 trailer tell versions apart just as well).
const TRAILER_LEN: u64 = 20;

/// `GIT_INDEX_ENTRY_INTENT_TO_ADD` in an entry's extended flags.
const INTENT_TO_ADD: u16 = 1 << 13;

extern "C" {
    /// libgit2's `git_index_set_caps`, which git2 doesn't wrap.
    fn git_index_set_caps(index: *mut c_void, caps: c_int) -> c_int;
}

/// `GIT_INDEX_CAPABILITY_FROM_OWNER`: the case, file mode and symbolic link handling of the
/// repository's configuration, as the index libgit2 opens for a repository has.
const CAPABILITIES_FROM_OWNER: c_int = -1;

/// What identifies an index file's content: its length and trailer, or with `index.skipHash`
/// (a trailer of zeros) its whole content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fingerprint {
    len: u64,
    tail: Vec<u8>,
}

impl Fingerprint {
    pub(crate) fn read(path: &Path) -> io::Result<Self> {
        let mut file = fs::File::open(path)?;
        let len = file.metadata()?.len();
        let mut tail = vec![0; TRAILER_LEN.min(len) as usize];
        file.seek(SeekFrom::Start(len - tail.len() as u64))?;
        file.read_exact(&mut tail)?;
        if tail.iter().all(|&byte| byte == 0) {
            tail.clear();
            file.seek(SeekFrom::Start(0))?;
            file.read_to_end(&mut tail)?;
        }
        Ok(Self { len, tail })
    }
}

/// Replaces the index at `index_path` with `refreshed` under `index.lock`, as git writes an index,
/// while it still is the one `base` identifies. `false` when it isn't written: the lock is held
/// (it is left in place), the index changed or is gone, or a write failed (the lock is removed).
pub(crate) fn commit(index_path: &Path, base: &Fingerprint, refreshed: &[u8]) -> bool {
    let lock_path = lock_path(index_path);
    let Ok(lock) = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    else {
        return false;
    };
    let written = matches!(
        replace(index_path, &lock_path, lock, base, refreshed),
        Ok(true)
    );
    if !written {
        let _ = fs::remove_file(&lock_path);
    }
    written
}

fn replace(
    index_path: &Path,
    lock_path: &Path,
    mut lock: fs::File,
    base: &Fingerprint,
    refreshed: &[u8],
) -> io::Result<bool> {
    if Fingerprint::read(index_path)? != *base {
        return Ok(false);
    }
    lock.write_all(refreshed)?;
    drop(lock);
    fs::rename(lock_path, index_path)?;
    Ok(true)
}

fn lock_path(index_path: &Path) -> PathBuf {
    let mut name = index_path.file_name().unwrap_or_default().to_os_string();
    name.push(".lock");
    index_path.with_file_name(name)
}

/// A status scan of `repo` with the options `status` reads with, and the index it compared the
/// work tree with; the index's stat data is refreshed on the way when it can be (module docs).
pub(crate) fn scan(
    repo: &git2::Repository,
) -> Result<(git2::Statuses<'_>, git2::Index), git2::Error> {
    if let Some(scanned) = refreshing_scan(repo) {
        return Ok(scanned);
    }
    let statuses = repo.statuses(Some(&mut crate::status::status_options()))?;
    Ok((statuses, repo.index()?))
}

/// The scan on a scratch copy of the index, then the refresh committed; `None` when it can't run
/// (the caller then scans without a refresh).
fn refreshing_scan(repo: &git2::Repository) -> Option<(git2::Statuses<'_>, git2::Index)> {
    let git_dir = repo.path();
    let index_path = git_dir.join("index");
    if repo.is_bare() || !index_path.is_file() || lock_path(&index_path).exists() {
        return None;
    }
    let folder = tempfile::Builder::new()
        .prefix("browsitory-index-refresh-")
        .tempdir_in(git_dir)
        .ok()?;
    let scratch_path = folder.path().join("index");
    link_or_copy(&index_path, &scratch_path).ok()?;
    let base = Fingerprint::read(&scratch_path).ok()?;
    let mut scratch = git2::Index::open_ext(&scratch_path, repo.object_format()).ok()?;
    // libgit2 would drop the intent-to-add mark of a refreshed entry; git keeps it.
    if scratch
        .iter()
        .any(|entry| entry.flags_extended & INTENT_TO_ADD != 0)
    {
        return None;
    }

    let own = repo.index().ok()?;
    repo.set_index(&mut scratch).ok()?;
    let restore = Restore { repo, own };
    // SAFETY: `scratch` is a live index; the call only sets its flags from its owner's config.
    if unsafe { git_index_set_caps(scratch.raw().cast(), CAPABILITIES_FROM_OWNER) } < 0 {
        return None;
    }
    let mut options = crate::status::status_options();
    options.update_index(true);
    let statuses = repo.statuses(Some(&mut options)).ok()?;
    drop(restore);

    if Fingerprint::read(&scratch_path).ok().as_ref() != Some(&base) {
        if let Ok(refreshed) = fs::read(&scratch_path) {
            commit(&index_path, &base, &refreshed);
        }
    }
    Some((statuses, scratch))
}

/// Gives the repository its own index back.
struct Restore<'r> {
    repo: &'r git2::Repository,
    own: git2::Index,
}

impl Drop for Restore<'_> {
    fn drop(&mut self) {
        let _ = self.repo.set_index(&mut self.own);
    }
}

/// `to` as a hard link to `from`, or where that fails a copy with `from`'s modification time:
/// libgit2 tells racily clean entries by comparing their times with the index file's.
fn link_or_copy(from: &Path, to: &Path) -> io::Result<()> {
    if fs::hard_link(from, to).is_ok() {
        return Ok(());
    }
    let modified = fs::metadata(from)?.modified()?;
    fs::copy(from, to)?;
    fs::OpenOptions::new()
        .write(true)
        .open(to)?
        .set_modified(modified)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An index file's bytes: `body`, then a 20-byte trailer (`fill` repeated).
    fn index_bytes(body: &str, fill: u8) -> Vec<u8> {
        let mut bytes = body.as_bytes().to_vec();
        bytes.extend([fill; 20]);
        bytes
    }

    fn setup(content: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("index");
        std::fs::write(&path, content).unwrap();
        (dir, path)
    }

    #[test]
    fn commit_replaces_an_unchanged_index_and_releases_the_lock() {
        let (dir, path) = setup(&index_bytes("old", 1));
        let base = Fingerprint::read(&path).unwrap();
        let refreshed = index_bytes("new", 2);

        assert!(commit(&path, &base, &refreshed));

        assert_eq!(std::fs::read(&path).unwrap(), refreshed);
        assert!(!dir.path().join("index.lock").exists());
    }

    #[test]
    fn commit_leaves_an_index_that_changed_meanwhile() {
        let (dir, path) = setup(&index_bytes("old", 1));
        let base = Fingerprint::read(&path).unwrap();
        let theirs = index_bytes("theirs", 3);
        std::fs::write(&path, &theirs).unwrap();

        assert!(!commit(&path, &base, &index_bytes("new", 2)));

        assert_eq!(std::fs::read(&path).unwrap(), theirs);
        assert!(!dir.path().join("index.lock").exists());
    }

    /// `index.skipHash` writes a trailer of zeros: the content tells the versions apart.
    #[test]
    fn commit_tells_skip_hash_indexes_apart_by_their_content() {
        let (_dir, path) = setup(&index_bytes("old", 0));
        let base = Fingerprint::read(&path).unwrap();
        let theirs = index_bytes("odd", 0);
        std::fs::write(&path, &theirs).unwrap();

        assert!(!commit(&path, &base, &index_bytes("new", 0)));

        assert_eq!(std::fs::read(&path).unwrap(), theirs);
    }

    #[test]
    fn commit_skips_a_held_lock_and_leaves_it_in_place() {
        let (dir, path) = setup(&index_bytes("old", 1));
        let base = Fingerprint::read(&path).unwrap();
        let lock = dir.path().join("index.lock");
        std::fs::write(&lock, "theirs").unwrap();

        assert!(!commit(&path, &base, &index_bytes("new", 2)));

        assert_eq!(std::fs::read(&path).unwrap(), index_bytes("old", 1));
        assert_eq!(std::fs::read_to_string(&lock).unwrap(), "theirs");
    }

    #[test]
    fn commit_skips_a_missing_index() {
        let (dir, path) = setup(&index_bytes("old", 1));
        let base = Fingerprint::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert!(!commit(&path, &base, &index_bytes("new", 2)));

        assert!(!path.exists());
        assert!(!dir.path().join("index.lock").exists());
    }
}
