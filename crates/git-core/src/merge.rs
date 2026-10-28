use git2::{IndexConflict, Repository};
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
    #[error("no merge is currently in progress")]
    NotMerging,
    #[error("a merge is already in progress or has unresolved conflicts")]
    AlreadyMerging,
    #[error("'{0}' is a text conflict, not an add/delete conflict")]
    NotAnAddDeleteConflict(String),
    #[error("Can't fast-forward: '{0}' has diverged from the current branch.")]
    NotFastForward(String),
    #[error("'{0}' is a symlink or submodule conflict. Resolve it with git.")]
    UnsupportedEntry(String),
    #[error("'{0}' is inside a symlinked folder; resolve it with git.")]
    SymlinkedFolder(String),
}

/// Refuses a path whose existing parent folders include a symlink, so a resolution can never
/// write or delete outside the work tree through one.
fn ensure_inside_workdir(workdir: &Path, path: &str) -> Result<(), MergeError> {
    crate::stage::ensure_no_symlinked_parent(workdir, Path::new(path)).map_err(
        |error| match error {
            crate::stage::StageError::Io(error) => MergeError::Io(error),
            _ => MergeError::SymlinkedFolder(path.to_string()),
        },
    )
}

/// How "Merge into current branch" may move the branch, like `git merge --ff`, `--no-ff`
/// and `--ff-only`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastForward {
    /// Fast-forward when possible, else create a merge commit.
    #[default]
    Allow,
    /// Always create a merge commit, even when a fast-forward is possible.
    Never,
    /// Fast-forward or refuse (`MergeError::NotFastForward`).
    Only,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MergeOptions {
    pub fast_forward: FastForward,
    /// Stage the combined changes without committing or recording a merge (`--squash`).
    /// Ignores `fast_forward`.
    pub squash: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    UpToDate,
    FastForwarded,
    Merged,
    /// Squash merge staged the combined changes and wrote `SQUASH_MSG`; nothing is committed
    /// and no merge is in progress.
    Squashed,
    Conflicted {
        files: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictSegment {
    Clean { content: String },
    Conflict { ours: String, theirs: String },
}

/// Merges `target` into the current branch. `target` is a local branch name, or a full ref name
/// when it starts with `refs/` (the "Pull has diverged → Merge" path passes
/// `refs/remotes/<remote>/<branch>`).
pub fn start_merge(
    repo: &Repository,
    target: &str,
    opts: MergeOptions,
) -> Result<MergeOutcome, MergeError> {
    if is_merging(repo) || repo.index()?.has_conflicts() {
        return Err(MergeError::AlreadyMerging);
    }

    let refname = if target.starts_with("refs/") {
        target.to_string()
    } else {
        format!("refs/heads/{target}")
    };
    let their_reference = repo.find_reference(&refname)?;
    let their_commit = their_reference.peel_to_commit()?;
    let their_annotated = repo.reference_to_annotated_commit(&their_reference)?;

    let (analysis, _preference) = repo.merge_analysis(&[&their_annotated])?;

    if analysis.is_up_to_date() {
        return Ok(MergeOutcome::UpToDate);
    }

    // Safe checkouts create files through a symlinked tracked folder (#114).
    crate::worktree_guard::refuse_linked_folders(repo)?;
    if opts.squash {
        return squash_merge(repo, &their_annotated, their_commit.id());
    }

    if analysis.is_fast_forward() && opts.fast_forward != FastForward::Never {
        // Check out the target tree before moving the branch ref — a refused checkout
        // (modified/untracked files in the way) leaves the repo exactly as it was, matching
        // `branch::switch_branch`'s own ordering for the same reason.
        repo.checkout_tree(their_commit.as_object(), None)?;
        let mut head_ref = repo.head()?;
        head_ref.set_target(their_commit.id(), "merge: Fast-forward")?;
        return Ok(MergeOutcome::FastForwarded);
    }

    if !analysis.is_fast_forward() && opts.fast_forward == FastForward::Only {
        return Err(MergeError::NotFastForward(
            short_ref_name(target).to_string(),
        ));
    }

    repo.merge(&[&their_annotated], None, None)?;

    let files = conflicted_files(&repo.index()?)?;
    if files.is_empty() {
        Ok(MergeOutcome::Merged)
    } else {
        Ok(MergeOutcome::Conflicted { files })
    }
}

/// `git merge --squash`: merge into the index and worktree, write `SQUASH_MSG`, and leave no
/// merge state (`cleanup_state`), so the next commit has HEAD as its only parent. HEAD never
/// moves, even when a fast-forward was possible.
fn squash_merge(
    repo: &Repository,
    their_annotated: &git2::AnnotatedCommit<'_>,
    their_id: git2::Oid,
) -> Result<MergeOutcome, MergeError> {
    // Built before `repo.merge` so a failure here leaves the repository untouched.
    let message = build_squash_message(repo, their_id)?;
    repo.merge(&[their_annotated], None, None)?;
    let files = conflicted_files(&repo.index()?)?;
    std::fs::write(repo.path().join("SQUASH_MSG"), message)?;
    repo.cleanup_state()?;
    if files.is_empty() {
        Ok(MergeOutcome::Squashed)
    } else {
        Ok(MergeOutcome::Conflicted { files })
    }
}

/// git's `SQUASH_MSG` without the `Date:` lines: every commit reachable from `their_id` but not
/// from HEAD, newest first.
fn build_squash_message(repo: &Repository, their_id: git2::Oid) -> Result<String, MergeError> {
    let mut walk = repo.revwalk()?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME)?;
    walk.push(their_id)?;
    if let Ok(head) = repo.head().and_then(|head| head.peel_to_commit()) {
        walk.hide(head.id())?;
    }
    let mut message = String::from("Squashed commit of the following:\n");
    for oid in walk {
        let commit = repo.find_commit(oid?)?;
        let author = commit.author();
        message.push_str(&format!(
            "\ncommit {}\nAuthor: {} <{}>\n\n",
            commit.id(),
            String::from_utf8_lossy(author.name_bytes()),
            String::from_utf8_lossy(author.email_bytes()),
        ));
        let body = String::from_utf8_lossy(commit.message_bytes()).into_owned();
        for line in body.trim_end_matches('\n').lines() {
            message.push_str("    ");
            message.push_str(line);
            message.push('\n');
        }
    }
    Ok(message)
}

fn conflicted_files(index: &git2::Index) -> Result<Vec<String>, MergeError> {
    let mut files = Vec::new();
    if !index.has_conflicts() {
        return Ok(files);
    }
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        if let Some(path) = conflict_path(&conflict) {
            if !files.contains(&path) {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// The pending squash merge's commit message (`<gitdir>/SQUASH_MSG`), if any.
pub fn squash_message(repo: &Repository) -> Option<String> {
    std::fs::read_to_string(repo.path().join("SQUASH_MSG")).ok()
}

/// Removes `SQUASH_MSG` once it no longer applies (commit, abort, reset, undo); `cleanup_state`
/// leaves it behind. A missing file is fine.
pub(crate) fn clear_squash_message(repo: &Repository) -> std::io::Result<()> {
    match std::fs::remove_file(repo.path().join("SQUASH_MSG")) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// `target` without a `refs/heads/` or `refs/remotes/` prefix, for messages.
fn short_ref_name(target: &str) -> &str {
    target
        .strip_prefix("refs/heads/")
        .or_else(|| target.strip_prefix("refs/remotes/"))
        .unwrap_or(target)
}

/// The dialog's first default: `merge.ff` from the repository's (layered) git config.
pub fn merge_defaults(repo: &Repository) -> MergeOptions {
    let value = repo
        .config()
        .ok()
        .and_then(|config| config.get_string("merge.ff").ok());
    MergeOptions {
        fast_forward: fast_forward_from_merge_ff(value.as_deref()),
        squash: false,
    }
}

/// Maps a `merge.ff` value: `only` → `Only`, a false git boolean → `Never`, anything else or
/// unset → `Allow`.
pub fn fast_forward_from_merge_ff(value: Option<&str>) -> FastForward {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("only") => FastForward::Only,
        Some("false" | "no" | "off" | "0") => FastForward::Never,
        _ => FastForward::Allow,
    }
}

pub(crate) fn conflict_path(conflict: &IndexConflict) -> Option<String> {
    conflict
        .our
        .as_ref()
        .or(conflict.their.as_ref())
        .or(conflict.ancestor.as_ref())
        .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
}

pub(crate) fn find_conflict(repo: &Repository, path: &str) -> Result<IndexConflict, MergeError> {
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
    find_conflict(repo, path)?;

    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    ensure_inside_workdir(workdir, path)?;
    // Never through a symlink that replaced the file (#114).
    if !crate::worktree_guard::write_beneath(workdir, path, resolved_content.as_bytes())? {
        return Err(MergeError::SymlinkedFolder(path.to_string()));
    }

    let mut index = repo.index()?;
    index.add_path(Path::new(path))?;
    index.write()?;
    Ok(())
}

/// Which side of a conflict to keep for a whole file, like `git checkout --ours/--theirs`.
/// "Ours" is stage 2 (HEAD); "theirs" is stage 3, the incoming commit (during a rebase, the
/// commit being replayed, so "ours" is the branch being rebased onto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictSide {
    Ours,
    Theirs,
}

/// Resolves a conflicted path by taking one side's whole file: the chosen stage is checked out
/// through libgit2 (so filters and the file mode apply, like `git checkout --ours`) and staged.
/// When the chosen side deleted the file, the working file is removed and the deletion staged.
/// Works for content, add/delete and binary conflicts in any operation (merge, cherry-pick,
/// revert, rebase, stash). Symlink and submodule conflicts are refused. Not recorded for undo:
/// the paused operation's entry covers it.
pub fn resolve_whole_file(
    repo: &Repository,
    path: &str,
    side: ConflictSide,
) -> Result<(), MergeError> {
    if !crate::launch::is_safe_relative(path) {
        return Err(MergeError::NoConflict(path.to_string()));
    }
    let conflict = find_conflict(repo, path)?;
    let unsupported = [&conflict.ancestor, &conflict.our, &conflict.their]
        .into_iter()
        .flatten()
        .any(|entry| matches!(entry.mode, 0o120000 | 0o160000));
    if unsupported {
        return Err(MergeError::UnsupportedEntry(path.to_string()));
    }
    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    ensure_inside_workdir(workdir, path)?;
    let chosen = match side {
        ConflictSide::Ours => conflict.our.as_ref(),
        ConflictSide::Theirs => conflict.their.as_ref(),
    };
    match chosen {
        Some(_) => {
            let index = repo.index()?;
            crate::worktree_guard::prepare_forced_checkout::<MergeError, _>(
                repo,
                Some(&crate::worktree_guard::GuardSource::Index(&index)),
                Some(&[path]),
            )?;
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout.force().disable_pathspec_match(true).path(path);
            match side {
                ConflictSide::Ours => checkout.use_ours(true),
                ConflictSide::Theirs => checkout.use_theirs(true),
            };
            repo.checkout_index(None, Some(&mut checkout))?;
            let mut index = repo.index()?;
            index.add_path(Path::new(path))?;
            index.write()?;
        }
        None => {
            crate::worktree_guard::remove_beneath(workdir, path)?;
            let mut index = repo.index()?;
            index.remove_path(Path::new(path))?;
            index.write()?;
        }
    }
    Ok(())
}

/// What `mark_resolved` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkResolvedOutcome {
    /// The work tree file was staged, which cleared the conflict.
    Resolved,
    /// Not staged: the file still has `<<<<<<<` / `>>>>>>>` lines. Ask, then call again with
    /// `force`.
    MarkersRemain,
}

/// Stages the work tree file of a conflicted path (or its deletion, when the file is gone),
/// which clears its conflict entries, like `git add` after a merge tool. Without `force`, a
/// file that still contains conflict markers is left alone and reported. Not recorded for
/// undo, like `resolve_conflict`.
pub fn mark_resolved(
    repo: &Repository,
    path: &str,
    force: bool,
) -> Result<MarkResolvedOutcome, MergeError> {
    if !crate::launch::is_safe_relative(path) {
        return Err(MergeError::NoConflict(path.to_string()));
    }
    find_conflict(repo, path)?;
    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    ensure_inside_workdir(workdir, path)?;
    let full = workdir.join(path);
    let meta = std::fs::symlink_metadata(&full).ok();
    if !force
        && meta.as_ref().is_some_and(|meta| meta.is_file())
        && has_conflict_markers(&std::fs::read(&full)?)
    {
        return Ok(MarkResolvedOutcome::MarkersRemain);
    }
    let mut index = repo.index()?;
    if meta.is_some() {
        index.add_path(Path::new(path))?;
    } else {
        index.remove_path(Path::new(path))?;
    }
    index.write()?;
    Ok(MarkResolvedOutcome::Resolved)
}

/// A line starting with `<<<<<<<` or `>>>>>>>` followed by a space or the line end. `=======`
/// alone is not a marker (it is also a Markdown/RST underline). Binary files (a NUL in the
/// first 8000 bytes, git's heuristic) are never reported.
pub(crate) fn has_conflict_markers(content: &[u8]) -> bool {
    if content.iter().take(8000).any(|b| *b == 0) {
        return false;
    }
    content.split(|b| *b == b'\n').any(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        [&b"<<<<<<<"[..], &b">>>>>>>"[..]].iter().any(|marker| {
            line.strip_prefix(*marker)
                .is_some_and(|rest| rest.is_empty() || rest[0] == b' ')
        })
    })
}

pub fn abort_merge(repo: &Repository) -> Result<(), MergeError> {
    use git2::RepositoryState::{CherryPick, CherryPickSequence, Merge, Revert, RevertSequence};
    let in_operation = matches!(
        repo.state(),
        Merge | CherryPick | CherryPickSequence | Revert | RevertSequence
    );
    if !in_operation && !repo.index()?.has_conflicts() {
        return Err(MergeError::NotMerging);
    }
    let head_commit = repo.head()?.peel_to_commit()?;
    let head_tree = head_commit.tree()?;

    // Like `git merge --abort` (reset --merge): only restore paths the operation touched, so
    // unrelated unstaged edits survive.
    let mut touched: Vec<Vec<u8>> = Vec::new();
    let mut index = repo.index()?;
    for delta in repo
        .diff_tree_to_index(Some(&head_tree), Some(&index), None)?
        .deltas()
    {
        for file in [delta.old_file(), delta.new_file()] {
            if let Some(path) = file.path_bytes() {
                touched.push(path.to_vec());
            }
        }
    }
    for conflict in index.conflicts()?.filter_map(|c| c.ok()) {
        if let Some(entry) = conflict.our.or(conflict.their).or(conflict.ancestor) {
            touched.push(entry.path);
        }
    }
    touched.sort();
    touched.dedup();

    index.read_tree(&head_tree)?;
    index.write()?;

    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    // The forced checkout below writes through a symlink that replaced a touched file or a
    // folder on the way to one (#114): remove such links first, like git.
    crate::worktree_guard::prepare_forced_checkout::<MergeError, _>(
        repo,
        Some(&crate::worktree_guard::GuardSource::Tree(&head_tree)),
        Some(&touched),
    )?;
    let mut to_checkout = Vec::new();
    for path in touched {
        let rel = path_from_bytes(&path);
        if head_tree.get_path(&rel).is_ok() {
            to_checkout.push(path);
        } else if let Ok(rel) = std::str::from_utf8(&path) {
            // Added by the operation: not in HEAD, so remove it, never through a link and never
            // a folder.
            crate::worktree_guard::remove_beneath(workdir, rel)?;
        }
    }
    if !to_checkout.is_empty() {
        let mut checkout = git2::build::CheckoutBuilder::new();
        // Paths are literal file names, not globs (`[ab].txt` must not match `a.txt`).
        checkout.force().disable_pathspec_match(true);
        for path in to_checkout {
            checkout.path(path);
        }
        repo.checkout_head(Some(&mut checkout))?;
    }
    repo.cleanup_state()?;
    clear_squash_message(repo)?;
    Ok(())
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStrExt;
    std::path::PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> std::path::PathBuf {
    std::path::PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

pub fn merge_message(repo: &Repository) -> Option<String> {
    repo.message().ok()
}

pub fn is_merging(repo: &Repository) -> bool {
    repo.state() == git2::RepositoryState::Merge
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileConflictChoice {
    Ours,
    Theirs,
    Delete,
}

pub fn resolve_add_delete_conflict(
    repo: &Repository,
    path: &str,
    choice: FileConflictChoice,
) -> Result<(), MergeError> {
    let conflict = find_conflict(repo, path)?;
    if let (Some(_), Some(_), Some(_)) = (&conflict.ancestor, &conflict.our, &conflict.their) {
        return Err(MergeError::NotAnAddDeleteConflict(path.to_string()));
    }
    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    ensure_inside_workdir(workdir, path)?;
    let mut index = repo.index()?;

    let entry = match choice {
        FileConflictChoice::Ours => conflict.our.as_ref(),
        FileConflictChoice::Theirs => conflict.their.as_ref(),
        FileConflictChoice::Delete => None,
    };

    match entry {
        Some(entry) => {
            let blob = repo.find_blob(entry.id)?;
            if !crate::worktree_guard::write_beneath(workdir, path, blob.content())? {
                return Err(MergeError::SymlinkedFolder(path.to_string()));
            }
            index.add_path(Path::new(path))?;
        }
        None => {
            // Ignore a missing file — the case where the chosen side already means
            // "deleted" (e.g. `Theirs` when `their` is `None`), so there's nothing on disk
            // to remove in the first place.
            let _ = crate::worktree_guard::remove_beneath(workdir, path);
            index.remove_path(Path::new(path))?;
        }
    }
    index.write()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_markers_are_seven_angle_brackets_at_a_line_start() {
        assert!(has_conflict_markers(b"a\n<<<<<<< HEAD\nb\n"));
        assert!(has_conflict_markers(b"a\r\n>>>>>>>\r\n"));
        assert!(has_conflict_markers(b">>>>>>>"));
        assert!(!has_conflict_markers(b"Title\n=======\n"));
        assert!(!has_conflict_markers(b"x <<<<<<< y\n"));
        assert!(!has_conflict_markers(b"<<<<<<<<< nine\n"));
        assert!(!has_conflict_markers(b"\0binary\n<<<<<<< x\n"));
    }
}
