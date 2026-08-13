use git2::{IndexConflict, Repository};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MergeError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("failed to write resolved file: {0}")]
    Io(#[from] std::io::Error),
    #[error("repository has no working directory (bare repository)")]
    NoWorkdir,
    #[error("no conflict found for path '{0}'")]
    NoConflict(String),
    #[error("'{0}' is an add/delete conflict, not a text conflict")]
    NotATextConflict(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    UpToDate,
    FastForwarded,
    Merged,
    Conflicted { files: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictSegment {
    Clean { content: String },
    Conflict { ours: String, theirs: String },
}

pub fn start_merge(repo: &Repository, branch_name: &str) -> Result<MergeOutcome, MergeError> {
    let branch_ref = format!("refs/heads/{branch_name}");
    let their_commit = repo.find_reference(&branch_ref)?.peel_to_commit()?;
    let their_annotated = repo.find_annotated_commit(their_commit.id())?;

    let (analysis, _preference) = repo.merge_analysis(&[&their_annotated])?;

    if analysis.is_up_to_date() {
        return Ok(MergeOutcome::UpToDate);
    }

    if analysis.is_fast_forward() {
        // Check out the target tree before moving the branch ref — a refused checkout
        // (modified/untracked files in the way) leaves the repo exactly as it was, matching
        // `branch::switch_branch`'s own ordering for the same reason.
        repo.checkout_tree(their_commit.as_object(), None)?;
        let mut head_ref = repo.head()?;
        head_ref.set_target(their_commit.id(), "merge: Fast-forward")?;
        return Ok(MergeOutcome::FastForwarded);
    }

    repo.merge(&[&their_annotated], None, None)?;

    let index = repo.index()?;
    if index.has_conflicts() {
        let mut files = Vec::new();
        for conflict in index.conflicts()? {
            let conflict = conflict?;
            if let Some(path) = conflict_path(&conflict) {
                if !files.contains(&path) {
                    files.push(path);
                }
            }
        }
        Ok(MergeOutcome::Conflicted { files })
    } else {
        Ok(MergeOutcome::Merged)
    }
}

fn conflict_path(conflict: &IndexConflict) -> Option<String> {
    conflict
        .our
        .as_ref()
        .or(conflict.their.as_ref())
        .or(conflict.ancestor.as_ref())
        .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
}
