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
//! anything is written, and writes git's conflict itself instead (#416,
//! [`check_out_as_conflicts`]). Only a stash apply that restores the index, or whose untracked
//! files meet the merge, refuses.
//!
//! When libgit2 does find the conflict for a file coming in over HEAD's folder, it records the
//! file only as a conflict entry at `c` (stage 3, "added by them"; stages 1 and 3 when HEAD
//! deleted it) next to the folder's entries at stage 0, and its checkout then tries to write the
//! file over the folder ("could not open 'c' for writing: Is a directory", #607). That entry is
//! moved to `c~<label>` the same way, as git moves it.

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

/// The first file/folder collision in `index` (a merge result): an entry `p` while another
/// entry lives below `p/`, where `p` is a stage-0 entry or a conflict entry without an "ours"
/// side (stage 3, "added by them", or stages 1 and 3, "deleted by us"). libgit2 records the
/// file coming in over HEAD's folder that way, and its checkout of that can write the file over
/// the folder ("Is a directory", #607). A conflict entry `p` at stage 2 with entries below `p/`
/// is libgit2's own record of a file/folder conflict (it checks HEAD's file out as `p~<label>`),
/// so it is not one; nor is a stage-0 file below a conflicted name.
pub(crate) fn find_file_folder_conflict(index: &git2::Index) -> Option<FileFolderConflict> {
    let ours: HashSet<Vec<u8>> = index
        .iter()
        .filter(|entry| stage(entry) == 2)
        .map(|entry| entry.path)
        .collect();
    let files: HashSet<Vec<u8>> = index
        .iter()
        .map(|entry| entry.path)
        .filter(|path| !ours.contains(path))
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

/// Rewrites `index` (a merge result) into git's outcome for a file/folder conflict: each
/// colliding file `p` leaves stage 0 and becomes a conflict entry at the path git moves it to,
/// `p~<our_label>` at stage 2 ("added by us") when it is the file of `ours` (which has it with
/// that id), else `p~<their_label>` at stage 3 ("added by them"). A `/` in a label becomes `_`,
/// as git flattens it (`c~origin_side`). A file libgit2 already recorded as a conflict entry
/// without an "ours" side (#607) keeps its stages (3, or 1 and 3) and moves to
/// `p~<their_label>`. The entries below `p/` stay merged, so the checkout writes the folder's
/// files and the moved file. A name already taken gets `_0`, `_1`... after it, as git does.
/// Returns the moved files' old names (`p`), in index order; nothing when there is no collision.
pub(crate) fn record_as_conflicts(
    index: &mut git2::Index,
    ours: &git2::Tree<'_>,
    our_label: &str,
    their_label: &str,
) -> Result<Vec<String>, git2::Error> {
    let mut files = Vec::new();
    while let Some(conflict) = find_file_folder_conflict(index) {
        let path = std::path::Path::new(&conflict.file);
        let (entries, label) = match index.get_path(path, 0) {
            Some(mut entry) => {
                let is_ours = ours
                    .get_path(path)
                    .map(|found| found.id() == entry.id)
                    .unwrap_or(false);
                let (stage, label) = if is_ours {
                    (2, our_label)
                } else {
                    (3, their_label)
                };
                entry.flags = (entry.flags & !(0x3 << 12)) | (stage << 12);
                (vec![(0, entry)], label)
            }
            None => {
                let entries: Vec<_> = [1, 3]
                    .into_iter()
                    .filter_map(|stage| index.get_path(path, stage).map(|entry| (stage, entry)))
                    .collect();
                (entries, their_label)
            }
        };
        if entries.is_empty() {
            break;
        }
        for (stage, _) in &entries {
            index.remove(path, *stage)?;
        }
        let label = label.replace('/', "_");
        let mut moved = format!("{}~{label}", conflict.file);
        let mut counter = 0;
        while index.iter().any(|other| other.path == moved.as_bytes())
            || ours.get_path(std::path::Path::new(&moved)).is_ok()
        {
            moved = format!("{}~{label}_{counter}", conflict.file);
            counter += 1;
        }
        for (_, mut entry) in entries {
            entry.path = moved.clone().into_bytes();
            index.add(&entry)?;
        }
        files.push(conflict.file);
    }
    Ok(files)
}

/// How [`check_out_as_conflicts`] writes a file/folder conflict.
pub(crate) struct ConflictCheckout<'a, 'repo> {
    /// git's name for HEAD's side (`HEAD`, a stash's `Updated upstream`).
    pub our_label: &'a str,
    /// git's name for the other side (a branch, `<short id> (<subject>)`, `Stashed changes`).
    pub their_label: &'a str,
    /// A stash's untracked files: checked out with the merge, rolled back with it, and left
    /// out of the index. The caller makes sure the merge holds none of their paths.
    pub untracked: Option<&'a git2::Tree<'repo>>,
    /// Leave a file with local changes in the way as it is instead of refusing, as libgit2's
    /// own cherry-pick and revert checkout does (`GIT_CHECKOUT_ALLOW_CONFLICTS`). A merge and a
    /// stash apply refuse then, writing nothing.
    pub skip_local_changes: bool,
}

/// Writes a merge result holding a file/folder collision as git's conflict (see
/// [`record_as_conflicts`]): the repository's index becomes the rewritten `merged`, and a safe
/// checkout writes it (HEAD's file becomes `c~<our_label>`, the folder's files appear), rolled
/// back when it stops part-way (#321). HEAD is the "ours" side. The operation's own state files
/// (`MERGE_HEAD`, `CHERRY_PICK_HEAD`, the message) are the caller's. Returns the repository's
/// index, written.
pub(crate) fn check_out_as_conflicts<E>(
    repo: &git2::Repository,
    mut merged: git2::Index,
    how: ConflictCheckout<'_, '_>,
) -> Result<git2::Index, E>
where
    E: From<git2::Error> + From<crate::HeldOpen>,
{
    let ours = repo.head()?.peel_to_commit()?.tree()?;
    record_as_conflicts(&mut merged, &ours, how.our_label, how.their_label)?;
    let mut untracked_files = git2::Index::new()?;
    if let Some(tree) = how.untracked {
        untracked_files.read_tree(tree)?;
    }
    let written = crate::partial_checkout::Written::default();
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe().allow_conflicts(how.skip_local_changes);
    crate::hardlink::on_checkout(repo, &mut checkout);
    written.record(repo, &mut checkout);
    written.expect_conflicts(repo, &merged);
    let mut index = repo.index()?;
    index.clear()?;
    for entry in merged.iter().chain(untracked_files.iter()) {
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
    if !untracked_files.is_empty() {
        let untracked: HashSet<Vec<u8>> = untracked_files.iter().map(|entry| entry.path).collect();
        let tracked: Vec<_> = index
            .iter()
            .filter(|entry| !untracked.contains(&entry.path))
            .collect();
        index.clear()?;
        for entry in &tracked {
            index.add(entry)?;
        }
    }
    index.write()?;
    Ok(index)
}

/// The conflicted paths of `index` (each conflict once, by its first side's path), sorted.
pub(crate) fn conflicted_files(index: &git2::Index) -> Result<Vec<String>, git2::Error> {
    let mut files: Vec<String> = index
        .conflicts()?
        .filter_map(|conflict| conflict.ok())
        .filter_map(|conflict| conflict.our.or(conflict.their).or(conflict.ancestor))
        .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// git's label for the commit a cherry-pick or rebase step applies, `<short id> (<subject>)`
/// (a revert's is `parent of ` and that).
pub(crate) fn commit_label(commit: &git2::Commit<'_>) -> Result<String, git2::Error> {
    let short = commit.as_object().short_id()?;
    let summary = commit.summary().ok().flatten().unwrap_or("");
    Ok(format!(
        "{} ({summary})",
        short.as_str().unwrap_or_default()
    ))
}

/// Pauses a cherry-pick, revert or rebase step at its file/folder conflict as libgit2's pick
/// pauses at a conflict: `head_file` (`CHERRY_PICK_HEAD`/`REVERT_HEAD`) names `target`, and
/// `MERGE_MSG` holds `message` with git's list of the conflicted `files`.
pub(crate) fn write_step_state(
    repo: &git2::Repository,
    head_file: &str,
    target: git2::Oid,
    message: &str,
    files: &[String],
) -> std::io::Result<()> {
    let mut message = message.to_string();
    if !message.ends_with('\n') {
        message.push('\n');
    }
    message.push_str("\n# Conflicts:\n");
    for file in files {
        message.push_str(&format!("#\t{file}\n"));
    }
    let git_dir = repo.path();
    std::fs::write(git_dir.join(head_file), format!("{target}\n"))?;
    std::fs::write(git_dir.join("MERGE_MSG"), message)
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
    fn a_file_added_by_them_over_a_folder_is_a_collision() {
        // What libgit2 records for a file coming in over HEAD's folder (#607).
        let index = index(&[("c", 3), ("c.txt", 0), ("c/g", 0)]);
        assert_eq!(
            find_file_folder_conflict(&index),
            Some(FileFolderConflict {
                file: "c".into(),
                inside: "c/g".into(),
            })
        );
    }

    #[test]
    fn a_file_deleted_by_us_for_a_folder_is_a_collision() {
        let index = index(&[("c", 1), ("c", 3), ("c/g", 0)]);
        assert!(find_file_folder_conflict(&index).is_some());
    }

    #[test]
    fn a_conflict_with_our_side_next_to_a_folder_is_no_collision() {
        let index = index(&[("c", 1), ("c", 2), ("c", 3), ("c/g", 0)]);
        assert_eq!(find_file_folder_conflict(&index), None);
    }

    #[test]
    fn a_file_added_by_them_moves_to_their_label() {
        let repo_dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(repo_dir.path()).unwrap();
        let empty = repo.treebuilder(None).unwrap().write().unwrap();
        let ours = repo.find_tree(empty).unwrap();
        let mut index = index(&[("c", 1), ("c", 3), ("c/g", 0)]);
        let moved = record_as_conflicts(&mut index, &ours, "HEAD", "topic/file").unwrap();
        assert_eq!(moved, ["c"]);
        let entries: Vec<(String, u16)> = index
            .iter()
            .map(|entry| {
                (
                    String::from_utf8_lossy(&entry.path).into_owned(),
                    stage(&entry),
                )
            })
            .collect();
        assert_eq!(
            entries,
            [
                ("c/g".to_string(), 0),
                ("c~topic_file".to_string(), 1),
                ("c~topic_file".to_string(), 3)
            ]
        );
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
