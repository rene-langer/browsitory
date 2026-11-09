//! Patches (#448): a commit's file or hunk reverted or applied in the working tree, patch text
//! for a commit, several commits, a commit's file or the staged changes, and a patch file
//! applied to the working tree.
//!
//! Nothing here commits. Every apply runs libgit2's check first and changes nothing when the
//! patch doesn't apply, like `git apply`. libgit2 applies a hunk only at the exact line its header
//! names, so before the check each text hunk is moved to where its old lines are now (the nearest
//! match to that line, as `git apply` searches), which lets a commit's hunk land in a file that
//! has changed above it since. No fuzz: the old lines must match exactly.
//!
//! The guards are those of discarding a hunk: no path outside the repository or in `.git`,
//! nothing written through a symbolic link, no folder recreated over a file, and on a file system
//! that ignores case no file created next to one whose name differs only in case.

use std::path::{Component, Path};

use thiserror::Error;

use crate::diff::DiffOptions;
use crate::line_patch::{self, Direction, FileRef, HunkRef, PatchLine};
use crate::worktree_guard;

#[derive(Debug, Error)]
pub enum PatchError {
    #[error("{0}")]
    Git(#[from] git2::Error),
    #[error("repository has no working directory (bare repository)")]
    NoWorkdir,
    #[error("hunk not found (the commit's diff has changed since it was shown)")]
    HunkNotFound,
    #[error("The commit doesn't change {0}.")]
    NothingToApply(String),
    #[error("There are no staged changes.")]
    NothingStaged,
    #[error("Pick at least one commit.")]
    NoCommits,
    /// libgit2's check refused the patch; nothing was written.
    #[error("The patch doesn't apply to the working tree, which was left unchanged: {0}")]
    DoesNotApply(String),
    #[error("The file holds no patch.")]
    EmptyPatch,
    #[error("That file isn't a patch Browsitory can read: {0}")]
    Unreadable(String),
    #[error("The patch names '{0}', which is outside the repository.")]
    InvalidPath(String),
    #[error("'{0}' is beyond a symbolic link.")]
    BeyondSymlink(String),
    #[error("'{0}' is a symbolic link in the working tree; the patch changes a file there.")]
    LinkInPlace(String),
    /// A folder on the way to `path` is a file in the working tree (#154).
    #[error("Couldn't apply to '{path}': '{parent}' is now a file, not a folder.")]
    FolderIsNowAFile { path: String, parent: String },
    /// The patch creates `path` next to `existing`, which differs only in case (#403).
    #[error("The patch creates '{path}', but '{existing}' differs from it only in case and this file system can't hold both.")]
    CasePair { path: String, existing: String },
    #[error("{0}")]
    CaseConfig(#[from] crate::CaseConfigUnreadable),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl From<crate::diff::DiffError> for PatchError {
    fn from(error: crate::diff::DiffError) -> Self {
        match error {
            crate::diff::DiffError::Diff(e) => PatchError::Git(e),
            crate::diff::DiffError::Io(e) => PatchError::Io(e),
            other => PatchError::Git(git2::Error::from_str(&other.to_string())),
        }
    }
}

/// A hunk of a commit's diff, as the commit view showed it: `(old_start, new_start)` read with
/// `context_lines` (whitespace significant, as for staging).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitHunk {
    pub old_start: u32,
    pub new_start: u32,
    pub context_lines: u32,
}

/// Where an apply lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyTarget {
    /// The working tree only: the result shows as unstaged changes.
    WorkTree,
    /// The working tree and the index (`git apply --index`): the touched files must have no
    /// unstaged changes.
    WorkTreeAndIndex,
}

impl ApplyTarget {
    fn location(self) -> git2::ApplyLocation {
        match self {
            ApplyTarget::WorkTree => git2::ApplyLocation::WorkDir,
            ApplyTarget::WorkTreeAndIndex => git2::ApplyLocation::Both,
        }
    }
}

/// The context a hunk keeps when it is applied: what the commit view shows may be the whole
/// file, which would only apply to an unchanged file.
const APPLY_CONTEXT_LINES: usize = 3;

/// Applies (`reverse: false`) or reverts (`reverse: true`) `path`'s change in `commit_id` (against
/// its first parent), or only `hunk` of it, in the working tree. `old_path` is set for a rename
/// the commit view paired (the hunk is then one of the rename's diff).
pub fn apply_commit_change(
    repo: &git2::Repository,
    commit_id: &str,
    path: &str,
    old_path: Option<&str>,
    hunk: Option<CommitHunk>,
    reverse: bool,
    target: ApplyTarget,
) -> Result<(), PatchError> {
    let patch = match hunk {
        None => commit_file_diff_bytes(repo, commit_id, path, old_path, reverse)?,
        Some(hunk) => commit_hunk_patch(repo, commit_id, path, old_path, hunk, reverse)?,
    };
    if patch.is_empty() {
        return Err(PatchError::NothingToApply(path.to_string()));
    }
    apply_one(repo, &patch, target)
}

/// `path`'s change in `commit_id` (and `old_path`'s, for a paired rename), as patch text: a
/// plain `git diff` of that file, binary content included.
pub fn commit_file_patch(
    repo: &git2::Repository,
    commit_id: &str,
    path: &str,
    old_path: Option<&str>,
) -> Result<Vec<u8>, PatchError> {
    let patch = commit_file_diff_bytes(repo, commit_id, path, old_path, false)?;
    if patch.is_empty() {
        return Err(PatchError::NothingToApply(path.to_string()));
    }
    Ok(patch)
}

/// `commit_ids` (oldest first) as one mailbox of `git format-patch` mails, `[PATCH n/m]` when
/// there are several. Each commit is diffed against its first parent, renames detected and
/// binary content included.
pub fn commits_patch(
    repo: &git2::Repository,
    commit_ids: &[String],
) -> Result<Vec<u8>, PatchError> {
    if commit_ids.is_empty() {
        return Err(PatchError::NoCommits);
    }
    let mut out = Vec::new();
    let count = commit_ids.len();
    for (index, id) in commit_ids.iter().enumerate() {
        let commit = repo.find_commit(git2::Oid::from_str(id)?)?;
        let parent_tree = commit.parent(0).ok().map(|p| p.tree()).transpose()?;
        let tree = commit.tree()?;
        let mut opts = git2::DiffOptions::new();
        opts.show_binary(true);
        let mut diff =
            repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;
        crate::diff::detect_renames(&mut diff)?;
        let message = String::from_utf8_lossy(commit.message_bytes()).into_owned();
        let (summary, body) = split_message(&message);
        let author = commit.author();
        let mut email_opts = git2::EmailCreateOptions::new();
        let email = git2::Email::from_diff(
            &diff,
            index + 1,
            count,
            &commit.id(),
            summary,
            body,
            &author,
            &mut email_opts,
        )?;
        out.extend_from_slice(email.as_slice());
    }
    Ok(out)
}

/// The staged changes (HEAD → index) as patch text, renames detected and binary content
/// included.
pub fn staged_patch(repo: &git2::Repository) -> Result<Vec<u8>, PatchError> {
    let head_tree = match repo.head() {
        Ok(head) => Some(head.peel_to_tree()?),
        Err(e)
            if e.code() == git2::ErrorCode::UnbornBranch
                || e.code() == git2::ErrorCode::NotFound =>
        {
            None
        }
        Err(e) => return Err(e.into()),
    };
    let mut opts = git2::DiffOptions::new();
    opts.show_binary(true);
    let mut diff = repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?;
    crate::diff::detect_renames(&mut diff)?;
    let mut out = Vec::new();
    for index in 0..diff.deltas().len() {
        if let Some(mut patch) = git2::Patch::from_diff(&diff, index)? {
            out.extend_from_slice(&patch.to_buf()?);
        }
    }
    if out.is_empty() {
        return Err(PatchError::NothingStaged);
    }
    Ok(out)
}

/// Applies the patch file `content` to the working tree: a plain diff, or one or more
/// `git format-patch` mails, applied in order. When any of them doesn't apply, the files the
/// earlier ones changed are put back, so the working tree is left as it was.
pub fn apply_patch(repo: &git2::Repository, content: &[u8]) -> Result<(), PatchError> {
    let parts = split_mails(content);
    if parts.len() <= 1 {
        return apply_one(repo, content, ApplyTarget::WorkTree);
    }
    // Parse and guard every mail before anything is written.
    let mut diffs = Vec::with_capacity(parts.len());
    for part in &parts {
        let diff = parse(part)?;
        guard(repo, &diff)?;
        diffs.push(diff);
    }
    let workdir = repo.workdir().ok_or(PatchError::NoWorkdir)?;
    let mut touched: Vec<String> = Vec::new();
    for diff in &diffs {
        for delta in diff.deltas() {
            for file in [delta.old_file(), delta.new_file()] {
                if let Some(path) = file.path().and_then(Path::to_str) {
                    if !touched.iter().any(|t| t == path) {
                        touched.push(path.to_string());
                    }
                }
            }
        }
    }
    let saved = Saved::take(workdir, &touched)?;
    for part in &parts {
        if let Err(error) = apply_one(repo, part, ApplyTarget::WorkTree) {
            saved.restore(workdir);
            return Err(error);
        }
    }
    Ok(())
}

/// The forward (or, `reverse`, the backward) diff of `path` in `commit_id`, only `path`'s own
/// deltas (and `old_path`'s), as patch text.
fn commit_file_diff_bytes(
    repo: &git2::Repository,
    commit_id: &str,
    path: &str,
    old_path: Option<&str>,
    reverse: bool,
) -> Result<Vec<u8>, PatchError> {
    let (old_tree, new_tree) = crate::diff::commit_and_parent_trees(repo, commit_id)?;
    let mut opts = crate::diff::literal_path_options(path, old_path, &DiffOptions::default());
    opts.show_binary(true);
    let (from, to) = if reverse {
        (Some(&new_tree), old_tree.as_ref())
    } else {
        (old_tree.as_ref(), Some(&new_tree))
    };
    let mut diff = repo.diff_tree_to_tree(from, to, Some(&mut opts))?;
    if old_path.is_some() {
        crate::diff::detect_renames(&mut diff)?;
    }
    let wanted: Vec<&str> = std::iter::once(path).chain(old_path).collect();
    let mut out = Vec::new();
    for (index, delta) in diff.deltas().enumerate() {
        if !crate::diff::delta_is_one_of(&delta, &wanted) {
            continue;
        }
        if let Some(mut patch) = git2::Patch::from_diff(&diff, index)? {
            out.extend_from_slice(&patch.to_buf()?);
        }
    }
    Ok(out)
}

/// A one-hunk patch of the commit view's hunk at `(old_start, new_start)`, forward or reversed,
/// trimmed to `APPLY_CONTEXT_LINES` of context on each side.
fn commit_hunk_patch(
    repo: &git2::Repository,
    commit_id: &str,
    path: &str,
    old_path: Option<&str>,
    hunk: CommitHunk,
    reverse: bool,
) -> Result<Vec<u8>, PatchError> {
    let (old_tree, new_tree) = crate::diff::commit_and_parent_trees(repo, commit_id)?;
    // The same options as the commit view's diff (`diff::commit_diff`/`commit_rename_diff`),
    // whitespace significant, so the hunk identity matches.
    let display = DiffOptions {
        ignore_whitespace: false,
        context_lines: hunk.context_lines,
    };
    let mut opts = crate::diff::literal_path_options(path, old_path, &display);
    let mut diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;
    if old_path.is_some() {
        crate::diff::detect_renames(&mut diff)?;
    }
    let found = crate::stage::find_hunk(&diff, path, hunk.old_start, hunk.new_start)
        .map_err(|_| PatchError::HunkNotFound)?;
    let (lines, old_start, new_start) = trim_context(&found.lines, hunk.old_start, hunk.new_start);
    let selected: Vec<u32> = (0..lines.len() as u32)
        .filter(|&i| matches!(lines[i as usize].origin, '+' | '-'))
        .collect();
    let file = FileRef {
        path,
        old_absent: found.old_absent,
        new_absent: found.new_absent,
        old_mode: found.old_mode,
        new_mode: found.new_mode,
        // Reverting an added file's only hunk removes the file, as reverting the commit would.
        allow_delete: true,
    };
    let direction = if reverse {
        Direction::Reverse
    } else {
        Direction::Forward
    };
    line_patch::build(
        &file,
        &HunkRef {
            old_start,
            new_start,
        },
        &lines,
        &selected,
        direction,
    )
    .map_err(|_| PatchError::DoesNotApply("the hunk can't be written as a patch".to_string()))
}

/// `lines` without the context beyond `APPLY_CONTEXT_LINES` before the first and after the last
/// change, and the hunk's starts moved past the lines dropped in front.
fn trim_context(lines: &[PatchLine], old_start: u32, new_start: u32) -> (Vec<PatchLine>, u32, u32) {
    let is_change = |l: &PatchLine| matches!(l.origin, '+' | '-');
    let Some(first) = lines.iter().position(is_change) else {
        return (lines.to_vec(), old_start, new_start);
    };
    let last = lines.iter().rposition(is_change).unwrap_or(first);
    let start = first.saturating_sub(APPLY_CONTEXT_LINES);
    let mut end = last + 1;
    let mut context = 0;
    while end < lines.len() {
        match lines[end].origin {
            // An end-of-file marker belongs to the line before it.
            '=' | '>' | '<' => end += 1,
            ' ' if context < APPLY_CONTEXT_LINES => {
                context += 1;
                end += 1;
            }
            _ => break,
        }
    }
    let dropped = start as u32;
    (
        lines[start..end].to_vec(),
        old_start + dropped,
        new_start + dropped,
    )
}

/// `message`'s first paragraph (joined into one line, as `git format-patch` does) and the rest.
fn split_message(message: &str) -> (String, String) {
    let trimmed = message.trim_start_matches('\n');
    let (head, rest) = match trimmed.find("\n\n") {
        Some(at) => (&trimmed[..at], &trimmed[at + 2..]),
        None => (trimmed, ""),
    };
    let summary = head.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    (summary, rest.trim_matches('\n').to_string())
}

/// The mails of a `git format-patch` mailbox (each starting at its `From <id> Mon Sep 17
/// 00:00:00 2001` line), or the whole content as one part.
fn split_mails(content: &[u8]) -> Vec<&[u8]> {
    let is_mail_start = |line: &[u8]| {
        line.starts_with(b"From ")
            && line.len() >= 5 + 40
            && line[5..45].iter().all(u8::is_ascii_hexdigit)
            && line[45..].starts_with(b" Mon Sep 17 00:00:00 2001")
    };
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in content.split_inclusive(|&b| b == b'\n') {
        if is_mail_start(line) {
            starts.push(offset);
        }
        offset += line.len();
    }
    if starts.len() <= 1 {
        return vec![content];
    }
    let mut parts = Vec::with_capacity(starts.len());
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).copied().unwrap_or(content.len());
        parts.push(&content[start..end]);
    }
    parts
}

fn parse(content: &[u8]) -> Result<git2::Diff<'static>, PatchError> {
    let diff = git2::Diff::from_buffer(content)
        .map_err(|e| PatchError::Unreadable(e.message().to_string()))?;
    if diff.deltas().len() == 0 {
        return Err(PatchError::EmptyPatch);
    }
    Ok(diff)
}

/// Guards, moves the hunks to where they apply now, checks, then applies one patch.
fn apply_one(
    repo: &git2::Repository,
    content: &[u8],
    target: ApplyTarget,
) -> Result<(), PatchError> {
    let diff = parse(content)?;
    guard(repo, &diff)?;
    let relocated = rewrite_headers(content, &relocate(repo, &diff)?);
    let diff = git2::Diff::from_buffer(&relocated)
        .map_err(|e| PatchError::Unreadable(e.message().to_string()))?;
    // libgit2's check reads only the files a patch changes: one it creates over an untracked
    // file would pass it and then stop the checkout (#448: nothing is written either way, but
    // the reason is clearer here).
    let workdir = repo.workdir().ok_or(PatchError::NoWorkdir)?;
    for delta in diff.deltas() {
        let creates = match delta.status() {
            git2::Delta::Added | git2::Delta::Copied => true,
            git2::Delta::Renamed => delta.old_file().path() != delta.new_file().path(),
            _ => false,
        };
        if let (true, Some(path)) = (creates, delta.new_file().path()) {
            if workdir.join(path).symlink_metadata().is_ok() {
                return Err(PatchError::DoesNotApply(format!(
                    "{} already exists in the working tree",
                    path.display()
                )));
            }
        }
    }
    let location = target.location();
    let mut check = git2::ApplyOptions::new();
    check.check(true);
    repo.apply(&diff, location, Some(&mut check))
        .map_err(does_not_apply)?;
    crate::hardlink::before_apply(repo, &diff, location);
    repo.apply(&diff, location, None).map_err(does_not_apply)
}

/// A refusal of the patch itself is "doesn't apply"; anything else (I/O, a locked index) is
/// reported as it is.
fn does_not_apply(e: git2::Error) -> PatchError {
    if e.code() == git2::ErrorCode::ApplyFail
        || e.class() == git2::ErrorClass::Patch
        || e.code() == git2::ErrorCode::Exists
        || e.code() == git2::ErrorCode::NotFound
        || (e.class() == git2::ErrorClass::Checkout && e.code() == git2::ErrorCode::Conflict)
    {
        PatchError::DoesNotApply(e.message().to_string())
    } else {
        PatchError::Git(e)
    }
}

/// Refuses a patch that names a path outside the repository or in `.git`, would write through
/// a symbolic link or over a file standing where a folder should be, or creates a name that
/// differs only in case from a tracked one where case folds.
fn guard(repo: &git2::Repository, diff: &git2::Diff) -> Result<(), PatchError> {
    let workdir = repo.workdir().ok_or(PatchError::NoWorkdir)?;
    let folds = worktree_guard::try_folds_case(repo)?;
    let index = if folds { Some(repo.index()?) } else { None };
    for delta in diff.deltas() {
        for file in [delta.old_file(), delta.new_file()] {
            let Some(bytes) = file.path_bytes() else {
                continue;
            };
            let path = std::str::from_utf8(bytes).map_err(|_| {
                PatchError::InvalidPath(String::from_utf8_lossy(bytes).into_owned())
            })?;
            let rel = Path::new(path);
            if path.is_empty()
                || rel.is_absolute()
                || rel.components().any(|c| match c {
                    Component::Normal(name) => name
                        .to_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case(".git")),
                    _ => true,
                })
            {
                return Err(PatchError::InvalidPath(path.to_string()));
            }
            if worktree_guard::has_symlinked_ancestor(workdir, path) {
                return Err(PatchError::BeyondSymlink(path.to_string()));
            }
            crate::stage::ensure_no_symlinked_parent(workdir, rel)
                .map_err(|_| PatchError::BeyondSymlink(path.to_string()))?;
            if let Some(parent) = worktree_guard::file_in_place_of_folder(workdir, path)? {
                return Err(PatchError::FolderIsNowAFile {
                    path: path.to_string(),
                    parent,
                });
            }
            // A link the patch treats as a file would be read through.
            let mode = u32::from(file.mode());
            if mode != 0 && mode != 0o120000 {
                if let Ok(meta) = workdir.join(rel).symlink_metadata() {
                    if meta.file_type().is_symlink() {
                        return Err(PatchError::LinkInPlace(path.to_string()));
                    }
                }
            }
        }
        if let (Some(index), git2::Delta::Added | git2::Delta::Renamed | git2::Delta::Copied) =
            (&index, delta.status())
        {
            if let Some(new) = delta.new_file().path_bytes() {
                let folded = worktree_guard::fold_name(new);
                if let Some(existing) = index.iter().find(|entry| {
                    entry.path != new && worktree_guard::fold_name(&entry.path) == folded
                }) {
                    return Err(PatchError::CasePair {
                        path: String::from_utf8_lossy(new).into_owned(),
                        existing: String::from_utf8_lossy(&existing.path).into_owned(),
                    });
                }
            }
        }
    }
    Ok(())
}

/// One hunk's lines as libgit2 matches them: the content of each line (a line followed by an
/// end-of-file marker loses its newline).
struct HunkLines {
    old: Vec<Vec<u8>>,
    new: Vec<Vec<u8>>,
}

fn hunk_lines(patch: &git2::Patch, hunk: usize) -> Result<HunkLines, git2::Error> {
    let count = patch.num_lines_in_hunk(hunk)?;
    let mut old: Vec<Vec<u8>> = Vec::new();
    let mut new: Vec<Vec<u8>> = Vec::new();
    // Which side(s) the previous line went to, for an end-of-file marker.
    let mut last: (bool, bool) = (false, false);
    for i in 0..count {
        let line = patch.line_in_hunk(hunk, i)?;
        let content = line.content().to_vec();
        match line.origin() {
            ' ' => {
                old.push(content.clone());
                new.push(content);
                last = (true, true);
            }
            '-' => {
                old.push(content);
                last = (true, false);
            }
            '+' => {
                new.push(content);
                last = (false, true);
            }
            '=' | '<' | '>' => {
                if last.0 {
                    if let Some(l) = old.last_mut() {
                        if l.ends_with(b"\n") {
                            l.pop();
                        }
                    }
                }
                if last.1 {
                    if let Some(l) = new.last_mut() {
                        if l.ends_with(b"\n") {
                            l.pop();
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(HunkLines { old, new })
}

/// Lines compared as libgit2 compares the filtered working tree file: a CRLF line ending counts
/// as LF.
fn same_line(a: &[u8], b: &[u8]) -> bool {
    fn strip(l: &[u8]) -> &[u8] {
        if l.ends_with(b"\r\n") {
            &l[..l.len() - 2]
        } else if l.ends_with(b"\n") {
            &l[..l.len() - 1]
        } else {
            l
        }
    }
    let ends = |l: &[u8]| l.ends_with(b"\n");
    ends(a) == ends(b) && strip(a) == strip(b)
}

/// The position nearest to `expected` where `needle` is in `image`.
fn nearest_match(image: &[Vec<u8>], needle: &[Vec<u8>], expected: usize) -> Option<usize> {
    if needle.len() > image.len() {
        return None;
    }
    let last = image.len() - needle.len();
    let matches_at = |pos: usize| {
        image[pos..pos + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| same_line(a, b))
    };
    let expected = expected.min(last);
    for distance in 0..=last {
        let below = expected + distance;
        if below <= last && matches_at(below) {
            return Some(below);
        }
        if distance > 0 && distance <= expected && matches_at(expected - distance) {
            return Some(expected - distance);
        }
        if below > last && distance > expected {
            break;
        }
    }
    None
}

fn split_lines(content: &[u8]) -> Vec<Vec<u8>> {
    content
        .split_inclusive(|&b| b == b'\n')
        .map(<[u8]>::to_vec)
        .collect()
}

/// For each text hunk of `diff`, in order, where its old lines are in the working tree file now
/// (see the module docs): the header's new `(old_start, new_start)`, or `None` to keep it (a hunk
/// that matches nowhere; the check then refuses it). Binary and whole-file (added, deleted)
/// changes have no hunks to move.
fn relocate(
    repo: &git2::Repository,
    diff: &git2::Diff,
) -> Result<Vec<Option<(u32, u32)>>, PatchError> {
    let workdir = repo.workdir().ok_or(PatchError::NoWorkdir)?;
    let mut headers = Vec::new();
    for index in 0..diff.deltas().len() {
        let Some(patch) = git2::Patch::from_diff(diff, index)? else {
            continue;
        };
        let hunks = patch.num_hunks();
        let delta = patch.delta();
        let movable = !delta.flags().is_binary()
            && matches!(
                delta.status(),
                git2::Delta::Modified | git2::Delta::Renamed | git2::Delta::Copied
            );
        let current = if movable && hunks > 0 {
            delta
                .old_file()
                .path()
                .and_then(|rel| std::fs::read(workdir.join(rel)).ok())
        } else {
            None
        };
        let Some(current) = current else {
            headers.extend(std::iter::repeat_n(None, hunks));
            continue;
        };
        let mut image = split_lines(&current);
        // How far the earlier hunks were moved from where their headers said, and how many
        // lines they added.
        let mut drift: isize = 0;
        let mut growth: isize = 0;
        let mut lost = false;
        for h in 0..hunks {
            if lost {
                headers.push(None);
                continue;
            }
            let (hunk, _) = patch.hunk(h)?;
            let lines = hunk_lines(&patch, h)?;
            let said = hunk.new_start().saturating_sub(1) as isize;
            let expected = (said + drift).max(0) as usize;
            let found = if lines.old.is_empty() {
                Some(expected.min(image.len()))
            } else {
                nearest_match(&image, &lines.old, expected)
            };
            let Some(pos) = found else {
                headers.push(None);
                lost = true;
                continue;
            };
            drift = pos as isize - said;
            let old_start = (pos as isize - growth + 1).max(1) as u32;
            headers.push(Some((old_start, pos as u32 + 1)));
            growth += lines.new.len() as isize - lines.old.len() as isize;
            let end = pos + lines.old.len();
            image.splice(pos..end, lines.new);
        }
    }
    Ok(headers)
}

/// `@@ -a[,b] +c[,d] @@`'s old and new line counts.
fn header_counts(line: &[u8]) -> Option<(u32, u32)> {
    let text = std::str::from_utf8(line).ok()?;
    let (ranges, _) = text.strip_prefix("@@ -")?.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let count = |range: &str| match range.split_once(',') {
        Some((_, c)) => c.parse().ok(),
        None => Some(1),
    };
    Some((count(old)?, count(new)?))
}

/// The patch `text` with its `n`th hunk header's starts replaced by `headers[n]`. A hunk header
/// is an `@@ -` line inside a file's patch (after `diff --git` or `---`/`+++`, before anything
/// that isn't part of it), never one in a hunk's body or a mail's text, which is the order
/// libgit2's parser reads hunks in.
fn rewrite_headers(text: &[u8], headers: &[Option<(u32, u32)>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut n = 0;
    let mut in_file = false;
    let mut hunk_seen = false;
    // Old and new lines left in the current hunk's body.
    let mut body = (0u32, 0u32);
    for line in text.split_inclusive(|&b| b == b'\n') {
        if body != (0, 0) {
            match line.first() {
                Some(b' ') | Some(b'\n') | Some(b'\r') => {
                    body = (body.0.saturating_sub(1), body.1.saturating_sub(1));
                }
                Some(b'-') => body.0 = body.0.saturating_sub(1),
                Some(b'+') => body.1 = body.1.saturating_sub(1),
                Some(b'\\') => {}
                _ => {
                    body = (0, 0);
                    in_file = false;
                }
            }
            if in_file {
                out.extend_from_slice(line);
                continue;
            }
        }
        if line.starts_with(b"diff --git ") {
            in_file = true;
            hunk_seen = false;
        } else if line.starts_with(b"--- ") || line.starts_with(b"+++ ") {
            if !hunk_seen {
                in_file = true;
            }
        } else if in_file && line.starts_with(b"@@ -") {
            hunk_seen = true;
            body = header_counts(line).unwrap_or((0, 0));
            let rewritten = headers
                .get(n)
                .copied()
                .flatten()
                .and_then(|(old_start, new_start)| rewrite_header(line, old_start, new_start));
            n += 1;
            out.extend_from_slice(rewritten.as_deref().unwrap_or(line));
            continue;
        } else if hunk_seen && !line.starts_with(b"\\") {
            // Past the file's last hunk: a mail's trailer or the next mail's text.
            in_file = false;
            hunk_seen = false;
        }
        out.extend_from_slice(line);
    }
    out
}

/// `@@ -a,b +c,d @@rest` with `a` and `c` replaced.
fn rewrite_header(line: &[u8], old_start: u32, new_start: u32) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(line).ok()?;
    let rest = text.strip_prefix("@@ -")?;
    let (ranges, tail) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let count = |range: &str| range.split_once(',').map(|(_, c)| c.to_string());
    let old_count = count(old);
    let new_count = count(new);
    let side = |start: u32, count: Option<String>| match count {
        Some(c) => format!("{start},{c}"),
        None => start.to_string(),
    };
    Some(
        format!(
            "@@ -{} +{} @@{tail}",
            side(old_start, old_count),
            side(new_start, new_count)
        )
        .into_bytes(),
    )
}

/// The working tree files a patch series touches, as they were before it, to put back when a
/// later patch of the series doesn't apply.
struct Saved(Vec<(String, Option<SavedFile>)>);

enum SavedFile {
    File { bytes: Vec<u8>, executable: bool },
    Link(std::path::PathBuf),
}

impl Saved {
    fn take(workdir: &Path, paths: &[String]) -> Result<Self, PatchError> {
        let mut saved = Vec::with_capacity(paths.len());
        for path in paths {
            let full = workdir.join(path);
            let file = match full.symlink_metadata() {
                Ok(meta) if meta.file_type().is_symlink() => {
                    Some(SavedFile::Link(std::fs::read_link(&full)?))
                }
                Ok(meta) if meta.is_file() => Some(SavedFile::File {
                    bytes: std::fs::read(&full)?,
                    executable: is_executable(&meta),
                }),
                // A folder is never written by a patch to a file of that name: the check refuses.
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => None,
                Err(e) => return Err(e.into()),
            };
            saved.push((path.clone(), file));
        }
        Ok(Saved(saved))
    }

    /// Best effort: each file is removed and written anew (never through a link or a hard
    /// link's other names); a file that was missing is removed.
    fn restore(&self, workdir: &Path) {
        for (path, file) in &self.0 {
            if worktree_guard::has_symlinked_ancestor(workdir, path) {
                continue;
            }
            let full = workdir.join(path);
            if let Ok(meta) = full.symlink_metadata() {
                if meta.is_dir() {
                    continue;
                }
                let _ = std::fs::remove_file(&full);
            }
            match file {
                None => {}
                Some(SavedFile::File { bytes, executable }) => {
                    if let Some(parent) = full.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if std::fs::write(&full, bytes).is_ok() {
                        set_executable(&full, *executable);
                    }
                }
                Some(SavedFile::Link(target)) => {
                    if let Some(parent) = full.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    make_link(target, &full);
                }
            }
        }
    }
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) {
    use std::os::unix::fs::PermissionsExt;
    if executable {
        if let Ok(meta) = std::fs::metadata(path) {
            let mut permissions = meta.permissions();
            permissions.set_mode(permissions.mode() | 0o111);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) {}

#[cfg(unix)]
fn make_link(target: &Path, at: &Path) {
    let _ = std::os::unix::fs::symlink(target, at);
}

#[cfg(not(unix))]
fn make_link(target: &Path, at: &Path) {
    // Without symbolic links git writes the link's target as a plain file.
    let _ = std::fs::write(at, target.to_string_lossy().as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_message_joins_the_first_paragraph() {
        assert_eq!(
            split_message("Fix the\nthing\n\nBody line\n"),
            ("Fix the thing".to_string(), "Body line".to_string())
        );
        assert_eq!(split_message("Only"), ("Only".to_string(), String::new()));
    }

    #[test]
    fn rewrite_header_keeps_counts_and_function_context() {
        assert_eq!(
            rewrite_header(b"@@ -10,3 +10,4 @@ fn main\n", 2, 3).unwrap(),
            b"@@ -2,3 +3,4 @@ fn main\n".to_vec()
        );
        assert_eq!(
            rewrite_header(b"@@ -1 +1 @@\n", 5, 6).unwrap(),
            b"@@ -5 +6 @@\n".to_vec()
        );
    }

    #[test]
    fn nearest_match_prefers_the_closest_position() {
        let image: Vec<Vec<u8>> = ["a\n", "x\n", "x\n", "a\n", "x\n"]
            .iter()
            .map(|s| s.as_bytes().to_vec())
            .collect();
        let needle = vec![b"a\n".to_vec()];
        assert_eq!(nearest_match(&image, &needle, 2), Some(3));
        assert_eq!(nearest_match(&image, &needle, 1), Some(0));
        assert_eq!(nearest_match(&image, &needle, 4), Some(3));
        assert_eq!(nearest_match(&image, &[b"zz\n".to_vec()], 0), None);
    }

    #[test]
    fn split_mails_finds_each_format_patch_mail() {
        let id = "0123456789abcdef0123456789abcdef01234567";
        let text = format!(
            "From {id} Mon Sep 17 00:00:00 2001\nA\nFrom {id} Mon Sep 17 00:00:00 2001\nB\n"
        );
        let parts = split_mails(text.as_bytes());
        assert_eq!(parts.len(), 2);
        assert!(parts[1].ends_with(b"B\n"));
        assert_eq!(split_mails(b"diff --git a/x b/x\n").len(), 1);
    }

    #[test]
    fn trim_context_keeps_three_lines_around_the_changes() {
        let line = |origin: char, text: &str| PatchLine {
            origin,
            content: text.as_bytes().to_vec(),
        };
        let mut lines: Vec<PatchLine> = (0..6).map(|i| line(' ', &format!("{i}\n"))).collect();
        lines.push(line('-', "old\n"));
        lines.push(line('+', "new\n"));
        lines.extend((0..5).map(|i| line(' ', &format!("t{i}\n"))));
        let (kept, old_start, new_start) = trim_context(&lines, 1, 1);
        assert_eq!(kept.len(), 3 + 2 + 3);
        assert_eq!((old_start, new_start), (4, 4));
    }
}
