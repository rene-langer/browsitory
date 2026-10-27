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

pub const MAX_CONTEXT_LINES: u32 = 20;

/// Display options shared by every diff the UI shows. Staging builds its diff through
/// `working_diff_options` with the same `context_lines`, so the `(old_start, new_start)` hunk
/// identity the UI captured always matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOptions {
    pub ignore_whitespace: bool,
    pub context_lines: u32,
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self {
            ignore_whitespace: false,
            context_lines: 3,
        }
    }
}

/// One changed path in a compare. `old_path` is set when rename detection paired a delete and
/// an add; `path` is then the new name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
}

fn apply_display_options(git_opts: &mut git2::DiffOptions, opts: &DiffOptions) {
    git_opts
        .ignore_whitespace(opts.ignore_whitespace)
        .context_lines(opts.context_lines.min(MAX_CONTEXT_LINES));
}

/// Literal pathspec for `path` (and `old_path`, for a rename) plus the display options.
fn literal_path_options(
    path: &str,
    old_path: Option<&str>,
    opts: &DiffOptions,
) -> git2::DiffOptions {
    let mut git_opts = git2::DiffOptions::new();
    // Literal paths, not globs — see `working_diff_options`.
    if let Some(old_path) = old_path {
        git_opts.pathspec(old_path);
    }
    git_opts.pathspec(path).disable_pathspec_match(true);
    apply_display_options(&mut git_opts, opts);
    git_opts
}

/// Pairs deletes and adds into renames (git's default 50% similarity, no copies).
fn detect_renames(diff: &mut git2::Diff) -> Result<(), DiffError> {
    let mut find = git2::DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))?;
    Ok(())
}

fn changed_files(diff: &git2::Diff) -> Vec<ChangedFile> {
    let lossy = |p: &std::path::Path| p.to_string_lossy().into_owned();
    diff.deltas()
        .filter_map(|delta| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(lossy)?;
            let old_path = if delta.status() == git2::Delta::Renamed {
                delta.old_file().path().map(lossy)
            } else {
                None
            };
            Some(ChangedFile { path, old_path })
        })
        .collect()
}

fn commit_tree<'repo>(
    repo: &'repo Repository,
    commit_id: &str,
) -> Result<git2::Tree<'repo>, DiffError> {
    let oid = git2::Oid::from_str(commit_id)?;
    Ok(repo.find_commit(oid)?.tree()?)
}

/// Single source of the git2 options for working-tree diffs (display *and* staging).
pub(crate) fn working_diff_options(
    path: &str,
    staged: bool,
    opts: &DiffOptions,
) -> git2::DiffOptions {
    let mut git_opts = git2::DiffOptions::new();
    // `pathspec` is a glob pattern by default, so a real filename containing `*`, `?` or `[`
    // would match unintended files. Callers always pass a literal path from `status()`/
    // `commit_files()`, so match it literally.
    git_opts.pathspec(path).disable_pathspec_match(true);
    if !staged {
        // `status()` deliberately reports untracked files (`include_untracked(true)`), so an
        // untracked path shows up in the UI as an unstaged `New` row and gets diffed here.
        // Plain `git diff` emits nothing for such a path; without these options the pane would
        // render an empty "No differences" state for a brand-new file — the single most common
        // first thing a user clicks. Show the whole file as added instead.
        git_opts
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .show_untracked_content(true);
    }
    apply_display_options(&mut git_opts, opts);
    git_opts
}

/// Diff for one path in the working tree. `staged: true` diffs HEAD's tree against the index
/// (what `git diff --cached -- path` shows); `staged: false` diffs the index against the
/// working directory (what `git diff -- path` shows, *plus* untracked files — see
/// `working_diff_options`).
pub fn working_diff(
    repo: &Repository,
    path: &str,
    staged: bool,
    opts: &DiffOptions,
) -> Result<Vec<DiffHunk>, DiffError> {
    let mut git_opts = working_diff_options(path, staged, opts);

    let diff = if staged {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut git_opts))?
    } else {
        repo.diff_index_to_workdir(None, Some(&mut git_opts))?
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
    opts: &DiffOptions,
) -> Result<Vec<DiffHunk>, DiffError> {
    let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
    let mut git_opts = literal_path_options(path, None, opts);
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut git_opts))?;
    hunks_from_diff(&diff)
}

/// Diff of a file that `commit_id` renamed from `old_path` to `new_path`, against the first
/// parent, with rename detection so edits show as edits rather than a whole-file add. A pure
/// rename yields no hunks.
pub fn commit_rename_diff(
    repo: &Repository,
    commit_id: &str,
    old_path: &str,
    new_path: &str,
    opts: &DiffOptions,
) -> Result<Vec<DiffHunk>, DiffError> {
    let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
    let mut git_opts = literal_path_options(new_path, Some(old_path), opts);
    let mut diff =
        repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut git_opts))?;
    detect_renames(&mut diff)?;
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

/// Paths that differ between the trees of `from_id` and `to_id` (`git diff from to`), with
/// renames detected. The commits need not be related.
pub fn range_diff_files(
    repo: &Repository,
    from_id: &str,
    to_id: &str,
) -> Result<Vec<ChangedFile>, DiffError> {
    let old_tree = commit_tree(repo, from_id)?;
    let new_tree = commit_tree(repo, to_id)?;
    let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)?;
    detect_renames(&mut diff)?;
    Ok(changed_files(&diff))
}

/// One file's diff between `from_id` and `to_id`. With `old_path` (a rename reported by
/// `range_diff_files`), both paths are diffed with rename detection, like `commit_rename_diff`.
pub fn range_diff(
    repo: &Repository,
    from_id: &str,
    to_id: &str,
    path: &str,
    old_path: Option<&str>,
    opts: &DiffOptions,
) -> Result<Vec<DiffHunk>, DiffError> {
    let old_tree = commit_tree(repo, from_id)?;
    let new_tree = commit_tree(repo, to_id)?;
    let mut git_opts = literal_path_options(path, old_path, opts);
    let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut git_opts))?;
    if old_path.is_some() {
        detect_renames(&mut diff)?;
    }
    hunks_from_diff(&diff)
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

pub(crate) fn hunks_from_diff(diff: &git2::Diff) -> Result<Vec<DiffHunk>, DiffError> {
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
