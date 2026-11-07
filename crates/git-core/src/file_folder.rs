//! The guard against libgit2 merging a file and a folder of the same name without a conflict
//! (#410).
//!
//! libgit2 1.9 finds a file/folder conflict only when the file and the folder's entries are
//! neighbours in the merged, sorted entry list. A name that sorts between them (`c` < `c.txt` <
//! `c/g`: `.` sorts before `/`) hides it: merging a side that deletes `c.txt` and adds `c/g` into
//! a HEAD that adds the file `c` gives an index with both `c` and `c/g` at stage 0 and no
//! conflict. git reports a file/directory conflict there (`c/g` merged, HEAD's file kept as
//! `c~HEAD`). libgit2's checkout of such an index either stops part-way ("directory exists") or
//! leaves the file `c` in the work tree without `c/g`, and the tree written from it drops one of
//! the two, so the commit a cherry-pick, revert or rebase step makes from it silently loses a
//! file. Every operation that writes a libgit2 merge checks the merged index first, before
//! anything is written, and refuses.

use std::collections::HashSet;

/// A merge result holds the file `file` and the entry `inside` below a folder of the same name,
/// which libgit2 merged without a conflict (#410). Nothing was changed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "'{file}' is a file on one side and a folder on the other (with '{inside}'). Browsitory \
     can't merge that safely, so nothing was changed. Run it with git in a terminal, which \
     reports it as a conflict you can resolve."
)]
pub struct FileFolderConflict {
    pub file: String,
    pub inside: String,
}

/// The first file/folder collision in `index` (a merge result): a stage-0 entry `p` while
/// another entry lives below `p/`. A *conflict* entry `p` with entries below `p/` is libgit2's
/// own record of a file/folder conflict (it checks the file out as `p~<label>`), so it is not
/// one; nor is a stage-0 file below a conflicted name.
pub(crate) fn find_file_folder_conflict(index: &git2::Index) -> Option<FileFolderConflict> {
    let files: HashSet<Vec<u8>> = index
        .iter()
        .filter(|entry| stage(entry) == 0)
        .map(|entry| entry.path)
        .collect();
    // `index.iter()` is sorted by path, so the report is the first collision in that order.
    index.iter().find_map(|entry| {
        let path = &entry.path;
        path.iter()
            .enumerate()
            .filter(|(_, byte)| **byte == b'/')
            .map(|(at, _)| &path[..at])
            .find(|prefix| files.contains(*prefix))
            .map(|file| FileFolderConflict {
                file: String::from_utf8_lossy(file).into_owned(),
                inside: String::from_utf8_lossy(path).into_owned(),
            })
    })
}

/// Refuses a merge result libgit2 can't write without losing a file (see the module docs).
/// Call it on the in-memory merge (`merge_commits`, `cherrypick_commit`, `revert_commit`,
/// `merge_trees`) before the operation writes anything.
pub(crate) fn refuse_file_folder_conflict(index: &git2::Index) -> Result<(), FileFolderConflict> {
    match find_file_folder_conflict(index) {
        Some(conflict) => Err(conflict),
        None => Ok(()),
    }
}

fn stage(entry: &git2::IndexEntry) -> u16 {
    (entry.flags >> 12) & 0x3
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An in-memory index with `entries` (path, stage), added in order. libgit2's `add` keeps
    /// an entry's stage; at the same stage it replaces a file by an entry below it, so a real
    /// merge result (`tests/file_folder_conflict.rs`) is what pins `c` next to `c/g`.
    fn index(entries: &[(&str, u16)]) -> git2::Index {
        let mut index = git2::Index::new().unwrap();
        for (path, stage) in entries {
            index
                .add(&git2::IndexEntry {
                    ctime: git2::IndexTime::new(0, 0),
                    mtime: git2::IndexTime::new(0, 0),
                    dev: 0,
                    ino: 0,
                    mode: 0o100644,
                    uid: 0,
                    gid: 0,
                    file_size: 0,
                    id: git2::Oid::ZERO_SHA1,
                    flags: stage << 12,
                    flags_extended: 0,
                    path: path.as_bytes().to_vec(),
                })
                .unwrap();
        }
        index
    }

    #[test]
    fn a_clean_index_has_no_collision() {
        let index = index(&[("a.txt", 0), ("c", 0), ("c.txt", 0), ("d/g", 0)]);
        assert_eq!(find_file_folder_conflict(&index), None);
    }

    #[test]
    fn a_conflicted_file_next_to_a_folder_is_libgit2s_own_conflict() {
        // What libgit2 records when it does find the conflict (`ab` vs `ab/g`).
        let index = index(&[("ab", 2), ("ab/g", 0)]);
        assert_eq!(find_file_folder_conflict(&index), None);
    }

    #[test]
    fn a_name_that_only_starts_like_a_file_is_no_collision() {
        let index = index(&[("c", 0), ("c.d/g", 0), ("cd/g", 0)]);
        assert_eq!(find_file_folder_conflict(&index), None);
    }

    #[test]
    fn a_conflict_below_a_merged_file_is_a_collision() {
        let index = index(&[("c", 0), ("c/d/g", 3)]);
        assert_eq!(
            find_file_folder_conflict(&index),
            Some(FileFolderConflict {
                file: "c".into(),
                inside: "c/d/g".into(),
            })
        );
    }
}
