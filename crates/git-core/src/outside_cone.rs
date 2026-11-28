//! The files outside a sparse checkout (skip-worktree entries, missing from disk) around an
//! operation that libgit2 runs as if there were none: a merge, cherry-pick, revert, rebase step
//! or checkout takes a missing marked file it changes for a local deletion and refuses, and
//! writes a file it adds outside the sparse patterns, unmarked (#1184, #1185, #1191).
//!
//! [`OutsideCone`] brings the marked paths the operation changes into the work tree before it
//! (unmarked, their missing files written from the index), so libgit2 merges or checks them out
//! as any file, and takes them out again afterwards: each path whose index entry is the version
//! the operation was to leave, as git leaves it, is marked and its file removed. A conflicted
//! path keeps its written file, unmarked, as git's merge leaves it.

use std::collections::HashSet;

use git2::Repository;

use crate::index_marks::SKIP_WORKTREE;

/// A path's blob and mode; `None` when it isn't there.
pub(crate) type Version = Option<(git2::Oid, u32)>;

pub(crate) struct OutsideCone {
    /// The marked paths the operation changes: unmarked before it.
    changed: Vec<Vec<u8>>,
    /// Those of `changed` whose missing file was written before the operation.
    written: Vec<Vec<u8>>,
    /// The other marked paths: marked again afterwards.
    kept: Vec<Vec<u8>>,
    /// The paths to mark afterwards, their files removed, when their stage-0 entry is then the
    /// given version: of `changed`, and new to the index outside the sparse patterns.
    restaged: Vec<(Vec<u8>, Version)>,
}

/// What an operation leaves in the index (and work tree) for each path.
pub(crate) enum Outcome<'a, 'r> {
    /// A checkout of this tree.
    Tree(&'a git2::Tree<'r>),
    /// This merged index (libgit2's in-memory result of a merge or pick): a conflicted path has
    /// no stage-0 entry.
    Index(&'a git2::Index),
}

impl Outcome<'_, '_> {
    /// `path`'s stage-0 version once the operation is done; `None` when it is gone or
    /// conflicted. A folder in a tree has a version no index entry can have.
    fn version(&self, path: &[u8]) -> Version {
        let rel = crate::worktree_guard::path_from_bytes(path);
        match self {
            Self::Tree(tree) => tree
                .get_path(&rel)
                .ok()
                .map(|entry| (entry.id(), entry.filemode() as u32)),
            Self::Index(index) => index.get_path(&rel, 0).map(|entry| (entry.id, entry.mode)),
        }
    }

    /// Every entry the operation leaves, with its version. A conflicted path's are those of
    /// its sides, which no stage-0 entry has.
    fn entries(&self) -> Result<Vec<(Vec<u8>, Version)>, git2::Error> {
        let collect = |index: &git2::Index| {
            index
                .iter()
                .map(|entry| (entry.path, Some((entry.id, entry.mode))))
                .collect()
        };
        match self {
            Self::Tree(tree) => {
                let mut flat = git2::Index::new()?;
                flat.read_tree(tree)?;
                Ok(collect(&flat))
            }
            Self::Index(index) => Ok(collect(index)),
        }
    }
}

/// Runs `op`, an operation that leaves `outcome`, with the files outside the sparse checkout
/// brought in before it and taken out after it ([`OutsideCone`]). Without a sparse checkout
/// (`core.sparseCheckout` off, or no patterns file: git applies none) `op` runs alone. A failed
/// `op` is followed by [`OutsideCone::roll_back`]; its own error is returned.
pub(crate) fn around<T, E>(
    repo: &Repository,
    outcome: Outcome<'_, '_>,
    op: impl FnOnce() -> Result<T, E>,
) -> Result<T, E>
where
    E: From<git2::Error>,
{
    let Some(outside) = OutsideCone::plan(repo, &outcome)? else {
        return op();
    };
    match op() {
        Ok(value) => {
            outside.finish(repo)?;
            Ok(value)
        }
        // The operation's error is what the user needs to read; a failed roll-back leaves only
        // files written from the index, unmarked.
        Err(error) => {
            let _ = outside.roll_back(repo);
            Err(error)
        }
    }
}

impl OutsideCone {
    /// [`Self::prepare`] for an operation that leaves `outcome`, in a sparse checkout with
    /// patterns: a marked path whose version `outcome` changes is `changed`, marked
    /// again afterwards when the operation left that version (git, which drops the mark of a
    /// marked file it finds on disk, marks a changed one again and removes it, as one outside
    /// the patterns); the other marked paths are `kept`; a path new to the index that lies
    /// outside the patterns is marked afterwards. `None` (nothing done) without a sparse
    /// checkout, or when the operation changes no marked path and adds none outside.
    fn plan(repo: &Repository, outcome: &Outcome<'_, '_>) -> Result<Option<Self>, git2::Error> {
        let sparse = crate::sparse::Sparse::read(repo)?;
        if sparse == crate::sparse::Sparse::Off || repo.workdir().is_none() {
            return Ok(None);
        }
        let mut index = repo.index()?;
        index.read(false)?;
        let mut changed = Vec::new();
        let mut kept = Vec::new();
        let mut restaged = Vec::new();
        let mut present = HashSet::new();
        for entry in index.iter() {
            present.insert(entry.path.clone());
            // git drops the mark of a conflicted entry, and none of these operations runs with
            // conflicts.
            if entry.flags_extended & SKIP_WORKTREE == 0 {
                continue;
            }
            let after = outcome.version(&entry.path);
            if after == Some((entry.id, entry.mode)) {
                kept.push(entry.path);
                continue;
            }
            restaged.push((entry.path.clone(), after));
            changed.push(entry.path);
        }
        for (path, version) in outcome.entries()? {
            if !present.contains(&path) && sparse.excludes(&path) {
                restaged.push((path, version));
            }
        }
        if changed.is_empty() && restaged.is_empty() {
            return Ok(None);
        }
        Self::prepare(repo, changed, kept, restaged).map(Some)
    }

    /// Unmarks `changed` and writes those whose file is missing from the index, so the
    /// operation that follows sees them as any file. On a failed write, everything is put back
    /// first.
    pub(crate) fn prepare(
        repo: &Repository,
        changed: Vec<Vec<u8>>,
        kept: Vec<Vec<u8>>,
        restaged: Vec<(Vec<u8>, Version)>,
    ) -> Result<Self, git2::Error> {
        let mut outside = OutsideCone {
            changed,
            written: Vec::new(),
            kept,
            restaged,
        };
        let Some(workdir) = repo.workdir() else {
            return Ok(outside);
        };
        if outside.changed.is_empty() {
            return Ok(outside);
        }
        let mut index = repo.index()?;
        index.read(false)?;
        for path in &outside.changed {
            let rel = crate::worktree_guard::path_from_bytes(path);
            if let Some(mut entry) = index.get_path(&rel, 0) {
                entry.flags_extended &= !SKIP_WORKTREE;
                index.add(&entry)?;
            }
            if workdir.join(&rel).symlink_metadata().is_err() {
                outside.written.push(path.clone());
            }
        }
        index.write()?;
        if !outside.written.is_empty() {
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout
                .safe()
                .recreate_missing(true)
                .disable_pathspec_match(true);
            for path in &outside.written {
                checkout.path(crate::worktree_guard::path_from_bytes(path));
            }
            if let Err(error) = repo.checkout_index(Some(&mut index), Some(&mut checkout)) {
                outside.roll_back(repo)?;
                return Err(error);
            }
        }
        Ok(outside)
    }

    /// After the operation: the marks back on `kept`, and on the paths of `restaged` whose
    /// stage-0 entry is the version given there, their files removed.
    pub(crate) fn finish(&self, repo: &Repository) -> Result<(), git2::Error> {
        let mut index = repo.index()?;
        index.read(true)?;
        let workdir = repo
            .workdir()
            .ok_or_else(|| git2::Error::from_str("a bare repository has no work tree"))?;
        let mut mark = self.kept.clone();
        let mut remove = Vec::new();
        for (path, version) in &self.restaged {
            let rel = crate::worktree_guard::path_from_bytes(path);
            let Some(entry) = index.get_path(&rel, 0) else {
                continue;
            };
            if *version != Some((entry.id, entry.mode)) {
                continue;
            }
            // The operation wrote the file from that blob: a file already there with other
            // content made it refuse (the path was unmarked, or new to the index).
            if workdir.join(&rel).symlink_metadata().is_ok() {
                remove.push(rel);
            }
            mark.push(path.clone());
        }
        crate::index_marks::mark_skip_worktree(&mut index, &mark)?;
        index.write()?;
        for rel in remove {
            crate::worktree_guard::remove_and_prune_beneath(workdir, &rel).map_err(os)?;
        }
        Ok(())
    }

    /// After a refused operation: the files written before it removed (while they still hold
    /// the index's version) and the marks back.
    pub(crate) fn roll_back(&self, repo: &Repository) -> Result<(), git2::Error> {
        let mut index = repo.index()?;
        index.read(true)?;
        if let Some(workdir) = repo.workdir() {
            let config = crate::undo::worktree_config(repo)
                .map_err(|error| git2::Error::from_str(&error.to_string()))?;
            for path in &self.written {
                let rel = crate::worktree_guard::path_from_bytes(path);
                let Some(entry) = index.get_path(&rel, 0) else {
                    continue;
                };
                let name = String::from_utf8_lossy(path).into_owned();
                let on_disk = crate::stash::worktree_blob::<Failure>(
                    repo,
                    workdir,
                    &name,
                    &config,
                    Some(entry.mode),
                )
                .map_err(|failure| failure.0)?;
                if on_disk == Some((entry.id, entry.mode)) {
                    crate::worktree_guard::remove_and_prune_beneath(workdir, &rel).map_err(os)?;
                }
            }
        }
        crate::index_marks::mark_skip_worktree(&mut index, &self.changed)?;
        index.write()?;
        Ok(())
    }
}

/// An I/O error as a libgit2 one of the OS class.
fn os(error: std::io::Error) -> git2::Error {
    git2::Error::new(
        git2::ErrorCode::GenericError,
        git2::ErrorClass::Os,
        error.to_string(),
    )
}

/// `worktree_blob`'s error type: both kinds end up as a libgit2 error.
struct Failure(git2::Error);

impl From<git2::Error> for Failure {
    fn from(error: git2::Error) -> Self {
        Self(error)
    }
}

impl From<std::io::Error> for Failure {
    fn from(error: std::io::Error) -> Self {
        Self(os(error))
    }
}
