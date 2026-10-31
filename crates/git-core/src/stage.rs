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
    /// A tracked file that is now a folder holding `inner`, which the discard would delete and
    /// Undo's snapshot can't hold (an ignored file, a nested repository) (#154).
    #[error("Couldn't discard '{path}': it is now a folder holding '{inner}', which Undo can't bring back. Move it first.")]
    FolderHoldsUnrestorable { path: String, inner: String },
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

/// Stages the file as it is in the work tree, or its deletion when it is gone. Below a symlinked
/// folder git sees no file at `path` (it is beyond the link), so the path is staged as deleted
/// and nothing is read through the link (#157); its siblings and the link itself stay as they
/// were, and staging the link later still replaces the folder's remaining entries (like `git add
/// -A`). A conflicted path there is refused ("beyond a symbolic link"). A symlink is staged as a
/// link even when it dangles.
pub fn stage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let rel = checked_relative(path)?;
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    let mut index = repo.index()?;
    if crate::worktree_guard::has_symlinked_ancestor(workdir, path) {
        if has_conflict_entries(&index, path) {
            return Err(StageError::BeyondSymlink(path.to_string()));
        }
        // Not in the index (staging the link already took it out): nothing left to stage.
        if index.get_path(rel, 0).is_some() {
            index.remove_path(rel)?;
            index.write()?;
        }
        return Ok(());
    }
    let full = workdir.join(rel);
    if let Ok(meta) = full.symlink_metadata() {
        if meta.is_dir() && folder_replaced_a_file(repo, &index, rel)? {
            // A folder where HEAD or the index has a file or link: git sees that file deleted.
            // Its deletion is what stages here (a conflict there resolves as deleted, as `git
            // add -A`); the folder's files stage on their own (#157).
            if index.get_path(rel, 0).is_some() || has_conflict_entries(&index, path) {
                index.remove_path(rel)?;
                index.write()?;
            }
            return Ok(());
        }
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

/// True when HEAD or the index has a file or link (not a folder, not a submodule) at `rel`.
fn folder_replaced_a_file(
    repo: &git2::Repository,
    index: &git2::Index,
    rel: &Path,
) -> Result<bool, StageError> {
    const GITLINK: u32 = 0o160000;
    if let Some(entry) = (0..=3).find_map(|stage| index.get_path(rel, stage)) {
        return Ok(entry.mode != GITLINK);
    }
    let Ok(head) = repo.head().and_then(|head| head.peel_to_tree()) else {
        return Ok(false);
    };
    Ok(head
        .get_path(rel)
        .is_ok_and(|entry| entry.kind() == Some(git2::ObjectType::Blob)))
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
/// deleted. Folders (for example an untracked nested repository) are never removed. A tracked
/// file that is now a folder is restored and the folder's untracked files deleted, as `git
/// restore` does (Undo brings them back); a folder that holds anything Undo can't bring back
/// (an ignored file, a nested repository) is refused instead (#154).
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
        if let Some(inner) = unrestorable_inside(repo, workdir, path)? {
            return Err(StageError::FolderHoldsUnrestorable {
                path: path.to_string(),
                inner,
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

/// When `path` is now a real folder: the first thing in it that Undo's snapshot leaves out (an
/// ignored file or folder, a nested repository reported as `dir/`), which a forced checkout of
/// `path` would delete for good.
fn unrestorable_inside(
    repo: &git2::Repository,
    workdir: &Path,
    path: &str,
) -> Result<Option<String>, StageError> {
    let is_folder = workdir
        .join(path)
        .symlink_metadata()
        .is_ok_and(|meta| meta.is_dir());
    if !is_folder {
        return Ok(None);
    }
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(true)
        // An ignored folder is reported (and named) as `dir/`, without listing what is in it.
        .recurse_ignored_dirs(false)
        .exclude_submodules(true)
        .disable_pathspec_match(true)
        .pathspec(format!("{path}/"));
    let below = format!("{path}/");
    let statuses = repo.statuses(Some(&mut options))?;
    let found = statuses.iter().find_map(|entry| {
        let inner = entry.path().ok()?;
        let unrestorable = entry.status().contains(git2::Status::IGNORED) || inner.ends_with('/');
        (unrestorable && inner.starts_with(&below)).then(|| inner.to_string())
    });
    Ok(found)
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
    // The reasons that explain failures, with how many failures each explains.
    let mut reasons: Vec<(String, usize)> = Vec::new();
    // A tracked file that is now a folder holding what Undo can't bring back is refused before
    // anything runs, and its untracked files are left alone with it, so nothing is half done.
    let workdir = repo.workdir().ok_or(StageError::NoWorkdir)?;
    let mut refused: Vec<String> = Vec::new();
    for path in &tracked {
        if let Some(inner) = unrestorable_inside(repo, workdir, path)? {
            failed.push(path.to_string());
            reasons.push((
                format!(
                    " '{path}' is now a folder holding '{inner}', which Undo can't bring back."
                ),
                1,
            ));
            refused.push(format!("{path}/"));
        }
    }
    for path in untracked.into_iter().chain(tracked) {
        if failed.contains(path) {
            continue;
        }
        if let Some(position) = refused
            .iter()
            .position(|folder| path.starts_with(folder.as_str()))
        {
            failed.push(path.clone());
            reasons[position].1 += 1;
            continue;
        }
        match discard_file(repo, path) {
            Ok(()) => {}
            Err(StageError::IsFolder(_)) => {
                any_folder = true;
                failed.push(path.clone());
            }
            Err(StageError::FolderIsNowAFile { parent, .. }) => {
                reasons.push((format!(" '{parent}' is now a file, not a folder."), 1));
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
    // The reason is named when one folder/file swap explains every failure (#154).
    if let [(reason, explained)] = reasons.as_slice() {
        if *explained == count {
            message.push_str(reason);
        }
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

/// Puts `path`'s index entry back to HEAD's. Inside a folder/link pair (a folder replaced by a
/// link or file, or the reverse) the pair is the unit: git's index can't hold half of it, so
/// unstaging any member, the root included, restores HEAD's entries for the whole pair (#157).
/// A staged rename unstages with its source (#137), except a source inside another staged pair:
/// its deletion belongs to that pair, which stays staged (as `git restore --staged` leaves it).
pub fn unstage_file(repo: &git2::Repository, path: &str) -> Result<(), StageError> {
    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let target = head_commit.as_ref().map(|c| c.as_object());
    let head_tree = head_commit.as_ref().map(|c| c.tree()).transpose()?;
    let mut index = repo.index()?;
    let root = pair_root(&index, head_tree.as_ref(), path)?;
    let unit = root.clone().unwrap_or_else(|| path.to_string());
    let below = format!("{unit}/");
    let in_unit = |new: &str| new == unit || (root.is_some() && new.starts_with(&below));
    let mut paths = vec![unit.clone()];
    for source in staged_rename_sources(repo, in_unit)? {
        let inside_unit = source == unit || source.starts_with(&below);
        if !inside_unit && pair_root(&index, head_tree.as_ref(), &source)?.is_none() {
            paths.push(source);
        }
    }
    if root.is_some() {
        // libgit2 can't reset a root whose index entries sit on the other side of the
        // file/folder swap (re-adding `l` silently drops `l/a`, then "index does not contain
        // l/a"), so the pair's entries go first and the reset only adds HEAD's back.
        let _ = index.remove_path(Path::new(&unit));
        index.remove_dir(Path::new(&unit), 0)?;
        index.write()?;
    }
    repo.reset_default(target, paths.iter())?;
    Ok(())
}

/// The root of the folder/link pair `path` belongs to, if any: the outermost of `path`'s proper
/// ancestors that is a non-folder in the index (a staged link or file that replaced the folder)
/// or in HEAD (a folder that replaced a committed link or file, with `a/…` staged), or else
/// `path` itself when it is a non-folder on one side and a folder on the other (HEAD's file or
/// link with entries below it in the index, or the index's link or file over HEAD's folder).
/// Resetting the root restores HEAD's entries at and below it.
fn pair_root(
    index: &git2::Index,
    head_tree: Option<&git2::Tree>,
    path: &str,
) -> Result<Option<String>, StageError> {
    let head_kind = |p: &str| {
        head_tree.and_then(|tree| tree.get_path(Path::new(p)).ok().map(|entry| entry.kind()))
    };
    let mut ancestor = String::new();
    let components: Vec<&str> = path.split('/').collect();
    for component in &components[..components.len().saturating_sub(1)] {
        if !ancestor.is_empty() {
            ancestor.push('/');
        }
        ancestor.push_str(component);
        let in_index = index.get_path(Path::new(&ancestor), 0).is_some();
        let in_head = head_kind(&ancestor).is_some_and(|kind| kind != Some(git2::ObjectType::Tree));
        if in_index || in_head {
            return Ok(Some(ancestor));
        }
    }
    let index_file = index.get_path(Path::new(path), 0).is_some();
    let index_folder = index_has_entries_below(index, path)?;
    let head = head_kind(path);
    let head_file = head.is_some_and(|kind| kind != Some(git2::ObjectType::Tree));
    let head_folder = head == Some(Some(git2::ObjectType::Tree));
    if (head_file && index_folder) || (index_file && head_folder) {
        return Ok(Some(path.to_string()));
    }
    Ok(None)
}

/// True when the index has a (stage-0 or conflict) entry below the folder `path`.
fn index_has_entries_below(index: &git2::Index, path: &str) -> Result<bool, StageError> {
    let prefix = format!("{path}/");
    match index.find_prefix(Path::new(&prefix)) {
        Ok(position) => Ok(index
            .get(position)
            .is_some_and(|entry| entry.path.starts_with(prefix.as_bytes()))),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// The old paths of the staged renames whose new path `matches`.
fn staged_rename_sources(
    repo: &git2::Repository,
    matches: impl Fn(&str) -> bool,
) -> Result<Vec<String>, StageError> {
    let entries = crate::status::status(repo).map_err(|e| StageError::Status(e.to_string()))?;
    Ok(entries
        .into_iter()
        .filter(|entry| {
            entry.staged && entry.kind == crate::status::StatusKind::Renamed && matches(&entry.path)
        })
        .filter_map(|entry| entry.old_path)
        .collect())
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
