use std::cell::RefCell;

use git2::Repository;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiffError {
    #[error("failed to compute diff: {0}")]
    Diff(#[from] git2::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineOrigin {
    Add,
    Remove,
    Context,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub origin: DiffLineOrigin,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

/// Diff for one path in the working tree. `staged: true` diffs HEAD's tree against the index
/// (what `git diff --cached -- path` shows); `staged: false` diffs the index against the
/// working directory (what `git diff -- path` shows).
pub fn working_diff(
    repo: &Repository,
    path: &str,
    staged: bool,
) -> Result<Vec<DiffHunk>, DiffError> {
    let mut opts = git2::DiffOptions::new();
    opts.pathspec(path);

    let diff = if staged {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
    } else {
        repo.diff_index_to_workdir(None, Some(&mut opts))?
    };

    hunks_from_diff(&diff)
}

/// Diff for one path between `commit_id` and its first parent (or an empty tree, for a
/// commit with no parent). Merge commits are diffed against their first parent only — no
/// combined/conflict view; fine, since Phase 1 has no merge UI.
pub fn commit_diff(
    repo: &Repository,
    commit_id: &str,
    path: &str,
) -> Result<Vec<DiffHunk>, DiffError> {
    let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
    let mut opts = git2::DiffOptions::new();
    opts.pathspec(path);
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;
    hunks_from_diff(&diff)
}

/// Paths changed by `commit_id` (vs. its first parent, same rule as `commit_diff`) — used to
/// build a commit's file list before diffing any individual file.
pub fn commit_files(repo: &Repository, commit_id: &str) -> Result<Vec<String>, DiffError> {
    let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;

    let mut paths = Vec::new();
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned());
        if let Some(path) = path {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn commit_and_parent_trees<'repo>(
    repo: &'repo Repository,
    commit_id: &str,
) -> Result<(Option<git2::Tree<'repo>>, git2::Tree<'repo>), DiffError> {
    let oid = git2::Oid::from_str(commit_id)?;
    let commit = repo.find_commit(oid)?;
    let new_tree = commit.tree()?;
    let old_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());
    Ok((old_tree, new_tree))
}

fn hunks_from_diff(diff: &git2::Diff) -> Result<Vec<DiffHunk>, DiffError> {
    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());

    diff.foreach(
        &mut |_delta, _progress| true,
        None,
        Some(&mut |_delta, hunk| {
            hunks.borrow_mut().push(DiffHunk {
                old_start: hunk.old_start(),
                old_lines: hunk.old_lines(),
                new_start: hunk.new_start(),
                new_lines: hunk.new_lines(),
                lines: Vec::new(),
            });
            true
        }),
        Some(&mut |_delta, _hunk, line| {
            let origin = match line.origin() {
                '+' => DiffLineOrigin::Add,
                '-' => DiffLineOrigin::Remove,
                _ => DiffLineOrigin::Context,
            };
            let content = String::from_utf8_lossy(line.content()).into_owned();
            if let Some(last) = hunks.borrow_mut().last_mut() {
                last.lines.push(DiffLine { origin, content });
            }
            true
        }),
    )?;

    Ok(hunks.into_inner())
}
