use std::path::Path;

use thiserror::Error;

use crate::diff::{working_diff_options, DiffOptions};

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
