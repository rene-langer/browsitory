use std::collections::HashMap;
use std::path::{Component, Path};

use git2::{Delta, DiffFindOptions, ErrorCode, Oid, Repository, Sort, Tree};
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

fn validate_path(path: &str) -> Result<(), HistoryError> {
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
fn blob_at(tree: &Tree, path: &str) -> Option<(Oid, i32)> {
    let entry = tree.get_path(Path::new(path)).ok()?;
    (entry.kind() == Some(git2::ObjectType::Blob)).then(|| (entry.id(), entry.filemode()))
}

/// The path `new_path` was renamed from between `old` and `new`, if libgit2 sees a rename.
fn rename_source(
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
