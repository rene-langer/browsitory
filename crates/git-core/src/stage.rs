use std::path::{Component, Path};

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
    #[error("That path is outside the repository.")]
    InvalidPath,
    #[error("Ignore pattern can't be empty or span several lines.")]
    InvalidPattern,
    #[error("{0} is a folder (for example a nested repository) and wasn't discarded.")]
    IsFolder(String),
    #[error("failed to update the working tree: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to read status: {0}")]
    Status(String),
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

fn checked_relative(path: &str) -> Result<&Path, StageError> {
    let p = Path::new(path);
    if p.as_os_str().is_empty()
        || p.is_absolute()
        || p.components().any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(StageError::InvalidPath);
    }
    Ok(p)
}

/// Refuses `rel` when any of its existing leading components (inside `workdir`) is a symlink, so
/// a working-tree write can never land outside the repository through a symlinked folder.
fn ensure_no_symlinked_parent(workdir: &Path, rel: &Path) -> Result<(), StageError> {
    let mut current = workdir.to_path_buf();
    let Some(parent) = rel.parent() else {
        return Ok(());
    };
    for component in parent.components() {
        current.push(component);
        match current.symlink_metadata() {
            Ok(meta) if meta.file_type().is_symlink() => return Err(StageError::InvalidPath),
            Ok(_) => {}
            // Nothing below a missing folder exists either; the write will create it.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

/// Throws away the unstaged changes of one file; staged changes are kept. Untracked files are
/// deleted. Folders (for example an untracked nested repository) are never removed.
pub fn discard_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let rel = checked_relative(path)?;
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    ensure_no_symlinked_parent(workdir, rel)?;
    if repo.index()?.get_path(rel, 0).is_some() {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().path(rel).disable_pathspec_match(true);
        repo.checkout_index(None, Some(&mut checkout))?;
    } else {
        let full = workdir.join(rel);
        if let Ok(meta) = full.symlink_metadata() {
            if meta.is_dir() {
                return Err(StageError::IsFolder(path.to_string()));
            }
            std::fs::remove_file(full)?;
        }
    }
    Ok(())
}

/// Discards every unstaged change (conflicted entries are skipped).
pub fn discard_all_unstaged(repo: &git2::Repository) -> Result<(), StageError> {
    let entries = crate::status::status(repo).map_err(|e| StageError::Status(e.to_string()))?;
    for entry in entries
        .iter()
        .filter(|e| !e.staged && e.kind != crate::status::StatusKind::Conflicted)
    {
        discard_file(repo, &entry.path)?;
    }
    Ok(())
}

/// Appends `pattern` to the repo-root `.gitignore` unless it is already listed.
pub fn ignore_path(repo: &git2::Repository, pattern: &str) -> Result<(), StageError> {
    if pattern.is_empty() || pattern.contains(['\n', '\r']) || pattern.trim() != pattern {
        return Err(StageError::InvalidPattern);
    }
    let file = repo
        .workdir()
        .ok_or(StageError::NoWorkdir)?
        .join(".gitignore");
    if file
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(StageError::InvalidPath);
    }
    let existing = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    if existing.lines().any(|line| line.trim_end() == pattern) {
        return Ok(());
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(pattern);
    next.push('\n');
    std::fs::write(&file, next)?;
    Ok(())
}

pub fn unstage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let target = head_commit.as_ref().map(|c| c.as_object());
    repo.reset_default(target, [path])?;
    Ok(())
}

/// Stages one whole hunk of the index→workdir diff, identified by the `(old_start, new_start)`
/// read from `diff::working_diff(.., staged: false, ..)` with the same `context_lines`.
pub fn stage_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
) -> Result<(), StageError> {
    apply_selection(
        repo,
        path,
        (old_start, new_start),
        context_lines,
        Selection::WholeHunk,
        Target::Stage,
    )
}

/// Reverts one whole hunk of the index→workdir diff in the working tree.
pub fn discard_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
) -> Result<(), StageError> {
    apply_selection(
        repo,
        path,
        (old_start, new_start),
        context_lines,
        Selection::WholeHunk,
        Target::Discard,
    )
}

/// Unstages one whole hunk of the HEAD→index diff.
pub fn unstage_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
) -> Result<(), StageError> {
    apply_selection(
        repo,
        path,
        (old_start, new_start),
        context_lines,
        Selection::WholeHunk,
        Target::Unstage,
    )
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

/// Which lines of the target hunk to apply.
enum Selection<'a> {
    /// Every `+`/`-` line (the whole-hunk operations).
    WholeHunk,
    /// Indices into the hunk's lines as `diff::hunks_from_diff` emits them, plus every line's
    /// content as the caller displayed it; the hunk must still read exactly like that.
    Lines {
        indices: &'a [u32],
        expected: &'a [String],
    },
}

/// The three operations; each fixes which diff the hunk comes from, the patch direction and
/// where it is applied.
#[derive(Clone, Copy)]
enum Target {
    /// index→workdir hunk, forward, onto the index.
    Stage,
    /// HEAD→index hunk, reversed, onto the index.
    Unstage,
    /// index→workdir hunk, reversed, onto the working tree.
    Discard,
}

/// Single path for every hunk and line operation: re-diff exactly as `diff::working_diff` does,
/// find the hunk, build a one-hunk patch from the selection (`line_patch`), apply it.
fn apply_selection(
    repo: &git2::Repository,
    path: &str,
    (old_start, new_start): (u32, u32),
    context_lines: u32,
    selection: Selection,
    target: Target,
) -> Result<(), StageError> {
    let (staged_side, direction, location) = match target {
        Target::Stage => (false, Direction::Forward, git2::ApplyLocation::Index),
        Target::Unstage => (true, Direction::Reverse, git2::ApplyLocation::Index),
        Target::Discard => (false, Direction::Reverse, git2::ApplyLocation::WorkDir),
    };
    let mut opts = working_diff_options(path, staged_side, &staging_opts(context_lines));
    let diff = if staged_side {
        let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
        repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?
    } else {
        repo.diff_index_to_workdir(None, Some(&mut opts))?
    };
    let hunk = find_hunk(&diff, old_start, new_start)?;
    if matches!(target, Target::Discard) {
        let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
        ensure_no_symlinked_parent(workdir, checked_relative(path)?)?;
    }
    let all_changed: Vec<u32>;
    let line_indices = match selection {
        Selection::Lines { indices, expected } => {
            // `(old_start, new_start)` survives edits elsewhere in the hunk, so the indices
            // only mean what the user selected if the content is unchanged.
            let current = hunk
                .lines
                .iter()
                .map(|l| String::from_utf8_lossy(&l.content));
            if !current.eq(expected
                .iter()
                .map(|e| std::borrow::Cow::Borrowed(e.as_str())))
            {
                return Err(StageError::HunkNotFound);
            }
            indices
        }
        Selection::WholeHunk => {
            all_changed = (0..hunk.lines.len() as u32)
                .filter(|&i| matches!(hunk.lines[i as usize].origin, '+' | '-'))
                .collect();
            &all_changed
        }
    };
    let file = FileRef {
        path,
        old_absent: hunk.old_absent,
        new_absent: hunk.new_absent,
        mode: hunk.mode,
        // Emptying the index side of an added/deleted file removes the entry, like `git add`
        // of a deletion. The working tree never loses a file here — deleting an untracked
        // file is `discard_file`'s job, behind its own confirmation.
        allow_delete: !matches!(target, Target::Discard),
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
    let parsed = git2::Diff::from_buffer(&patch).map_err(not_applicable_or_index)?;
    repo.apply(&parsed, location, None)
        .map_err(not_applicable_or_index)
}

/// Only a patch libgit2 can't parse or apply means "this selection doesn't work"; anything else
/// (a locked index, I/O, the object database) is a real failure and is reported as such.
fn not_applicable_or_index(e: git2::Error) -> StageError {
    if e.code() == git2::ErrorCode::ApplyFail || e.class() == git2::ErrorClass::Patch {
        StageError::SelectionNotApplicable
    } else {
        StageError::Index(e)
    }
}

/// Stages only the hunk lines at `line_indices` (indices into the index→workdir hunk as
/// `diff::working_diff` returns it). `expected_lines` is every line's `content` of the hunk as
/// displayed; if the hunk no longer matches it, nothing happens and `HunkNotFound` is returned.
/// The same holds for `unstage_lines` and `discard_lines`.
pub fn stage_lines(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: &[u32],
    expected_lines: &[String],
) -> Result<(), StageError> {
    apply_selection(
        repo,
        path,
        (old_start, new_start),
        context_lines,
        Selection::Lines {
            indices: line_indices,
            expected: expected_lines,
        },
        Target::Stage,
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
    expected_lines: &[String],
) -> Result<(), StageError> {
    apply_selection(
        repo,
        path,
        (old_start, new_start),
        context_lines,
        Selection::Lines {
            indices: line_indices,
            expected: expected_lines,
        },
        Target::Unstage,
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
    expected_lines: &[String],
) -> Result<(), StageError> {
    apply_selection(
        repo,
        path,
        (old_start, new_start),
        context_lines,
        Selection::Lines {
            indices: line_indices,
            expected: expected_lines,
        },
        Target::Discard,
    )
}
