//! The skip-worktree and intent-to-add marks of index entries, which libgit2 handles differently
//! from git.
//!
//! - **skip-worktree** (`git update-index --skip-worktree`, and every file outside a sparse
//!   checkout's cone): git neither reads nor writes the file. libgit2's status reports such a
//!   file as deleted when it is missing, and its `read_tree` clears the mark on every entry.
//! - **intent-to-add** (`git add -N`): an entry with the empty blob that git treats as not
//!   staged. libgit2's status reports it as a staged new file and `write_tree` commits it.

use std::collections::HashSet;
use std::path::Path;

use git2::IndexEntryExtendedFlag;

pub(crate) const SKIP_WORKTREE: u16 = IndexEntryExtendedFlag::SKIP_WORKTREE.bits();
pub(crate) const INTENT_TO_ADD: u16 = IndexEntryExtendedFlag::INTENT_TO_ADD.bits();

/// The tree a commit of `index` records: the index's tree without its intent-to-add entries,
/// which git leaves out of a commit (#1164). Written from an in-memory copy when there are any.
pub(crate) fn committed_tree(
    repo: &git2::Repository,
    index: &mut git2::Index,
) -> Result<git2::Oid, git2::Error> {
    if !index
        .iter()
        .any(|entry| entry.flags_extended & INTENT_TO_ADD != 0)
    {
        return index.write_tree();
    }
    let mut committed = git2::Index::new()?;
    for entry in index
        .iter()
        .filter(|entry| entry.flags_extended & INTENT_TO_ADD == 0)
    {
        committed.add(&entry)?;
    }
    committed.write_tree_to(repo)
}

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

/// The paths of `index`'s skip-worktree entries.
pub(crate) fn skip_worktree_paths(index: &git2::Index) -> Vec<Vec<u8>> {
    index
        .iter()
        .filter(|entry| entry.flags_extended & SKIP_WORKTREE != 0)
        .map(|entry| entry.path)
        .collect()
}

/// The skip-worktree marks of the index once it is reset to a tree, as git's reset sets them.
pub(crate) struct Marks {
    /// Each path to mark: every stage-0 path marked now (#1145), and each path of the tree
    /// without any index entry that lies outside the sparse checkout (#1172).
    pub(crate) skip_worktree: Vec<Vec<u8>>,
    /// The paths among them new to the index: the reset doesn't write their files.
    pub(crate) added: HashSet<Vec<u8>>,
    /// A sparse checkout with readable patterns applies (git applies none without them).
    pub(crate) sparse: bool,
    /// The paths whose file goes because the sparse patterns leave them out (#1187): see
    /// [`apply_sparse_patterns`].
    pub(crate) removed: HashSet<Vec<u8>>,
}

/// [`Marks`] for resetting `index` (as it is now, before anything is written) to `tree`. A
/// path with an index entry keeps its stage-0 mark (a conflicted path ends unmarked); one
/// without gets the mark when the sparse patterns leave it out (`builtin/reset.c`,
/// `path_in_sparse_checkout`).
pub(crate) fn marks_after(
    repo: &git2::Repository,
    index: &git2::Index,
    tree: &git2::Tree<'_>,
) -> Result<Marks, git2::Error> {
    let mut marks = Marks {
        skip_worktree: skip_worktree_paths(index),
        added: HashSet::new(),
        sparse: false,
        removed: HashSet::new(),
    };
    let sparse = crate::sparse::Sparse::read(repo)?;
    if sparse == crate::sparse::Sparse::Off {
        return Ok(marks);
    }
    marks.sparse = true;
    let present: HashSet<Vec<u8>> = index.iter().map(|entry| entry.path).collect();
    let mut flat = git2::Index::new()?;
    flat.read_tree(tree)?;
    for entry in flat.iter() {
        if !present.contains(&entry.path) && sparse.excludes(&entry.path) {
            marks.skip_worktree.push(entry.path.clone());
            marks.added.insert(entry.path);
        }
    }
    Ok(marks)
}

/// The paths a hard reset of `index` to `tree` leaves alone, as `git reset --hard` does
/// (#1145): a skip-worktree file the target keeps (same blob and mode), and in a sparse
/// checkout one that is missing; an intent-to-add file the target doesn't have, which stays as
/// an untracked file; the paths [`Marks::added`] (#1172).
pub(crate) struct LeftAlone {
    pub(crate) paths: HashSet<Vec<u8>>,
    /// The first skip-worktree file with local edits that the target changes (git refuses the
    /// reset: "not uptodate"); it is in `paths` too.
    pub(crate) edited: Option<String>,
}

/// [`LeftAlone`] for a hard reset of `index` to `tree`.
pub(crate) fn left_alone<E>(
    repo: &git2::Repository,
    index: &git2::Index,
    tree: &git2::Tree<'_>,
    marks: &Marks,
) -> Result<LeftAlone, E>
where
    E: From<git2::Error> + From<std::io::Error>,
{
    let mut left = LeftAlone {
        paths: marks.added.clone(),
        edited: None,
    };
    let marked: Vec<git2::IndexEntry> = index
        .iter()
        .filter(|entry| entry.flags_extended & (SKIP_WORKTREE | INTENT_TO_ADD) != 0)
        .collect();
    if marked.is_empty() {
        return Ok(left);
    }
    let (_, edited) = edited_skip_worktree::<E>(repo)?;
    let workdir = repo.workdir();
    for entry in marked {
        let path = crate::worktree_guard::path_from_bytes(&entry.path);
        let target = tree.get_path(&path).ok();
        if entry.flags_extended & SKIP_WORKTREE == 0 {
            if target.is_none() {
                left.paths.insert(entry.path);
            }
            continue;
        }
        let unchanged = target.is_some_and(|target| {
            target.id() == entry.id && u32::try_from(target.filemode()) == Ok(entry.mode)
        });
        let name = String::from_utf8_lossy(&entry.path).into_owned();
        if unchanged {
            left.paths.insert(entry.path);
        } else if edited.contains(&name) {
            left.edited.get_or_insert(name);
            left.paths.insert(entry.path);
        } else if marks.sparse
            && workdir.is_some_and(|workdir| workdir.join(&path).symlink_metadata().is_err())
        {
            left.paths.insert(entry.path);
        }
    }
    Ok(left)
}

/// For a hard reset of `index` to `tree` (Reset hard, Abort rebase) in a sparse checkout: the
/// patterns applied to every entry the index already has, as git's unpack-trees does
/// (`apply_sparse_checkout`, #1187), once [`left_alone`] has judged the marked ones. An unmarked
/// entry outside the patterns gets the mark and its file goes; a marked one inside loses the mark
/// and gets its file; a marked one outside whose unchanged file is on disk loses that file (git
/// drops the mark of a file it finds, then marks it again). A marked file with local edits keeps
/// its mark and stays as it is (git would overwrite or remove it): nothing shows the edit or
/// brings it back. The files go with [`remove_left_out_files`], after the checkout.
pub(crate) fn apply_sparse_patterns<E>(
    repo: &git2::Repository,
    index: &git2::Index,
    tree: &git2::Tree<'_>,
    marks: &mut Marks,
    left: &mut LeftAlone,
) -> Result<(), E>
where
    E: From<git2::Error> + From<std::io::Error>,
{
    if !marks.sparse {
        return Ok(());
    }
    let sparse = crate::sparse::Sparse::read(repo)?;
    let (_, edited) = edited_skip_worktree::<E>(repo)?;
    let workdir = repo.workdir();
    for entry in index.iter() {
        let stage = (entry.flags >> 12) & 0x3;
        let path = crate::worktree_guard::path_from_bytes(&entry.path);
        if stage != 0
            || entry.flags_extended & INTENT_TO_ADD != 0
            || tree.get_path(&path).is_err()
            || edited.contains(&String::from_utf8_lossy(&entry.path).into_owned())
        {
            continue;
        }
        let marked = entry.flags_extended & SKIP_WORKTREE != 0;
        let on_disk =
            || workdir.is_some_and(|workdir| workdir.join(&path).symlink_metadata().is_ok());
        match (marked, sparse.excludes(&entry.path)) {
            (true, false) => {
                marks.skip_worktree.retain(|marked| *marked != entry.path);
                left.paths.remove(&entry.path);
            }
            (false, true) => {
                marks.skip_worktree.push(entry.path.clone());
                left.paths.insert(entry.path.clone());
                marks.removed.insert(entry.path);
            }
            (true, true) if on_disk() => {
                left.paths.insert(entry.path.clone());
                marks.removed.insert(entry.path);
            }
            _ => {}
        }
    }
    Ok(())
}

/// Removes the file of each of [`Marks::removed`], and the folders that empty, as git does. One
/// that can't be removed stays and goes unmarked, as git leaves a file it can't remove.
pub(crate) fn remove_left_out_files(repo: &git2::Repository, marks: &mut Marks) {
    let Some(workdir) = repo.workdir() else {
        return;
    };
    let kept: HashSet<Vec<u8>> = marks
        .removed
        .iter()
        .filter(|path| {
            crate::worktree_guard::remove_and_prune_beneath(
                workdir,
                &crate::worktree_guard::path_from_bytes(path),
            )
            .is_err()
        })
        .cloned()
        .collect();
    marks.skip_worktree.retain(|path| !kept.contains(path));
}

/// `index.read_tree(tree)`, then the skip-worktree mark back on each of `skip_worktree` that is
/// still in the index, as git's reset keeps it (#1145): libgit2 clears it on every entry.
/// Intent-to-add entries go, their files left untracked, as with git.
pub(crate) fn read_tree_marking(
    index: &mut git2::Index,
    tree: &git2::Tree<'_>,
    skip_worktree: &[Vec<u8>],
) -> Result<(), git2::Error> {
    index.read_tree(tree)?;
    mark(index, skip_worktree, SKIP_WORKTREE)
}

/// Marks skip-worktree the stage-0 entry of each of `paths` that is in `index`.
pub(crate) fn mark_skip_worktree(
    index: &mut git2::Index,
    paths: &[Vec<u8>],
) -> Result<(), git2::Error> {
    mark(index, paths, SKIP_WORKTREE)
}

/// Sets `flag` on the stage-0 entry of each of `paths` that is in `index`.
fn mark(index: &mut git2::Index, paths: &[Vec<u8>], flag: u16) -> Result<(), git2::Error> {
    for path in paths {
        let path = crate::worktree_guard::path_from_bytes(path);
        // libgit2 sets the entry's "extended flags" bit itself when it writes the index.
        if let Some(mut entry) = index.get_path(&path, 0) {
            entry.flags_extended |= flag;
            index.add(&entry)?;
        }
    }
    Ok(())
}

/// The paths of `index`'s intent-to-add entries.
pub(crate) fn intent_to_add_paths(index: &git2::Index) -> Vec<Vec<u8>> {
    index
        .iter()
        .filter(|entry| entry.flags_extended & INTENT_TO_ADD != 0)
        .map(|entry| entry.path)
        .collect()
}

/// [`read_tree_marking`], then the index written. On any error the index is read back from
/// disk: it is the repository's shared index object (the worker keeps its `Repository` open),
/// and a later write elsewhere would otherwise save the failed change (#1166).
pub(crate) fn reset_index_to_tree(
    index: &mut git2::Index,
    tree: &git2::Tree<'_>,
    skip_worktree: &[Vec<u8>],
) -> Result<(), git2::Error> {
    reset_index_marking(index, tree, skip_worktree, &[])
}

/// [`reset_index_to_tree`], with the intent-to-add mark also put on each of `intent_to_add`
/// that the tree has (Undo, #1174).
pub(crate) fn reset_index_marking(
    index: &mut git2::Index,
    tree: &git2::Tree<'_>,
    skip_worktree: &[Vec<u8>],
    intent_to_add: &[Vec<u8>],
) -> Result<(), git2::Error> {
    let Err(error) = read_tree_marking(index, tree, skip_worktree)
        .and_then(|()| mark(index, intent_to_add, INTENT_TO_ADD))
        .and_then(|()| index.write())
    else {
        return Ok(());
    };
    match index.read(true) {
        Ok(()) => Err(error),
        Err(reread) => Err(git2::Error::from_str(&format!(
            "{error}; re-reading the index afterwards failed too: {reread}"
        ))),
    }
}

/// Every path of `tree`, HEAD's tree and the index (any stage) for which `except` is false.
pub(crate) fn paths_except(
    repo: &git2::Repository,
    tree: &git2::Tree<'_>,
    except: impl Fn(&[u8]) -> bool,
) -> Result<Vec<Vec<u8>>, git2::Error> {
    let head_tree = repo.head().and_then(|head| head.peel_to_tree()).ok();
    let mut paths = std::collections::BTreeSet::new();
    for tree in std::iter::once(tree).chain(head_tree.as_ref()) {
        let mut flat = git2::Index::new()?;
        flat.read_tree(tree)?;
        paths.extend(flat.iter().map(|entry| entry.path));
    }
    let mut index = repo.index()?;
    index.read(false)?;
    paths.extend(index.iter().map(|entry| entry.path));
    Ok(paths.into_iter().filter(|path| !except(path)).collect())
}

/// Every skip-worktree path in the index, and those whose file exists with content (after
/// git's filters, e.g. `core.autocrlf`) other than the index's blob (#170). A file read through
/// a symlinked folder, or that can't be read, counts as edited.
pub(crate) fn edited_skip_worktree<E>(
    repo: &git2::Repository,
) -> Result<(std::collections::HashSet<Vec<u8>>, Vec<String>), E>
where
    E: From<git2::Error> + From<std::io::Error>,
{
    let mut all = std::collections::HashSet::new();
    let mut edited = Vec::new();
    let Some(workdir) = repo.workdir() else {
        return Ok((all, edited));
    };
    let mut index = repo.index()?;
    index.read(false)?;
    let entries: Vec<_> = index
        .iter()
        .filter(|entry| entry.flags_extended & SKIP_WORKTREE != 0)
        .collect();
    if entries.is_empty() {
        return Ok((all, edited));
    }
    let config = crate::undo::worktree_config(repo)
        .map_err(|error| git2::Error::from_str(&error.to_string()))?;
    for entry in entries {
        all.insert(entry.path.clone());
        if entry.mode == 0o160000 {
            continue;
        }
        let path = String::from_utf8_lossy(&entry.path).into_owned();
        let changed =
            match crate::undo::read_worktree_file(workdir, &path, &config, Some(entry.mode)) {
                Ok(None) => false,
                Ok(Some((bytes, mode))) => {
                    let raw = git2::Oid::hash_object(git2::ObjectType::Blob, &bytes)?;
                    if raw == entry.id {
                        false
                    } else if mode == crate::undo::MODE_LINK {
                        true
                    } else {
                        // Apply git's filters as status would (`core.autocrlf`, attributes); this
                        // writes a loose object, only when the raw bytes differ.
                        crate::undo::filtered_blob::<E>(repo, &path, &bytes)? != entry.id
                    }
                }
                Err(_) => true,
            };
        if changed {
            edited.push(path);
        }
    }
    Ok((all, edited))
}
