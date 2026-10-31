use std::path::{Component, Path};

use thiserror::Error;

use crate::diff::{working_diff_options, DiffOptions};
use crate::line_patch::{self, Direction, FileRef, HunkRef, LinePatchError, PatchLine};
use crate::worktree_guard::Leaf;

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
    #[error("'{0}' is beyond a symbolic link.")]
    BeyondSymlink(String),
    #[error("Ignore pattern can't be empty or span several lines.")]
    InvalidPattern,
    #[error("{0} is a folder (for example a nested repository) and wasn't discarded.")]
    IsFolder(String),
    /// A folder on the way to `path` was replaced by the (untracked) file `parent` (#154).
    #[error("Couldn't discard '{path}': '{parent}' is now a file, not a folder. Discard or move '{parent}' first.")]
    FolderIsNowAFile { path: String, parent: String },
    /// "Discard all" went through every listed path but some couldn't be discarded.
    #[error("{message}")]
    DiscardIncomplete {
        failed: Vec<String>,
        message: String,
    },
    #[error("failed to update the working tree: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to read status: {0}")]
    Status(String),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
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

/// Stages the file as it is in the work tree, or its deletion when it is gone. A path below a
/// symlinked folder is refused like git ("beyond a symbolic link"): staging it would read the
/// link's target, possibly outside the repository; once the link itself is staged (it replaces
/// the folder's entries, like `git add -A`) such a path has nothing left to stage. A symlink is
/// staged as a link even when it dangles.
pub fn stage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let rel = checked_relative(path)?;
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    let mut index = repo.index()?;
    if crate::worktree_guard::has_symlinked_ancestor(workdir, path) {
        // Staging the link that replaced the folder (as "Stage all" does first) already took
        // the path out of the index; nothing is left to stage, and nothing is read.
        if index.get_path(rel, 0).is_none() && !has_conflict_entries(&index, path) {
            return Ok(());
        }
        return Err(StageError::BeyondSymlink(path.to_string()));
    }
    let full = workdir.join(rel);
    if let Ok(meta) = full.symlink_metadata() {
        if meta.file_type().is_symlink() {
            // A link that replaced a tracked folder takes the folder's place, like `git add`.
            index.remove_dir(rel, 0)?;
        }
        index.add_path(Path::new(path))?;
    } else {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    Ok(())
}

fn has_conflict_entries(index: &git2::Index, path: &str) -> bool {
    (1..=3).any(|stage| index.get_path(Path::new(path), stage).is_some())
}

/// git's "'d/f' is beyond a symbolic link" for a path below a symlinked folder.
fn refuse_beyond_symlink(workdir: &Path, path: &str) -> Result<(), StageError> {
    if crate::worktree_guard::has_symlinked_ancestor(workdir, path) {
        return Err(StageError::BeyondSymlink(path.to_string()));
    }
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
pub(crate) fn ensure_no_symlinked_parent(workdir: &Path, rel: &Path) -> Result<(), StageError> {
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
    refuse_beyond_symlink(workdir, path)?;
    ensure_no_symlinked_parent(workdir, rel)?;
    let index = repo.index()?;
    if index.get_path(rel, 0).is_some() {
        // libgit2 can't put the folder back over a file ("directory exists"), and git would
        // delete that file, which the user didn't pick. Say what's in the way instead (#154).
        if let Some(parent) = crate::worktree_guard::file_in_place_of_folder(workdir, path)? {
            return Err(StageError::FolderIsNowAFile {
                path: path.to_string(),
                parent,
            });
        }
        // A forced checkout writes through a symlink that replaced the file (#114).
        crate::worktree_guard::prepare_forced_checkout::<StageError, _>(
            repo,
            Some(&crate::worktree_guard::GuardSource::Index(&index)),
            Some(&[path.to_string()]),
        )?;
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().path(rel).disable_pathspec_match(true);
        repo.checkout_index(None, Some(&mut checkout))?;
    } else {
        // Through the held-folder walker, so a folder swapped for a link after the checks above
        // can't redirect the delete outside the repository (#149).
        if !crate::worktree_guard::remove_beneath(workdir, path)? {
            if crate::worktree_guard::has_symlinked_ancestor(workdir, path) {
                return Err(StageError::BeyondSymlink(path.to_string()));
            }
            return Err(StageError::IsFolder(path.to_string()));
        }
    }
    Ok(())
}

/// At most this many paths are named in a `DiscardIncomplete` message.
const MAX_NAMED_FAILURES: usize = 3;

/// Discards the unstaged changes of exactly `paths` (the list the user confirmed), each the way
/// `discard_file` does. Conflicted entries are skipped. Untracked paths go first (each group in
/// the given order), so a file that replaced a folder is deleted before the folder's files are
/// restored (#154). A path that can't be discarded (a folder such as a nested repository, an I/O
/// error) doesn't stop the rest; they are all reported together afterwards as
/// `DiscardIncomplete`.
pub fn discard_all_unstaged(repo: &git2::Repository, paths: &[String]) -> Result<(), StageError> {
    let conflicted: std::collections::HashSet<String> = crate::status::status(repo)
        .map_err(|e| StageError::Status(e.to_string()))?
        .into_iter()
        .filter(|e| e.kind == crate::status::StatusKind::Conflicted)
        .map(|e| e.path)
        .collect();
    let index = repo.index()?;
    let (tracked, untracked): (Vec<&String>, Vec<&String>) = paths
        .iter()
        .filter(|p| !conflicted.contains(*p))
        .partition(|p| index.get_path(Path::new(p.as_str()), 0).is_some());
    drop(index);
    let mut failed = Vec::new();
    let mut any_folder = false;
    let mut in_the_way = None;
    for path in untracked.into_iter().chain(tracked) {
        match discard_file(repo, path) {
            Ok(()) => {}
            Err(StageError::IsFolder(_)) => {
                any_folder = true;
                failed.push(path.clone());
            }
            Err(StageError::FolderIsNowAFile { parent, .. }) => {
                in_the_way = Some(parent);
                failed.push(path.clone());
            }
            Err(_) => failed.push(path.clone()),
        }
    }
    if failed.is_empty() {
        return Ok(());
    }
    let count = failed.len();
    let mut names = failed[..count.min(MAX_NAMED_FAILURES)].join(", ");
    if count > MAX_NAMED_FAILURES {
        names.push_str(&format!(" and {} more", count - MAX_NAMED_FAILURES));
    }
    let noun = if count == 1 { "file" } else { "files" };
    let mut message = format!("Couldn't discard {count} {noun}: {names}.");
    if any_folder {
        message.push_str(" Folders (for example nested repositories) are never discarded.");
    }
    // A single failure names its reason when it is a file in a folder's place (#154).
    if let (1, Some(parent)) = (count, in_the_way) {
        message.push_str(&format!(" '{parent}' is now a file, not a folder."));
    }
    Err(StageError::DiscardIncomplete { failed, message })
}

/// Appends `pattern` to the repo-root `.gitignore` unless it is already listed.
pub fn ignore_path(repo: &git2::Repository, pattern: &str) -> Result<(), StageError> {
    if pattern.is_empty() || pattern.contains(['\n', '\r']) || pattern.trim() != pattern {
        return Err(StageError::InvalidPattern);
    }
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    // Read and written through the work tree walker: never through a link (a linked
    // `.gitignore` is refused), never blocking on or writing into a FIFO (#149).
    let existing = match crate::worktree_guard::read_beneath(workdir, ".gitignore")? {
        Some(Leaf::File { bytes, .. }) => String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        Some(Leaf::Link(_)) => return Err(StageError::InvalidPath),
        None => match std::fs::symlink_metadata(workdir.join(".gitignore")) {
            // Missing: a new file.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            // A folder, FIFO or device: not a file to append to.
            _ => return Err(StageError::InvalidPath),
        },
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
    // A link swapped in meanwhile is removed, never written through.
    if !crate::worktree_guard::write_beneath(workdir, ".gitignore", next.as_bytes())? {
        return Err(StageError::InvalidPath);
    }
    Ok(())
}

pub fn unstage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let target = head_commit.as_ref().map(|c| c.as_object());
    // A staged rename unstages as a whole: resetting only the new path would leave the old one
    // staged as deleted (#137).
    let mut paths = vec![path.to_string()];
    if let Some(old) = staged_rename_source(repo, path)? {
        paths.push(old);
    }
    repo.reset_default(target, paths.iter())?;
    Ok(())
}

/// The old path of the staged rename whose new path is `path`, if any.
fn staged_rename_source(repo: &git2::Repository, path: &str) -> Result<Option<String>, StageError> {
    let entries = crate::status::status(repo).map_err(|e| StageError::Status(e.to_string()))?;
    Ok(entries
        .into_iter()
        .find(|entry| {
            entry.staged && entry.kind == crate::status::StatusKind::Renamed && entry.path == path
        })
        .and_then(|entry| entry.old_path))
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
/// Only `path`'s own delta counts: a literal pathspec `d` also matches `d/…` (#116).
fn find_hunk(
    diff: &git2::Diff,
    path: &str,
    old_start: u32,
    new_start: u32,
) -> Result<FoundHunk, StageError> {
    use std::cell::{Cell, RefCell};
    let found: RefCell<Option<FoundHunk>> = RefCell::new(None);
    let in_target = Cell::new(false);
    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |delta, hunk| {
            let is_target = found.borrow().is_none()
                && hunk.old_start() == old_start
                && hunk.new_start() == new_start
                && crate::diff::delta_is_one_of(&delta, &[path]);
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
    if matches!(target, Target::Stage) {
        let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
        checked_relative(path)?;
        // The index→workdir diff reads through the link; git refuses the path instead.
        refuse_beyond_symlink(workdir, path)?;
    }
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
    let hunk = find_hunk(&diff, path, old_start, new_start)?;
    if matches!(target, Target::Discard) {
        let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
        refuse_beyond_symlink(workdir, path)?;
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
