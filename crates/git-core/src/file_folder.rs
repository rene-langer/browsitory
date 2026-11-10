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

/// Rewrites `index` (a merge result) into git's outcome for a file/folder conflict: each
/// colliding file `p` leaves stage 0 and becomes a conflict entry at the path git moves it to,
/// `p~HEAD` at stage 2 ("added by us") when it is HEAD's file (`ours` has it with that id),
/// else `p~<their_label>` at stage 3 ("added by them"). The entries below `p/` stay merged, so
/// the checkout writes the folder's files and the moved file. A name already taken gets `_0`,
/// `_1`... after it, as git does. Returns the moved files' old names (`p`), in index order;
/// nothing when there is no collision.
pub(crate) fn record_as_conflicts(
    index: &mut git2::Index,
    ours: &git2::Tree<'_>,
    their_label: &str,
) -> Result<Vec<String>, git2::Error> {
    let mut files = Vec::new();
    while let Some(conflict) = find_file_folder_conflict(index) {
        let path = std::path::Path::new(&conflict.file);
        let Some(mut entry) = index.get_path(path, 0) else {
            break;
        };
        index.remove(path, 0)?;
        let is_ours = ours
            .get_path(path)
            .map(|found| found.id() == entry.id)
            .unwrap_or(false);
        let (stage, label) = if is_ours {
            (2, "HEAD")
        } else {
            (3, their_label)
        };
        let mut moved = format!("{}~{label}", conflict.file);
        let mut counter = 0;
        while index.iter().any(|other| other.path == moved.as_bytes())
            || ours.get_path(std::path::Path::new(&moved)).is_ok()
        {
            moved = format!("{}~{label}_{counter}", conflict.file);
            counter += 1;
        }
        entry.path = moved.into_bytes();
        entry.flags = (entry.flags & !(0x3 << 12)) | (stage << 12);
        index.add(&entry)?;
        files.push(conflict.file);
    }
    Ok(files)
}

/// Writes a merge result holding a file/folder collision as git's conflict (see
/// [`record_as_conflicts`]): the repository's index becomes the rewritten `merged`, and a safe
/// checkout with conflicts allowed writes it (HEAD's file becomes `c~HEAD`, the folder's files
/// appear), rolled back when it stops part-way (#321). HEAD is the "ours" side. The operation's
/// own state files (`MERGE_HEAD`, `CHERRY_PICK_HEAD`, the message) are the caller's. Returns
/// the repository's index, written.
pub(crate) fn check_out_as_conflicts<E>(
    repo: &git2::Repository,
    mut merged: git2::Index,
    their_label: &str,
) -> Result<git2::Index, E>
where
    E: From<git2::Error> + From<crate::HeldOpen>,
{
    let ours = repo.head()?.peel_to_commit()?.tree()?;
    record_as_conflicts(&mut merged, &ours, their_label)?;
    let written = crate::partial_checkout::Written::default();
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe().allow_conflicts(true);
    crate::hardlink::on_checkout(repo, &mut checkout);
    written.record(repo, &mut checkout);
    written.expect_conflicts(repo, &merged);
    let mut index = repo.index()?;
    index.clear()?;
    for entry in merged.iter() {
        index.add(&entry)?;
    }
    if let Err(error) = repo.checkout_index(Some(&mut index), Some(&mut checkout)) {
        let restored = crate::partial_checkout::roll_back(repo, &written);
        return Err(crate::partial_checkout::report::<E>(
            repo,
            restored,
            crate::HeldDuring::RolledBack,
            error,
        ));
    }
    index.write()?;
    Ok(index)
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
