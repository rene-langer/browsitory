//! "Go to commit" (#441): where a revision's commit sits in the graph, or why the graph does not
//! show it.

use std::collections::HashMap;

use git2::{BranchType, ErrorCode, Oid, ReferenceType, Repository, Sort};
use thiserror::Error;

use crate::graph::GraphSelection;

#[derive(Debug, Error)]
pub enum LocateError {
    #[error("\"{0}\" matches more than one commit. Type more of the SHA.")]
    Ambiguous(String),
    #[error("failed to find commit: {0}")]
    Git(#[from] git2::Error),
}

/// A revision's commit and its place in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitLocation {
    pub commit_id: String,
    /// Its row in the graph of the selection (0 = the first commit `graph_log` returns), or
    /// `None` when that graph never reaches it however much history is loaded.
    pub position: Option<usize>,
    /// Only when `position` is `None`: the local branches whose tip is or contains the commit,
    /// sorted. A branch already in the selection appears when the graph walks its first parents
    /// only (one selected branch) and the commit is on a merged side branch.
    pub branches: Vec<String>,
    /// Like `branches`, the remote-tracking branches as `<remote>/<branch>`.
    pub remote_branches: Vec<String>,
    /// Only when `position` is `None` and the graph follows first parents ("Collapse merges" or
    /// one selected branch): the merge row that holds the commit, so expanding it shows it
    /// (#617). `None` when no merge of the graph brought it in.
    pub merge: Option<CollapsedMerge>,
}

/// Where a commit hidden in a collapsed merge sits (#617).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollapsedMerge {
    pub merge_id: String,
    /// The merge's row in the graph (as [`CommitLocation::position`]).
    pub merge_position: usize,
    /// The commit's index in [`crate::graph::merged_commits`] of that merge: expanding it with
    /// a limit above `offset` lists the commit.
    pub offset: usize,
}

/// Resolves `revision` (a SHA or SHA prefix, a branch or tag name, or any other revision git
/// understands) to a commit and finds it in the graph of `selection`: the same walk as
/// [`crate::graph::graph_log`]. `Ok(None)` when it names no commit (unknown, malformed, or a
/// tree or blob). An ambiguous SHA prefix is an error.
pub fn locate_commit(
    repo: &Repository,
    revision: &str,
    selection: &GraphSelection,
) -> Result<Option<CommitLocation>, LocateError> {
    let Some(target) = resolve(repo, revision.trim())? else {
        return Ok(None);
    };

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    if selection.walks_first_parent() {
        revwalk.simplify_first_parent()?;
    }
    for oid in selection.start_oids(repo)? {
        revwalk.push(oid)?;
    }
    let mut position = None;
    let mut rows = HashMap::new();
    for (index, oid) in revwalk.enumerate() {
        let oid = oid?;
        if oid == target {
            position = Some(index);
            break;
        }
        rows.insert(oid, index);
    }

    let (branches, remote_branches) = if position.is_some() {
        (Vec::new(), Vec::new())
    } else {
        (
            containing_branches(repo, BranchType::Local, target)?,
            containing_branches(repo, BranchType::Remote, target)?,
        )
    };
    let merge = if position.is_none() && selection.walks_first_parent() {
        collapsed_merge(repo, target, &selection.start_oids(repo)?, &rows)?
    } else {
        None
    };
    Ok(Some(CommitLocation {
        commit_id: target.to_string(),
        position,
        branches,
        remote_branches,
        merge,
    }))
}

/// The merge row of a first-parent graph that brought `target` in, if any (#617). Along one
/// tip's first-parent line the commits that reach `target` come first, then none do: the last
/// one that does is a merge whose other parents hold it. Found by a binary search over the line,
/// so a long history costs a few reachability checks, not one per merge.
fn collapsed_merge(
    repo: &Repository,
    target: Oid,
    tips: &[Oid],
    rows: &HashMap<Oid, usize>,
) -> Result<Option<CollapsedMerge>, git2::Error> {
    let reaches = |oid: Oid| -> Result<bool, git2::Error> {
        Ok(oid == target || repo.graph_descendant_of(oid, target)?)
    };
    for &tip in tips {
        if !reaches(tip)? {
            continue;
        }
        let mut line = vec![tip];
        let mut commit = repo.find_commit(tip)?;
        while let Ok(parent) = commit.parent(0) {
            line.push(parent.id());
            commit = parent;
        }
        // `line[low]` reaches the target, `line[high]` (past the end) does not.
        let (mut low, mut high) = (0, line.len());
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if reaches(line[middle])? {
                low = middle;
            } else {
                high = middle;
            }
        }
        let merge = repo.find_commit(line[low])?;
        let Some(&merge_position) = rows.get(&merge.id()) else {
            continue;
        };
        if merge.parent_count() < 2 {
            continue;
        }
        let mut walk = crate::graph::merged_walk(repo, &merge)?;
        walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
        for (offset, oid) in walk.enumerate() {
            if oid? == target {
                return Ok(Some(CollapsedMerge {
                    merge_id: merge.id().to_string(),
                    merge_position,
                    offset,
                }));
            }
        }
    }
    Ok(None)
}

fn resolve(repo: &Repository, revision: &str) -> Result<Option<Oid>, LocateError> {
    if revision.is_empty() {
        return Ok(None);
    }
    let object = match repo.revparse_single(revision) {
        Ok(object) => object,
        Err(error) if error.code() == ErrorCode::Ambiguous => {
            return Err(LocateError::Ambiguous(revision.to_string()))
        }
        // An upper-case SHA or SHA prefix needs no special case: libgit2 parses hex digits
        // case-insensitively (its `from_hex` table), on every platform.
        Err(error) if matches!(error.code(), ErrorCode::NotFound | ErrorCode::InvalidSpec) => {
            return Ok(None)
        }
        Err(error) => return Err(error.into()),
    };
    Ok(object.peel_to_commit().ok().map(|commit| commit.id()))
}

/// Branches of `kind` whose tip is `target` or descends from it, sorted. Symbolic refs such as
/// `origin/HEAD` are skipped, as in the graph's badges.
fn containing_branches(
    repo: &Repository,
    kind: BranchType,
    target: Oid,
) -> Result<Vec<String>, git2::Error> {
    let mut names = Vec::new();
    for entry in repo.branches(Some(kind))? {
        let (branch, _) = entry?;
        if branch.get().kind() != Some(ReferenceType::Direct) {
            continue;
        }
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        let Ok(tip) = branch.get().peel_to_commit() else {
            continue;
        };
        if tip.id() == target || repo.graph_descendant_of(tip.id(), target)? {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}
