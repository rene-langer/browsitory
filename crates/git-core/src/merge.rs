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
    /// Mark resolved on a FIFO, socket or device (#165): libgit2 would block reading it.
    #[error(
        "'{0}' is a FIFO, socket or device, not a regular file; replace it before marking it \
         resolved."
    )]
    NotAFile(String),
    /// Mark resolved on a file it can't open safely: no permission, being deleted (Windows), or
    /// swapped for a link at that moment (#199).
    #[error("'{0}' can't be read safely; check its permissions and try again.")]
    Unreadable(String),
    /// Mark resolved on a folder that can't be staged as a nested repository's commit, which is
    /// the only way git stages a folder (#213). Nothing was staged.
    #[error("{}", .1.message(.0))]
    FolderNotStaged(String, NotStagedReason),
    #[error("{0}")]
    LinkedFolder(#[from] crate::LinkedFolder),
    /// The file to write is read-only; nothing was written (#209).
    #[error("'{0}' is read-only. Make it writable, then try again.")]
    ReadOnlyFile(String),
    /// Windows: another program keeps the file open so it can't be replaced; nothing was
    /// written (#239).
    #[error("'{0}' is open in another program. Close it, then try again.")]
    FileInUse(String),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
    /// Windows: a checkout stopped at a file another program holds open (#303).
    #[error(transparent)]
    HeldOpen(#[from] crate::HeldOpen),
    /// Abort would reset a path the operation touched (staged) whose work-tree file was changed
    /// again since: both versions would be lost. git's `reset --merge` refuses the same way
    /// ("Entry '…' not uptodate"); nothing was changed.
    #[error(
        "Abort would discard your changes to '{0}'. Save them elsewhere or discard them, then \
         try again."
    )]
    WouldDiscard(String),
    /// Abort reset the files but couldn't remove one of git's merge/cherry-pick/revert state
    /// files (`commit::clear_operation_files`).
    #[error("Couldn't remove git's merge state: {0}")]
    StateFiles(std::io::Error),
    /// Abort during a `git rebase` or `git am` started in a terminal: the paused step is the
    /// rebase's, which only the terminal ends (#263). Nothing was changed.
    #[error(transparent)]
    TerminalRebase(#[from] crate::repo::TerminalRebaseInProgress),
}

/// Why Mark resolved didn't stage a folder (`MergeError::FolderNotStaged`, #213).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotStagedReason {
    /// No repository: a folder of plain files, a `.git` that is a link or missing its git
    /// folder (a `gitdir:` to a folder that is gone), or no HEAD.
    NotARepository,
    /// A repository whose branch has no commit yet.
    NoCommit,
    /// A repository whose checked-out commit Browsitory can't read; what it can't read.
    Unsupported(&'static str),
}

impl NotStagedReason {
    fn message(&self, path: &str) -> String {
        match self {
            Self::NotARepository => format!(
                "'{path}' is a folder, not a repository. Only a repository can be marked \
                 resolved as a folder."
            ),
            Self::NoCommit => format!(
                "'{path}' is a repository without a commit checked out. Commit in it first, \
                 then try again."
            ),
            Self::Unsupported(what) => format!(
                "Browsitory can't read which commit the repository '{path}' has checked out \
                 ({what}). Stage it with git."
            ),
        }
    }
}

/// Refuses while a symlink stands where HEAD, the index or the merge's outcome has a folder: a
/// fast-forward checks out `theirs`' tree, any other merge writes the merged index.
fn refuse_linked_folders(
    repo: &Repository,
    theirs: &git2::Commit,
    fast_forward: bool,
) -> Result<(), MergeError> {
    use crate::worktree_guard::{GuardSource, Incoming};
    if fast_forward {
        let tree = theirs.tree()?;
        return crate::worktree_guard::refuse_linked_folders(repo, &[Incoming::Checkout(&tree)]);
    }
    let ours = repo.head()?.peel_to_commit()?;
    let merged = repo.merge_commits(&ours, theirs, None)?;
    crate::worktree_guard::refuse_linked_folders(
        repo,
        &[Incoming::Merge(GuardSource::Index(&merged))],
    )
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
    // A merge step of a terminal `rebase --rebase-merges` (MERGE_HEAD while libgit2 reports the
    // rebase) is the rebase's: a second merge on top would be committed as part of it (#263).
    if repo.path().join("MERGE_HEAD").exists() {
        crate::repo::check_no_terminal_rebase(repo)?;
    }
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

    // Safe checkouts create files through a symlinked folder (#114).
    refuse_linked_folders(
        repo,
        &their_commit,
        analysis.is_fast_forward() && !opts.squash && opts.fast_forward != FastForward::Never,
    )?;
    if opts.squash {
        return squash_merge(repo, &their_annotated, their_commit.id());
    }

    if analysis.is_fast_forward() && opts.fast_forward != FastForward::Never {
        // Check out the target tree before moving the branch ref — a refused checkout
        // (modified/untracked files in the way) leaves the repo exactly as it was, matching
        // `branch::switch_branch`'s own ordering for the same reason.
        // A file another program holds open: say so, not libgit2's raw error (#303).
        repo.checkout_tree(their_commit.as_object(), None).map_err(
            crate::worktree_guard::held_open::<MergeError>(
                repo.workdir(),
                crate::HeldDuring::FastForward,
            ),
        )?;
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
/// merge state (`commit::clear_commit_state`, which keeps a terminal cherry-pick's todo, #241),
/// so the next commit has HEAD as its only parent. HEAD never moves, even when a fast-forward
/// was possible.
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
    crate::commit::clear_commit_state(repo)?;
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

/// Removes `SQUASH_MSG` once it no longer applies (commit, abort, reset, undo);
/// `commit::clear_operation_files` leaves it behind. A missing file is fine.
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
    if !write_resolved(workdir, path, resolved_content.as_bytes())? {
        return Err(MergeError::SymlinkedFolder(path.to_string()));
    }

    let mut index = repo.index()?;
    index.add_path(Path::new(path))?;
    index.write()?;
    Ok(())
}

/// `worktree_guard::write_beneath`, with a read-only file reported as
/// [`MergeError::ReadOnlyFile`] (#209) and a file open in another program as
/// [`MergeError::FileInUse`] (#239).
fn write_resolved(workdir: &Path, path: &str, bytes: &[u8]) -> Result<bool, MergeError> {
    crate::worktree_guard::write_beneath(workdir, path, bytes).map_err(|error| {
        if crate::worktree_guard::is_read_only(&error) {
            MergeError::ReadOnlyFile(path.to_string())
        } else if crate::worktree_guard::is_in_use(&error) {
            MergeError::FileInUse(path.to_string())
        } else {
            MergeError::Io(error)
        }
    })
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
            // The forced checkout would replace a read-only file; refuse it as the other
            // resolutions do, before anything is written (#209).
            if crate::worktree_guard::is_read_only_beneath(workdir, path) {
                return Err(MergeError::ReadOnlyFile(path.to_string()));
            }
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
            // A file another program holds open: say so, not libgit2's raw error (#268).
            repo.checkout_index(None, Some(&mut checkout))
                .map_err(|error| {
                    match crate::worktree_guard::held_open_after_checkout(workdir, &error, [path]) {
                        Some(path) => MergeError::FileInUse(path.to_string()),
                        None => error.into(),
                    }
                })?;
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
    mark_resolved_with_pause(repo, path, force, &mut || {})
}

/// `mark_resolved` with a test seam: `after_check` runs between the marker check and the
/// staging, so a test can swap the file in that instant (#199).
///
/// The file is read once, through `worktree_guard::read_beneath` (no link followed at any
/// component, a FIFO never opened for reading), and exactly those bytes are checked and staged.
/// `Index::add_path` isn't used for a file or link: libgit2 would `lstat` and open the path a
/// second time, following a link and blocking on a FIFO swapped in meanwhile, and stage content
/// that was never checked (#199). A file's blob goes through `blob_writer` with the path as
/// hint, so the same clean filters (`core.autocrlf`, `.gitattributes`) apply as with
/// `add_path`; `remove_path` moves the conflict stages to resolve-undo (REUC), as `add_path`
/// does. The new entry carries the stat data of the descriptor the bytes were read from, so
/// status trusts it like any entry `git add` wrote. A folder at the path is probed and held
/// through `worktree_guard::probe_beneath`, never reopened by path: a nested repository is
/// staged as a gitlink to the commit its HEAD names, read through the held folder (#213), and
/// any other folder is refused (`FolderNotStaged`).
#[doc(hidden)]
pub fn mark_resolved_with_pause(
    repo: &Repository,
    path: &str,
    force: bool,
    after_check: &mut dyn FnMut(),
) -> Result<MarkResolvedOutcome, MergeError> {
    mark_resolved_with_pauses(repo, path, force, after_check, &mut || {})
}

/// `mark_resolved_with_pause` with a second test seam: `folder_held` runs once a folder at the
/// path is held and before anything in it is read, so a test can swap the folder then (#213).
#[doc(hidden)]
pub fn mark_resolved_with_pauses(
    repo: &Repository,
    path: &str,
    force: bool,
    after_check: &mut dyn FnMut(),
    folder_held: &mut dyn FnMut(),
) -> Result<MarkResolvedOutcome, MergeError> {
    use crate::worktree_guard::Leaf;
    if !crate::launch::is_safe_relative(path) {
        return Err(MergeError::NoConflict(path.to_string()));
    }
    find_conflict(repo, path)?;
    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    ensure_inside_workdir(workdir, path)?;
    let full = workdir.join(path);
    // A FIFO, socket or device: git refuses to add one too (#165).
    if let Ok(meta) = std::fs::symlink_metadata(&full) {
        let kind = meta.file_type();
        if !kind.is_file() && !kind.is_symlink() && !kind.is_dir() {
            return Err(MergeError::NotAFile(path.to_string()));
        }
    }
    let mut index = repo.index()?;
    // The mode git keeps when `core.filemode`/`core.symlinks` is off: the stage-0 entry's, else
    // "ours"', "theirs"' or the base's, as libgit2's `index_existing_and_best`.
    let previous_mode = [0, 2, 3, 1]
        .into_iter()
        .find_map(|stage| index.get_path(Path::new(path), stage))
        .map(|entry| entry.mode);
    let config = crate::undo::worktree_config(repo).map_err(undo_error)?;
    let leaf = crate::worktree_guard::read_beneath(workdir, path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            MergeError::Unreadable(path.to_string())
        } else {
            MergeError::Io(error)
        }
    })?;
    match leaf {
        Some(Leaf::Link(target)) => {
            after_check();
            let id = repo.blob(&target)?;
            index.remove_path(Path::new(path))?;
            index.add(&crate::undo::index_entry(path, id, crate::undo::MODE_LINK))?;
        }
        Some(Leaf::File {
            bytes,
            executable,
            stat,
        }) => {
            let mode = crate::undo::file_mode(&config, executable, previous_mode);
            // A link kept as a file (`core.symlinks` off) holds a target, not text to check.
            if !force && mode != crate::undo::MODE_LINK && has_conflict_markers(&bytes) {
                return Ok(MarkResolvedOutcome::MarkersRemain);
            }
            after_check();
            // Through the clean filters, also for a link kept as a file, as git does.
            let mut writer = repo.blob_writer(Some(Path::new(path)))?;
            std::io::Write::write_all(&mut writer, &bytes)?;
            let id = writer.commit()?;
            index.remove_path(Path::new(path))?;
            index.add(&staged_entry(path, id, mode, &stat))?;
        }
        None => {
            after_check();
            // Judged again through the held folders, never by path, so the type below is the
            // path's own, not something beyond a folder that became a link.
            use crate::worktree_guard::Probe;
            match crate::worktree_guard::probe_beneath(workdir, path)? {
                Probe::Missing => {
                    index.remove_path(Path::new(path))?;
                }
                Probe::BeyondLink => return Err(MergeError::SymlinkedFolder(path.to_string())),
                // A file or link `read_beneath` couldn't open (no permission, being deleted, or
                // swapped meanwhile).
                Probe::File | Probe::Link => return Err(MergeError::Unreadable(path.to_string())),
                Probe::Other => return Err(MergeError::NotAFile(path.to_string())),
                Probe::Folder(folder) => {
                    folder_held();
                    let head = nested_head(path, &folder)?;
                    let stat = folder.stat()?;
                    index.remove_path(Path::new(path))?;
                    index.add(&staged_entry(path, head, MODE_GITLINK, &stat))?;
                }
            }
        }
    }
    index.write()?;
    Ok(MarkResolvedOutcome::Resolved)
}

/// Git's file mode for a gitlink (a nested repository's commit).
const MODE_GITLINK: u32 = 0o160000;

/// The most [`nested_head`] reads of a small git file (`.git`, HEAD, `commondir`, a loose ref).
const SMALL_GIT_FILE_LIMIT: u64 = 4096;
/// The most [`nested_head`] reads of `packed-refs`.
const PACKED_REFS_LIMIT: u64 = 64 << 20;

/// Refs that live in each worktree's own git folder, not the common one (git's
/// `is_per_worktree_ref`).
const PER_WORKTREE_REFS: [&str; 3] = ["refs/worktree/", "refs/bisect/", "refs/rewritten/"];

/// The commit a nested repository has checked out, as `git add` stages it for a folder (#213):
/// HEAD, followed through symbolic refs to a loose ref or `packed-refs`. Everything is read
/// through held folders (`HeldFolder::resolve`), never by path, no link followed and no FIFO
/// opened: `.git` (a folder, or a `gitdir:` file), HEAD, `commondir` and the refs, each file
/// size-capped. A `gitdir:` or `commondir` outside the work tree is opened by path once and
/// then held.
fn nested_head(
    path: &str,
    folder: &crate::worktree_guard::HeldFolder,
) -> Result<git2::Oid, MergeError> {
    use crate::worktree_guard::{HeldFolder, Leaf};
    let refuse = |reason| MergeError::FolderNotStaged(path.to_string(), reason);
    let read = |dir: &HeldFolder, name: &str, limit| match dir.read(name, limit) {
        Ok(Some(Leaf::File { bytes, .. })) => Ok(Some(bytes)),
        Ok(_) => Ok(None),
        Err(error) if crate::worktree_guard::is_too_large(&error) => Err(refuse(
            NotStagedReason::Unsupported("a file in its git folder is too large"),
        )),
        Err(error) => Err(MergeError::Io(error)),
    };
    let not_a_repository = || refuse(NotStagedReason::NotARepository);
    let git = match read(folder, ".git", SMALL_GIT_FILE_LIMIT)? {
        Some(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let target = text
                .trim()
                .strip_prefix("gitdir:")
                .map(str::trim)
                .ok_or_else(not_a_repository)?;
            folder.resolve(Path::new(target))?
        }
        // A folder; a link, FIFO or nothing gives `None`.
        None => folder.resolve(Path::new(".git"))?,
    }
    .ok_or_else(not_a_repository)?;
    let mut content = read(&git, "HEAD", SMALL_GIT_FILE_LIMIT)?.ok_or_else(not_a_repository)?;
    // A linked worktree's branches are in its repository's common folder.
    let common = match read(&git, "commondir", SMALL_GIT_FILE_LIMIT)? {
        Some(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            git.resolve(Path::new(text.trim()))?
                .ok_or_else(not_a_repository)?
        }
        None => git,
    };
    // git reads at most 5 refs, HEAD included (`SYMREF_MAXDEPTH`): HEAD and 4 symbolic refs.
    for _ in 0..5 {
        let text = String::from_utf8_lossy(&content);
        let text = text.trim();
        let Some(name) = text.strip_prefix("ref:").map(str::trim) else {
            return parse_oid(text).map_err(refuse);
        };
        // What git's reftable backend writes into HEAD so older git refuses the repository.
        if name == "refs/heads/.invalid" {
            return Err(refuse(NotStagedReason::Unsupported("reftable refs")));
        }
        if PER_WORKTREE_REFS
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            return Err(refuse(NotStagedReason::Unsupported(
                "HEAD on a per-worktree ref",
            )));
        }
        if !name.starts_with("refs/") {
            return Err(refuse(NotStagedReason::Unsupported("HEAD outside refs/")));
        }
        match read(&common, name, SMALL_GIT_FILE_LIMIT)? {
            Some(next) => content = next,
            None => {
                let packed = read(&common, "packed-refs", PACKED_REFS_LIMIT)?.unwrap_or_default();
                let id = String::from_utf8_lossy(&packed)
                    .lines()
                    .filter(|line| !line.starts_with('#') && !line.starts_with('^'))
                    .find_map(|line| {
                        let (id, ref_name) = line.split_once(' ')?;
                        (ref_name.trim_end() == name).then(|| id.trim().to_string())
                    });
                return match id {
                    Some(id) => parse_oid(&id).map_err(refuse),
                    // An unborn branch.
                    None => Err(refuse(NotStagedReason::NoCommit)),
                };
            }
        }
    }
    Err(refuse(NotStagedReason::Unsupported(
        "symbolic refs nested too deeply",
    )))
}

/// A full hexadecimal SHA-1 object id (git2 would also take a prefix).
fn parse_oid(text: &str) -> Result<git2::Oid, NotStagedReason> {
    let hex = text.bytes().all(|b| b.is_ascii_hexdigit());
    match text.len() {
        40 if hex => git2::Oid::from_str(text)
            .map_err(|_| NotStagedReason::Unsupported("a malformed HEAD or ref")),
        64 if hex => Err(NotStagedReason::Unsupported("SHA-256 object ids")),
        _ => Err(NotStagedReason::Unsupported("a malformed HEAD or ref")),
    }
}

/// The index entry `git add` would write for a file read with `stat`: git truncates the times,
/// device, inode and size to 32 bits.
fn staged_entry(
    path: &str,
    id: git2::Oid,
    mode: u32,
    stat: &crate::worktree_guard::FileStat,
) -> git2::IndexEntry {
    let time = |(secs, nanos): (i64, u32)| git2::IndexTime::new(secs as i32, nanos);
    git2::IndexEntry {
        ctime: time(stat.ctime),
        mtime: time(stat.mtime),
        dev: stat.dev as u32,
        ino: stat.ino as u32,
        uid: stat.uid,
        gid: stat.gid,
        file_size: stat.size as u32,
        ..crate::undo::index_entry(path, id, mode)
    }
}

fn undo_error(error: crate::undo::UndoError) -> MergeError {
    match error {
        crate::undo::UndoError::Git(error) => MergeError::Git(error),
        crate::undo::UndoError::Io(error) => MergeError::Io(error),
        other => MergeError::Git(git2::Error::from_str(&other.to_string())),
    }
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
    let reset = plan_operation_reset(repo)?;
    apply_operation_reset(repo, reset)?;
    // Only the operation's own files, as git's `remove_branch_state` after `git reset --merge`:
    // a terminal bisect, rebase or cherry-pick list goes on, except that aborting the list's
    // last pick ends the list (git's `have_finished_the_last_pick`, #249).
    crate::commit::clear_commit_state(repo).map_err(MergeError::StateFiles)?;
    clear_squash_message(repo)?;
    Ok(())
}

/// Aborts the paused step of a terminal `git cherry-pick A B C` like `abort_merge`, but always
/// keeps the list (`.git/sequencer`), also after its last pick: the caller
/// (`pick::abort_terminal_sequence`) removes it once the rest of the abort has worked (#253).
pub(crate) fn abort_step_keeping_list(
    repo: &Repository,
    reset: OperationReset,
) -> Result<(), MergeError> {
    apply_operation_reset(repo, reset)?;
    crate::commit::clear_operation_files(repo).map_err(MergeError::StateFiles)?;
    clear_squash_message(repo)?;
    Ok(())
}

/// What Abort resets: the paths the paused operation touched (staged changes and conflicts).
/// Made by `plan_operation_reset`, which has checked that resetting them loses nothing git's
/// `reset --merge` would keep; applied by `apply_operation_reset`.
pub(crate) struct OperationReset {
    touched: Vec<Vec<u8>>,
}

impl OperationReset {
    /// The paths the reset puts back to HEAD (index and work tree).
    pub(crate) fn paths(&self) -> &[Vec<u8>] {
        &self.touched
    }
}

/// The checks of `abort_merge`, before anything is written: an operation (or its conflicts)
/// is there, and no touched path outside the conflicts has work-tree changes on top of its
/// staged version (`MergeError::WouldDiscard`, like git's "not uptodate"). A conflicted path's
/// work-tree file is the user's resolution of the step, which Abort discards, as git does.
pub(crate) fn plan_operation_reset(repo: &Repository) -> Result<OperationReset, MergeError> {
    use git2::RepositoryState::{CherryPick, CherryPickSequence, Merge, Revert, RevertSequence};
    // During a terminal rebase or `git am` the paused step is the rebase's, always: its
    // conflicts and MERGE_MSG, and also the MERGE_HEAD git writes for a `--rebase-merges` merge
    // step and the CHERRY_PICK_HEAD of an empty pick (`rebase -i`). Resetting it would make
    // `git rebase --continue` go on as if the step were resolved to nothing, dropping a merged
    // side branch. A merge started by hand during the rebase can't be told apart from the
    // rebase's own merge step, so Abort refuses that too; `git merge --abort` ends it (#263).
    crate::repo::check_no_terminal_rebase(repo)?;
    // The worker keeps one handle per repository, whose index may predate a merge or pick run
    // in a terminal: read the one on disk, or Abort would leave that operation's conflicts.
    repo.index()?.read(false)?;
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
    let index = repo.index()?;
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
    let conflicted: std::collections::HashSet<Vec<u8>> = index
        .conflicts()?
        .filter_map(|c| c.ok())
        .flat_map(|c| [c.ancestor, c.our, c.their])
        .flatten()
        .map(|entry| entry.path)
        .collect();
    let checked: Vec<&Vec<u8>> = touched
        .iter()
        .filter(|path| !conflicted.contains(*path))
        .collect();
    if !checked.is_empty() {
        let mut options = git2::DiffOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .disable_pathspec_match(true);
        for path in &checked {
            options.pathspec(path.as_slice());
        }
        let diff = repo.diff_index_to_workdir(Some(&index), Some(&mut options))?;
        for delta in diff.deltas() {
            // A deleted file has nothing to lose; a conflict is the step's own.
            if matches!(
                delta.status(),
                git2::Delta::Deleted | git2::Delta::Conflicted
            ) {
                continue;
            }
            if let Some(path) = delta.new_file().path().or(delta.old_file().path()) {
                return Err(MergeError::WouldDiscard(
                    path.to_string_lossy().into_owned(),
                ));
            }
        }
    }
    Ok(OperationReset { touched })
}

/// `abort_merge` up to the state files: the planned paths back to HEAD, first in the work tree,
/// then in the index. A checkout that stops part-way (a file another program holds open, a
/// read-only file) leaves the index with the paths it didn't reach, so aborting again
/// finishes (#321): the paths it did reach are put back in full, in the work tree and the index.
/// A file the operation added that can't be removed (Windows: held open) doesn't stop the rest;
/// it stays in the index for the next abort.
fn apply_operation_reset(repo: &Repository, reset: OperationReset) -> Result<(), MergeError> {
    let touched = reset.touched;
    let head_tree = repo.head()?.peel_to_commit()?.tree()?;
    let workdir = repo.workdir().ok_or(MergeError::NoWorkdir)?;
    // The forced checkout below writes through a symlink that replaced a touched file or a
    // folder on the way to one (#114): remove such links first, like git.
    crate::worktree_guard::prepare_forced_checkout::<MergeError, _>(
        repo,
        Some(&crate::worktree_guard::GuardSource::Tree(&head_tree)),
        Some(&touched),
    )?;
    let written = crate::partial_checkout::Written::default();
    let mut to_checkout = Vec::new();
    // The first file added by the operation that couldn't be removed. The rest of the abort
    // still runs, so aborting again has as little left to do as possible.
    let mut not_removed: Option<(String, std::io::Error)> = None;
    for path in touched {
        let rel = path_from_bytes(&path);
        if head_tree.get_path(&rel).is_ok() {
            to_checkout.push(path);
        } else if let Ok(rel) = std::str::from_utf8(&path) {
            // Added by the operation: not in HEAD, so remove it, never through a link and never
            // a folder.
            match crate::worktree_guard::remove_beneath(workdir, rel) {
                Ok(_) => written.push(path.clone()),
                Err(error) => {
                    not_removed.get_or_insert((rel.to_string(), error));
                }
            }
        }
    }
    let mut checked_out = Ok(());
    if !to_checkout.is_empty() {
        let mut checkout = git2::build::CheckoutBuilder::new();
        // Paths are literal file names, not globs (`[ab].txt` must not match `a.txt`).
        checkout.force().disable_pathspec_match(true);
        for path in to_checkout {
            checkout.path(path);
        }
        written.record(&mut checkout);
        checked_out = repo.checkout_head(Some(&mut checkout));
    }
    let failed = match (not_removed, checked_out) {
        (None, Ok(())) => None,
        (Some(not_removed), _) => Some(Err(not_removed)),
        (None, Err(error)) => Some(Ok(error)),
    };
    if let Some(failed) = failed {
        let finished = crate::partial_checkout::roll_forward(repo, &head_tree, &written);
        // Nothing put back yet: nothing changed, so it is the plain "try again".
        let during = if written.is_empty() {
            crate::HeldDuring::RolledBack
        } else {
            crate::HeldDuring::Abort
        };
        return Err(match failed {
            Err((path, error)) => {
                if finished.is_ok() && crate::worktree_guard::is_held_open(workdir, &path) {
                    crate::HeldOpen { path, during }.into()
                } else {
                    MergeError::Io(error)
                }
            }
            Ok(error) => {
                crate::partial_checkout::report::<MergeError>(repo, finished, during, error)
            }
        });
    }
    let mut index = repo.index()?;
    index.read(true)?;
    index.read_tree(&head_tree)?;
    index.write()?;
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
            if !write_resolved(workdir, path, blob.content())? {
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
