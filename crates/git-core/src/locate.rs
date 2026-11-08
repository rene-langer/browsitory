//! "Go to commit" (#441): where a revision's commit sits in the graph, or why the graph does not
//! show it.

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
    for (index, oid) in revwalk.enumerate() {
        if oid? == target {
            position = Some(index);
            break;
        }
    }

    let (branches, remote_branches) = if position.is_some() {
        (Vec::new(), Vec::new())
    } else {
        (
            containing_branches(repo, BranchType::Local, target)?,
            containing_branches(repo, BranchType::Remote, target)?,
        )
    };
    Ok(Some(CommitLocation {
        commit_id: target.to_string(),
        position,
        branches,
        remote_branches,
    }))
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
        // An upper-case SHA prefix (copied from elsewhere) names the same commit.
        Err(error)
            if matches!(error.code(), ErrorCode::NotFound | ErrorCode::InvalidSpec)
                && revision.chars().all(|c| c.is_ascii_hexdigit())
                && revision.chars().any(|c| c.is_ascii_uppercase()) =>
        {
            return resolve(repo, &revision.to_ascii_lowercase());
        }
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
