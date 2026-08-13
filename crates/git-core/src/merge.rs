use git2::{IndexConflict, Repository, ResetType};
use std::path::Path;
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
    let their_reference = repo.find_reference(&branch_ref)?;
    let their_commit = their_reference.peel_to_commit()?;
    let their_annotated = repo.reference_to_annotated_commit(&their_reference)?;

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

fn find_conflict(repo: &Repository, path: &str) -> Result<IndexConflict, MergeError> {
    let index = repo.index()?;
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        if conflict_path(&conflict).as_deref() == Some(path) {
            return Ok(conflict);
        }
    }
    Err(MergeError::NoConflict(path.to_string()))
}

pub fn conflict_hunks(repo: &Repository, path: &str) -> Result<Vec<ConflictSegment>, MergeError> {
    let conflict = find_conflict(repo, path)?;
    let (ancestor, our, their) = match (&conflict.ancestor, &conflict.our, &conflict.their) {
        (Some(a), Some(o), Some(t)) => (a, o, t),
        _ => return Err(MergeError::NotATextConflict(path.to_string())),
    };

    let result = repo.merge_file_from_index(ancestor, our, their, None)?;
    let content = String::from_utf8_lossy(result.content()).into_owned();

    Ok(parse_conflict_markers(&content))
}

// git2's default `MergeFileOptions` use `GIT_MERGE_FILE_STYLE_MERGE` (not diff3), so a conflict
// block is exactly `<<<<<<< ...\n`ours`\n=======\n`theirs`\n>>>>>>> ...\n` — no separate
// ancestor section to account for.
//
// Iterating with `split_inclusive('\n')` (rather than `.lines()`, which strips line terminators
// including the `\r` of a CRLF ending) means every yielded piece keeps its own trailing `\n` (and
// any preceding `\r`) attached. Segments are therefore joined with `""`, not `"\n"` — each piece
// already carries its own terminator, so this naturally preserves the file's exact original line
// endings, including a final line with no trailing newline at all.
fn parse_conflict_markers(content: &str) -> Vec<ConflictSegment> {
    let mut segments = Vec::new();
    let mut clean_lines: Vec<&str> = Vec::new();
    let mut lines = content.split_inclusive('\n');

    while let Some(line) = lines.next() {
        if line.starts_with("<<<<<<<") {
            if !clean_lines.is_empty() {
                segments.push(ConflictSegment::Clean {
                    content: clean_lines.join(""),
                });
                clean_lines.clear();
            }
            let mut ours_lines = Vec::new();
            for line in lines.by_ref() {
                if line.starts_with("=======") {
                    break;
                }
                ours_lines.push(line);
            }
            let mut theirs_lines = Vec::new();
            for line in lines.by_ref() {
                if line.starts_with(">>>>>>>") {
                    break;
                }
                theirs_lines.push(line);
            }
            segments.push(ConflictSegment::Conflict {
                ours: ours_lines.join(""),
                theirs: theirs_lines.join(""),
            });
        } else {
            clean_lines.push(line);
        }
    }
    if !clean_lines.is_empty() {
        segments.push(ConflictSegment::Clean {
            content: clean_lines.join(""),
        });
    }
    segments
}

pub fn resolve_conflict(
    repo: &Repository,
    path: &str,
    resolved_content: &str,
) -> Result<(), MergeError> {
    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    std::fs::write(workdir.join(path), resolved_content)?;

    let mut index = repo.index()?;
    index.add_path(Path::new(path))?;
    index.write()?;
    Ok(())
}

pub fn abort_merge(repo: &Repository) -> Result<(), MergeError> {
    let head_commit = repo.head()?.peel_to_commit()?;
    repo.reset(head_commit.as_object(), ResetType::Hard, None)?;
    repo.cleanup_state()?;
    Ok(())
}

pub fn merge_message(repo: &Repository) -> Option<String> {
    repo.message().ok()
}

pub fn is_merging(repo: &Repository) -> bool {
    repo.state() == git2::RepositoryState::Merge
}
