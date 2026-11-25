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

/// The paths of `index`'s skip-worktree entries (stage 0).
pub(crate) fn skip_worktree_paths(index: &git2::Index) -> Vec<Vec<u8>> {
    index
        .iter()
        .filter(|entry| (entry.flags >> 12) & 0x3 == 0 && entry.flags_extended & SKIP_WORKTREE != 0)
        .map(|entry| entry.path)
        .collect()
}

/// `index.read_tree(tree)`, then the skip-worktree mark back on each of `skip_worktree` that is
/// still in the index, as git's reset keeps it (#1145): libgit2 clears it on every entry.
/// Intent-to-add entries go, their files left untracked, as with git.
pub(crate) fn read_tree_marking(
    index: &mut git2::Index,
    tree: &git2::Tree<'_>,
    skip_worktree: &[Vec<u8>],
) -> Result<(), git2::Error> {
    // `GIT_INDEX_ENTRY_EXTENDED`: the entry has extended flags.
    const EXTENDED: u16 = 0x4000;
    index.read_tree(tree)?;
    for path in skip_worktree {
        let path = crate::worktree_guard::path_from_bytes(path);
        if let Some(mut entry) = index.get_path(&path, 0) {
            entry.flags |= EXTENDED;
            entry.flags_extended |= SKIP_WORKTREE;
            index.add(&entry)?;
        }
    }
    Ok(())
}

/// [`read_tree_marking`], then the index written. On any error the index is read back from
/// disk: it is the repository's shared index object (the worker keeps its `Repository` open),
/// and a later write elsewhere would otherwise save the failed change (#1166).
pub(crate) fn reset_index_to_tree(
    index: &mut git2::Index,
    tree: &git2::Tree<'_>,
    skip_worktree: &[Vec<u8>],
) -> Result<(), git2::Error> {
    let Err(error) = read_tree_marking(index, tree, skip_worktree).and_then(|()| index.write())
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
