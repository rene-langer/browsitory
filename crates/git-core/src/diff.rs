use std::cell::RefCell;

use git2::Repository;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiffError {
    #[error("failed to compute diff: {0}")]
    Diff(#[from] git2::Error),
    #[error("There is no commit checked out to compare with.")]
    UnbornHead,
    /// The short ref name (without `refs/heads/` / `refs/remotes/`).
    #[error("{0} and HEAD have no common ancestor.")]
    NoMergeBase(String),
    #[error("No branch named \"{0}\".")]
    UnknownRef(String),
    #[error("path must be relative to the repository: {0}")]
    InvalidPath(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
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

/// Resolved ends of a branch compare: the merge base with HEAD, and the branch tip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompareRange {
    pub from_id: String,
    pub to_id: String,
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

/// The working tree compares like the Uncommitted view: staged, unstaged and untracked files.
fn include_untracked_content(git_opts: &mut git2::DiffOptions) {
    git_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
}

/// Paths that differ between `commit_id` and the working tree (`git diff <commit>` plus
/// untracked files), with renames detected among tracked files.
pub fn compare_working_tree_files(
    repo: &Repository,
    commit_id: &str,
) -> Result<Vec<ChangedFile>, DiffError> {
    let tree = commit_tree(repo, commit_id)?;
    let mut git_opts = git2::DiffOptions::new();
    include_untracked_content(&mut git_opts);
    let mut diff = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut git_opts))?;
    detect_renames(&mut diff)?;
    Ok(changed_files(&diff))
}

/// One file's diff from `commit_id` to the working tree; `old_path` as in `range_diff`.
pub fn compare_working_tree(
    repo: &Repository,
    commit_id: &str,
    path: &str,
    old_path: Option<&str>,
    opts: &DiffOptions,
) -> Result<Vec<DiffHunk>, DiffError> {
    let tree = commit_tree(repo, commit_id)?;
    let mut git_opts = literal_path_options(path, old_path, opts);
    include_untracked_content(&mut git_opts);
    let mut diff = repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut git_opts))?;
    if old_path.is_some() {
        detect_renames(&mut diff)?;
    }
    hunks_from_diff(&diff)
}

/// `merge-base(HEAD, ref)..ref`: what `ref_name` (a full `refs/heads/…` or `refs/remotes/…`
/// name) changed since it diverged from HEAD. A detached HEAD is used as is.
pub fn branch_compare_range(repo: &Repository, ref_name: &str) -> Result<CompareRange, DiffError> {
    let short = ref_name
        .strip_prefix("refs/heads/")
        .or_else(|| ref_name.strip_prefix("refs/remotes/"))
        .unwrap_or(ref_name)
        .to_string();
    let head = match repo.head() {
        Ok(head) => head.peel_to_commit()?.id(),
        Err(e)
            if matches!(
                e.code(),
                git2::ErrorCode::UnbornBranch | git2::ErrorCode::NotFound
            ) =>
        {
            return Err(DiffError::UnbornHead)
        }
        Err(e) => return Err(e.into()),
    };
    let tip = match repo.find_reference(ref_name) {
        Ok(reference) => reference.peel_to_commit()?.id(),
        Err(e) if e.code() == git2::ErrorCode::NotFound => {
            return Err(DiffError::UnknownRef(short))
        }
        Err(e) => return Err(e.into()),
    };
    let base = match repo.merge_base(head, tip) {
        Ok(base) => base,
        Err(e) if e.code() == git2::ErrorCode::NotFound => {
            return Err(DiffError::NoMergeBase(short))
        }
        Err(e) => return Err(e.into()),
    };
    Ok(CompareRange {
        from_id: base.to_string(),
        to_id: tip.to_string(),
    })
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

// ---------------------------------------------------------------------------------------------
// File sides (image preview)
// ---------------------------------------------------------------------------------------------

/// Largest side `file_sides` loads for a preview; larger sides are `FileSide::TooLarge`.
pub const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;

/// One side of a file's diff, as raw bytes for a preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSide {
    /// The path does not exist on this side (added/deleted), or it is not a regular file (a
    /// symlink, a submodule, a folder, or a work tree file reached through a symlinked folder).
    Missing,
    /// Over the cap; nothing was loaded.
    TooLarge {
        size: u64,
    },
    Loaded {
        bytes: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSides {
    pub old: FileSide,
    pub new: FileSide,
}

const GITLINK_MODE: u32 = 0o160000;
const LINK_MODE: u32 = 0o120000;

fn check_relative(path: &str) -> Result<&std::path::Path, DiffError> {
    if crate::launch::is_safe_relative(path) {
        Ok(std::path::Path::new(path))
    } else {
        Err(DiffError::InvalidPath(path.to_string()))
    }
}

/// A blob's bytes, or `TooLarge` judged from the object header before the blob is read.
fn blob_contents(repo: &Repository, oid: git2::Oid, cap: u64) -> Result<FileSide, DiffError> {
    let (size, _) = repo.odb()?.read_header(oid)?;
    let size = size as u64;
    if size > cap {
        return Ok(FileSide::TooLarge { size });
    }
    Ok(FileSide::Loaded {
        bytes: repo.find_blob(oid)?.content().to_vec(),
    })
}

fn tree_side(
    repo: &Repository,
    tree: Option<&git2::Tree>,
    rel: &std::path::Path,
    cap: u64,
) -> Result<FileSide, DiffError> {
    let Some(tree) = tree else {
        return Ok(FileSide::Missing);
    };
    match tree.get_path(rel) {
        Ok(entry)
            if entry.kind() == Some(git2::ObjectType::Blob)
                && entry.filemode() as u32 != LINK_MODE =>
        {
            blob_contents(repo, entry.id(), cap)
        }
        Ok(_) => Ok(FileSide::Missing),
        Err(e) if e.code() == git2::ErrorCode::NotFound => Ok(FileSide::Missing),
        Err(e) => Err(e.into()),
    }
}

fn index_side(repo: &Repository, rel: &std::path::Path, cap: u64) -> Result<FileSide, DiffError> {
    // The worker's `Repository` is long-lived: pick up a `git add` made elsewhere.
    let mut index = repo.index()?;
    index.read(false)?;
    match index.get_path(rel, 0) {
        Some(entry) if entry.mode != GITLINK_MODE && entry.mode != LINK_MODE => {
            blob_contents(repo, entry.id, cap)
        }
        _ => Ok(FileSide::Missing),
    }
}

/// The work tree file, never through a symlink: a symlinked file, or one reached through a
/// folder symlink out of the work tree, is `Missing`. The read is bounded, so a file that grew
/// after the size check is still `TooLarge`.
fn workdir_side(repo: &Repository, rel: &std::path::Path, cap: u64) -> Result<FileSide, DiffError> {
    use std::io::Read;
    let Some(workdir) = repo.workdir() else {
        return Ok(FileSide::Missing);
    };
    // One open that follows no symlink at any depth below the work tree and does not block on a
    // FIFO; size and content both come from that handle, so nothing swapped in meanwhile (a
    // folder replaced by a link, #138) can be read.
    let Some(handle) = crate::worktree_guard::open_beneath(workdir, rel)? else {
        return Ok(FileSide::Missing);
    };
    let len = handle.metadata()?.len();
    if len > cap {
        return Ok(FileSide::TooLarge { size: len });
    }
    let mut bytes = Vec::new();
    (&handle)
        .take(cap.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        let size = handle
            .metadata()
            .map(|m| m.len())
            .unwrap_or(bytes.len() as u64)
            .max(bytes.len() as u64);
        return Ok(FileSide::TooLarge { size });
    }
    Ok(FileSide::Loaded { bytes })
}

/// Both sides of `path`'s diff in `source`, as bytes (each capped at `cap`), for an image
/// preview. The sides are those the diff view shows: index ↔ work tree (`Unstaged`), HEAD ↔
/// index (`Staged`), first parent (or nothing, for a root commit) ↔ commit (`Commit`), and
/// `from` ↔ `to` or the work tree (`Range`). The old side reads `old_path` when the source has
/// one (a rename). Paths must be relative, without `.`/`..` components.
pub fn file_sides(
    repo: &Repository,
    source: &crate::tools::DiffSource,
    path: &str,
    cap: u64,
) -> Result<FileSides, DiffError> {
    use crate::tools::DiffSource;
    let rel = check_relative(path)?;
    let old_rel = match source {
        DiffSource::Commit {
            old_path: Some(old),
            ..
        }
        | DiffSource::Range {
            old_path: Some(old),
            ..
        } => check_relative(old)?,
        _ => rel,
    };
    let (old, new) = match source {
        DiffSource::Unstaged => (index_side(repo, rel, cap)?, workdir_side(repo, rel, cap)?),
        DiffSource::Staged => {
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            (
                tree_side(repo, head_tree.as_ref(), rel, cap)?,
                index_side(repo, rel, cap)?,
            )
        }
        DiffSource::Commit { commit_id, .. } => {
            let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
            (
                tree_side(repo, old_tree.as_ref(), old_rel, cap)?,
                tree_side(repo, Some(&new_tree), rel, cap)?,
            )
        }
        DiffSource::Range { from_id, to_id, .. } => {
            let old_tree = commit_tree(repo, from_id)?;
            let new = match to_id {
                Some(to_id) => tree_side(repo, Some(&commit_tree(repo, to_id)?), rel, cap)?,
                None => workdir_side(repo, rel, cap)?,
            };
            (tree_side(repo, Some(&old_tree), old_rel, cap)?, new)
        }
    };
    Ok(FileSides { old, new })
}

/// The image type `bytes` actually are, from their magic bytes (never the file name): PNG,
/// JPEG, GIF, WebP, or SVG (an `<svg` root element, after an optional BOM, XML declaration,
/// comments and doctype). `None` for anything else. The preview renders only these types, and
/// only through `<img>`.
pub fn sniff_image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some("image/jpeg");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if svg_root(bytes) {
        return Some("image/svg+xml");
    }
    None
}

/// How far into a file the SVG sniffer looks for the root element.
const SVG_SNIFF_BYTES: usize = 64 * 1024;

fn svg_root(bytes: &[u8]) -> bool {
    let mut rest = &bytes[..bytes.len().min(SVG_SNIFF_BYTES)];
    rest = rest.strip_prefix(b"\xef\xbb\xbf").unwrap_or(rest);
    loop {
        let start = rest
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(rest.len());
        rest = &rest[start..];
        let skip_to = |rest: &[u8], end: &[u8]| -> Option<usize> {
            rest.windows(end.len())
                .position(|w| w == end)
                .map(|i| i + end.len())
        };
        let skipped = if rest.starts_with(b"<?") {
            skip_to(rest, b"?>")
        } else if rest.starts_with(b"<!--") {
            skip_to(rest, b"-->")
        } else if rest.len() >= 9 && rest[..9].eq_ignore_ascii_case(b"<!doctype") {
            skip_to(rest, b">")
        } else {
            return rest.starts_with(b"<svg")
                && rest
                    .get(4)
                    .is_some_and(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/');
        };
        match skipped {
            Some(end) => rest = &rest[end..],
            None => return false,
        }
    }
}
