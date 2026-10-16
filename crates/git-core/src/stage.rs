use std::path::Path;

use thiserror::Error;

use crate::diff::{working_diff_options, DiffOptions};
use crate::line_patch::{self, Direction, FileRef, HunkRef, LinePatchError, PatchLine};

fn staging_opts(context_lines: u32) -> DiffOptions {
    // Partial staging is disabled in the UI while ignore-whitespace is on, so staging always
    // diffs with whitespace significant.
    DiffOptions {
        ignore_whitespace: false,
        context_lines,
    }
}

#[derive(Debug, Error)]
pub enum StageError {
    #[error("failed to update the index: {0}")]
    Index(#[from] git2::Error),
    #[error("repository has no working directory (bare repository)")]
    NoWorkdir,
    #[error("hunk not found (file changed since the diff was fetched)")]
    HunkNotFound,
    #[error("Select at least one added or removed line.")]
    EmptySelection,
    #[error("Only added or removed lines can be staged.")]
    ContextLineSelected,
    #[error("Couldn't apply just those lines. Try the whole hunk instead.")]
    SelectionNotApplicable,
}

impl From<LinePatchError> for StageError {
    fn from(error: LinePatchError) -> Self {
        match error {
            LinePatchError::OutOfRange => StageError::HunkNotFound,
            LinePatchError::ContextSelected => StageError::ContextLineSelected,
            LinePatchError::Empty => StageError::EmptySelection,
            LinePatchError::NotApplicable => StageError::SelectionNotApplicable,
        }
    }
}

pub fn stage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    let mut index = repo.index()?;
    if workdir.join(path).exists() {
        index.add_path(Path::new(path))?;
    } else {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    Ok(())
}

pub fn unstage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let target = head_commit.as_ref().map(|c| c.as_object());
    repo.reset_default(target, [path])?;
    Ok(())
}

pub fn stage_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
) -> Result<(), StageError> {
    // Same options as `diff::working_diff`'s unstaged branch — the hunk identity the caller
    // passed in was read from that exact diff, so this one must be built identically or the
    // `(old_start, new_start)` pair won't line up with what's on screen.
    let mut opts = working_diff_options(path, false, &staging_opts(context_lines));
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;

    let matched = std::cell::Cell::new(false);
    let mut apply_opts = git2::ApplyOptions::new();
    apply_opts.hunk_callback(|hunk| {
        let is_match =
            matches!(hunk, Some(h) if h.old_start() == old_start && h.new_start() == new_start);
        if is_match {
            matched.set(true);
        }
        is_match
    });
    repo.apply(&diff, git2::ApplyLocation::Index, Some(&mut apply_opts))?;

    if !matched.get() {
        return Err(StageError::HunkNotFound);
    }
    Ok(())
}

pub fn discard_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
) -> Result<(), StageError> {
    let mut opts = working_diff_options(path, false, &staging_opts(context_lines));
    opts.reverse(true);
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;

    let matched = std::cell::Cell::new(false);
    let mut apply_opts = git2::ApplyOptions::new();
    // Same old/new swap as `unstage_hunk` — see its comment.
    apply_opts.hunk_callback(|hunk| {
        let is_match =
            matches!(hunk, Some(h) if h.old_start() == new_start && h.new_start() == old_start);
        if is_match {
            matched.set(true);
        }
        is_match
    });
    repo.apply(&diff, git2::ApplyLocation::WorkDir, Some(&mut apply_opts))?;

    if !matched.get() {
        return Err(StageError::HunkNotFound);
    }
    Ok(())
}

pub fn unstage_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
) -> Result<(), StageError> {
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let mut opts = working_diff_options(path, true, &staging_opts(context_lines));
    opts.reverse(true);
    let diff = repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?;

    let matched = std::cell::Cell::new(false);
    let mut apply_opts = git2::ApplyOptions::new();
    // `reverse(true)` swaps which side is "old" and which is "new", so the hunk this callback
    // sees has old/new flipped relative to the (old_start, new_start) the caller captured from
    // the forward (non-reversed) diff.
    apply_opts.hunk_callback(|hunk| {
        let is_match =
            matches!(hunk, Some(h) if h.old_start() == new_start && h.new_start() == old_start);
        if is_match {
            matched.set(true);
        }
        is_match
    });
    repo.apply(&diff, git2::ApplyLocation::Index, Some(&mut apply_opts))?;

    if !matched.get() {
        return Err(StageError::HunkNotFound);
    }
    Ok(())
}

struct FoundHunk {
    old_absent: bool,
    new_absent: bool,
    mode: u32,
    lines: Vec<PatchLine>,
}

/// Re-walks `diff` the same way `diff::hunks_from_diff` does and returns the lines of the hunk
/// at `(old_start, new_start)` with their raw libgit2 origins, so indices line up with the UI.
fn find_hunk(diff: &git2::Diff, old_start: u32, new_start: u32) -> Result<FoundHunk, StageError> {
    use std::cell::{Cell, RefCell};
    let found: RefCell<Option<FoundHunk>> = RefCell::new(None);
    let in_target = Cell::new(false);
    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |delta, hunk| {
            let is_target = hunk.old_start() == old_start && hunk.new_start() == new_start;
            in_target.set(is_target);
            if is_target {
                let old_absent =
                    matches!(delta.status(), git2::Delta::Untracked | git2::Delta::Added);
                let new_absent = matches!(delta.status(), git2::Delta::Deleted);
                let mode_side = if new_absent {
                    delta.old_file()
                } else {
                    delta.new_file()
                };
                *found.borrow_mut() = Some(FoundHunk {
                    old_absent,
                    new_absent,
                    mode: u32::from(mode_side.mode()),
                    lines: Vec::new(),
                });
            }
            true
        }),
        Some(&mut |_, _, line| {
            if in_target.get() {
                if let Some(h) = found.borrow_mut().as_mut() {
                    h.lines.push(PatchLine {
                        origin: line.origin(),
                        content: line.content().to_vec(),
                    });
                }
            }
            true
        }),
    )?;
    found.into_inner().ok_or(StageError::HunkNotFound)
}

#[allow(clippy::too_many_arguments)]
fn apply_lines(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: &[u32],
    staged_side: bool,
    direction: Direction,
    location: git2::ApplyLocation,
) -> Result<(), StageError> {
    let mut opts = working_diff_options(path, staged_side, &staging_opts(context_lines));
    let diff = if staged_side {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
    } else {
        repo.diff_index_to_workdir(None, Some(&mut opts))?
    };
    let hunk = find_hunk(&diff, old_start, new_start)?;
    let file = FileRef {
        path,
        old_absent: hunk.old_absent,
        new_absent: hunk.new_absent,
        mode: hunk.mode,
    };
    let patch = line_patch::build(
        &file,
        &HunkRef {
            old_start,
            new_start,
        },
        &hunk.lines,
        line_indices,
        direction,
    )?;
    let parsed = git2::Diff::from_buffer(&patch).map_err(|_| StageError::SelectionNotApplicable)?;
    repo.apply(&parsed, location, None)
        .map_err(|_| StageError::SelectionNotApplicable)
}

/// Stages only the hunk lines at `line_indices` (indices into the index→workdir hunk as
/// `diff::working_diff` returns it).
pub fn stage_lines(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: &[u32],
) -> Result<(), StageError> {
    apply_lines(
        repo,
        path,
        old_start,
        new_start,
        context_lines,
        line_indices,
        false,
        Direction::Forward,
        git2::ApplyLocation::Index,
    )
}

/// Unstages only the hunk lines at `line_indices` (indices into the HEAD→index hunk).
pub fn unstage_lines(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: &[u32],
) -> Result<(), StageError> {
    apply_lines(
        repo,
        path,
        old_start,
        new_start,
        context_lines,
        line_indices,
        true,
        Direction::Reverse,
        git2::ApplyLocation::Index,
    )
}

/// Reverts only the hunk lines at `line_indices` in the working tree (indices into the
/// index→workdir hunk).
pub fn discard_lines(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: &[u32],
) -> Result<(), StageError> {
    apply_lines(
        repo,
        path,
        old_start,
        new_start,
        context_lines,
        line_indices,
        false,
        Direction::Reverse,
        git2::ApplyLocation::WorkDir,
    )
}
