//! The skip-worktree and intent-to-add marks of index entries, which libgit2 handles differently
//! from git.
//!
//! - **skip-worktree** (`git update-index --skip-worktree`, and every file outside a sparse
//!   checkout's cone): git neither reads nor writes the file. libgit2's status reports such a
//!   file as deleted when it is missing, and its `read_tree` clears the mark on every entry.
//! - **intent-to-add** (`git add -N`): an entry with the empty blob that git treats as not
//!   staged. libgit2's status reports it as a staged new file and `write_tree` commits it.

use std::path::Path;

use git2::IndexEntryExtendedFlag;

pub(crate) const SKIP_WORKTREE: u16 = IndexEntryExtendedFlag::SKIP_WORKTREE.bits();

/// The extended flags of `path`'s stage-0 entry in `index`, 0 when there is none.
pub(crate) fn marks(index: &git2::Index, path: &str) -> u16 {
    index
        .get_path(Path::new(path), 0)
        .map_or(0, |entry| entry.flags_extended)
}

/// True when `path`'s stage-0 entry is marked skip-worktree.
pub(crate) fn is_skip_worktree(index: &git2::Index, path: &Path) -> bool {
    index
        .get_path(path, 0)
        .is_some_and(|entry| entry.flags_extended & SKIP_WORKTREE != 0)
}
