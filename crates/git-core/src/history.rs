use std::collections::HashMap;
use std::path::{Component, Path};

use git2::{Delta, DiffFindOptions, ErrorCode, ObjectType, Oid, Repository, Sort, Tree};
use thiserror::Error;

use crate::graph::{GraphCommit, RefDecorations};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryEntry {
    pub commit: GraphCommit,
    /// Path of the file in this commit.
    pub path: String,
    /// Set when this commit renamed `old_path` to `path` (relative to its first parent).
    pub old_path: Option<String>,
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("Path must be relative to the repository root.")]
    InvalidPath,
    #[error("{path} isn't a file in this commit.")]
    NotAFile { path: String },
    #[error("{path} is a binary file, so it has no lines to follow.")]
    NotText { path: String },
    #[error("Lines {start}-{end} aren't in the file, which has {count} lines.")]
    InvalidRange {
        start: usize,
        end: usize,
        count: usize,
    },
    #[error(transparent)]
    Git(#[from] git2::Error),
}

/// Commits reachable from HEAD that changed `path`, newest first, following renames.
///
/// The tracked path is carried from each commit to its parents (renamed where the parent has
/// the file under another name), so a rename on one branch doesn't rename the file on another.
/// A commit is listed when it differs from every parent at its path, so a merge that took one
/// side's version unchanged is hidden. The walk follows every parent (like
/// `git log --full-history`), so a side-branch edit later discarded by a merge is still listed.
pub fn file_history(
    repo: &Repository,
    path: &str,
    limit: usize,
) -> Result<Vec<FileHistoryEntry>, HistoryError> {
    validate_path(path)?;
    let head = match repo.head() {
        Ok(head) => head,
        Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
            return Ok(Vec::new())
        }
        Err(error) => return Err(error.into()),
    };
    let head_id = head.peel_to_commit()?.id();
    let decorations = RefDecorations::load(repo)?;

    let mut walk = repo.revwalk()?;
    // Topological: every child is visited before its parents, so a parent's tracked path is
    // final by the time the walk reaches it.
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    walk.push(head_id)?;

    let mut tracked: HashMap<Oid, String> = HashMap::from([(head_id, path.to_string())]);
    let mut entries = Vec::new();
    for oid in walk {
        if entries.len() >= limit || tracked.is_empty() {
            break;
        }
        let oid = oid?;
        let Some(current_path) = tracked.remove(&oid) else {
            continue;
        };
        let commit = repo.find_commit(oid)?;
        let tree = commit.tree()?;
        let Some(blob) = blob_at(&tree, &current_path) else {
            continue;
        };

        let mut same_as_a_parent = false;
        let mut renamed_from = None;
        for (index, parent) in commit.parents().enumerate() {
            let parent_tree = parent.tree()?;
            let parent_path = match blob_at(&parent_tree, &current_path) {
                Some(parent_blob) => {
                    same_as_a_parent |= parent_blob == blob;
                    Some(current_path.clone())
                }
                None => {
                    let source = rename_source(repo, &parent_tree, &tree, &current_path)?;
                    if index == 0 {
                        renamed_from.clone_from(&source);
                    }
                    source
                }
            };
            if let Some(parent_path) = parent_path {
                // First (nearest) child wins when two children track the same parent.
                tracked.entry(parent.id()).or_insert(parent_path);
            }
        }

        if !same_as_a_parent {
            entries.push(FileHistoryEntry {
                commit: decorations.graph_commit(&commit),
                path: current_path,
                old_path: renamed_from,
            });
        }
    }
    Ok(entries)
}

pub(crate) fn validate_path(path: &str) -> Result<(), HistoryError> {
    let relative = Path::new(path);
    if path.is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(HistoryError::InvalidPath);
    }
    Ok(())
}

/// Blob id and file mode at `path`, or `None` when absent or not a file (e.g. a directory).
pub(crate) fn blob_at(tree: &Tree, path: &str) -> Option<(Oid, i32)> {
    let entry = tree.get_path(Path::new(path)).ok()?;
    (entry.kind() == Some(git2::ObjectType::Blob)).then(|| (entry.id(), entry.filemode()))
}

/// The path `new_path` was renamed from between `old` and `new`, if libgit2 sees a rename.
pub(crate) fn rename_source(
    repo: &Repository,
    old: &Tree,
    new: &Tree,
    new_path: &str,
) -> Result<Option<String>, git2::Error> {
    let mut diff = repo.diff_tree_to_tree(Some(old), Some(new), None)?;
    let mut find = DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))?;
    Ok(diff
        .deltas()
        .find(|d| d.status() == Delta::Renamed && d.new_file().path() == Some(Path::new(new_path)))
        .and_then(|d| {
            d.old_file()
                .path()
                .map(|p| p.to_string_lossy().into_owned())
        }))
}

/// The tree entries along a file's path in one commit, from the root tree's entry down to the
/// file's own (`a`, `a/b`, `a/b/c` for `a/b/c`), each as (id, file mode). Where a parent's
/// entry equals the child's, everything below it is equal too, so the walks compare these
/// top-down and read a parent's folders only where they differ (#838).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathEntries(Vec<(Oid, i32)>);

impl PathEntries {
    /// The file's blob id and mode.
    pub(crate) fn blob(&self) -> (Oid, i32) {
        *self.0.last().expect("a path has at least one component")
    }
}

/// The entries along `path` in the tree `tree`, or `None` when a component is missing, a folder
/// component isn't a folder, or the last one isn't a file (as [`blob_at`]).
pub(crate) fn path_entries(
    repo: &Repository,
    tree: Oid,
    path: &str,
) -> Result<Option<PathEntries>, git2::Error> {
    entries_below(repo, &repo.find_tree(tree)?, &components(path), Vec::new())
}

/// The entries along `path` in a parent's root tree `parent_tree`, given the child's entries
/// `child` for the same path. Reads only the parent's folders whose ids differ from the child's.
pub(crate) fn parent_path_entries(
    repo: &Repository,
    parent_tree: Oid,
    child: &PathEntries,
    path: &str,
) -> Result<Option<PathEntries>, git2::Error> {
    let _ = child;
    path_entries(repo, parent_tree, path)
}

fn components(path: &str) -> Vec<&str> {
    path.split('/').collect()
}

fn entries_below(
    repo: &Repository,
    tree: &Tree,
    names: &[&str],
    mut found: Vec<(Oid, i32)>,
) -> Result<Option<PathEntries>, git2::Error> {
    let Some((name, rest)) = names.split_first() else {
        return Ok(Some(PathEntries(found)));
    };
    let Some(entry) = tree.get_name(name) else {
        return Ok(None);
    };
    let expected = if rest.is_empty() {
        ObjectType::Blob
    } else {
        ObjectType::Tree
    };
    if entry.kind() != Some(expected) {
        return Ok(None);
    }
    found.push((entry.id(), entry.filemode()));
    if rest.is_empty() {
        return Ok(Some(PathEntries(found)));
    }
    let subtree = repo.find_tree(entry.id())?;
    entries_below(repo, &subtree, rest, found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tree_with(repo: &Repository, entries: &[(&str, Oid, i32)]) -> Oid {
        let mut builder = repo.treebuilder(None).unwrap();
        for (name, id, mode) in entries {
            builder.insert(name, *id, *mode).unwrap();
        }
        builder.write().unwrap()
    }

    fn loose_object(repo: &Repository, id: Oid) -> std::path::PathBuf {
        let hex = id.to_string();
        repo.path().join("objects").join(&hex[..2]).join(&hex[2..])
    }

    #[test]
    fn a_parent_folder_equal_to_the_childs_is_not_read() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let blob = repo.blob(b"f\n").unwrap();
        let other = repo.blob(b"other\n").unwrap();
        let inner = tree_with(&repo, &[("f.txt", blob, 0o100644)]);
        let folder = tree_with(&repo, &[("b", inner, 0o040000)]);
        let child_root = tree_with(&repo, &[("a", folder, 0o040000), ("x", blob, 0o100644)]);
        let parent_root = tree_with(&repo, &[("a", folder, 0o040000), ("x", other, 0o100644)]);
        let child = path_entries(&repo, child_root, "a/b/f.txt")
            .unwrap()
            .unwrap();
        // The shared folder `a` (and `a/b` in it) is no longer in the object database: equal
        // ids at `a` must answer without reading below it.
        for id in [folder, inner] {
            let path = loose_object(&repo, id);
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            std::fs::set_permissions(&path, permissions).unwrap();
            std::fs::remove_file(path).unwrap();
        }
        let repo = Repository::open(dir.path()).unwrap();

        let parent = parent_path_entries(&repo, parent_root, &child, "a/b/f.txt").unwrap();

        assert_eq!(parent, Some(child));
    }

    #[test]
    fn a_parent_reads_only_the_folders_that_differ() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let blob = repo.blob(b"f\n").unwrap();
        let edited = repo.blob(b"edited\n").unwrap();
        let inner = tree_with(&repo, &[("f.txt", blob, 0o100644)]);
        let inner_edited = tree_with(&repo, &[("f.txt", edited, 0o100644)]);
        let folder = tree_with(&repo, &[("b", inner, 0o040000)]);
        let folder_edited = tree_with(&repo, &[("b", inner_edited, 0o040000)]);
        let child_root = tree_with(&repo, &[("a", folder_edited, 0o040000)]);
        let parent_root = tree_with(&repo, &[("a", folder, 0o040000)]);
        let child = path_entries(&repo, child_root, "a/b/f.txt")
            .unwrap()
            .unwrap();

        let parent = parent_path_entries(&repo, parent_root, &child, "a/b/f.txt")
            .unwrap()
            .unwrap();

        assert_eq!(parent.blob(), (blob, 0o100644));
        assert_eq!(
            parent,
            PathEntries(vec![
                (folder, 0o040000),
                (inner, 0o040000),
                (blob, 0o100644)
            ])
        );
        // A file where the path needs a folder, or a missing entry, ends the path.
        let file_at_a = tree_with(&repo, &[("a", blob, 0o100644)]);
        assert_eq!(
            parent_path_entries(&repo, file_at_a, &child, "a/b/f.txt").unwrap(),
            None
        );
        let empty = tree_with(&repo, &[]);
        assert_eq!(
            parent_path_entries(&repo, empty, &child, "a/b/f.txt").unwrap(),
            None
        );
    }
}
