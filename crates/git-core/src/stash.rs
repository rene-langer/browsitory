use crate::worktree_guard::has_symlinked_ancestor;
use git2::{ErrorClass, ErrorCode, Repository, StashFlags};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StashError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("There are no changes to stash.")]
    NothingToStash,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("Resolve the current conflicts before applying a stash.")]
    UnresolvedConflicts,
    /// The index has conflicts: a stash can't record them (git and libgit2 refuse, "cannot
    /// create a tree from a not fully merged index"), and the path-limited stash would flatten
    /// them to HEAD's entry while the merge stays in progress.
    #[error("Resolve the current conflicts before stashing.")]
    ConflictsBeforeStash,
    #[error("Commit or unstage your staged changes before applying a stash.")]
    StagedChanges,
    #[error("Applying this stash would overwrite local changes to the same files. Commit, stash or discard them first.")]
    WouldOverwrite,
    #[error("The stash's staged changes conflict with the current files. Apply it without \"Restore staged changes\".")]
    IndexConflict,
    #[error("No stash with id {0}.")]
    NotAStash(String),
    #[error("'{0}' is beyond a symbolic link.")]
    BeyondSymlink(String),
    #[error("The stash list changed; refresh and try again.")]
    StashListChanged,
    /// A selected tracked path is a real directory now but a symlink, file or submodule in
    /// HEAD (or the index, with keep-index). Reverting it would make the checkout replace the
    /// directory and silently delete anything in it the stash does not hold (ignored files,
    /// untracked files when they are left out). git refuses this too.
    #[error("Can't stash '{path}': it was {was} and is now a directory. Commit or move its contents first.")]
    DirectoryReplacedNonDirectory { path: String, was: &'static str },
    /// A folder on the way to a selected tracked path was replaced by the file `parent`: the
    /// revert couldn't put the folder back (#154).
    #[error("Can't stash '{path}': '{parent}' is now a file, not a folder. Move or delete '{parent}' first.")]
    FolderIsNowAFile { path: String, parent: String },
    #[error("This stash can't be applied here because a directory was replaced by a symbolic link; use git stash apply from a terminal.")]
    SymlinkReplacedByDirectory,
    /// The stash holds an untracked file where the worktree now has a folder (the stash also
    /// holds that folder's deletion). libgit2 checks untracked files out first, so it can't
    /// apply it: it would leave the file as `d~Stashed changes` and report success (#186).
    #[error("This stash can't be applied here because a file replaced a folder; use git stash apply from a terminal.")]
    FileReplacedFolder,
    /// The stash holds an untracked file where the worktree now has a folder the stash knows
    /// nothing about (it appeared after stashing).
    #[error("The untracked file '{0}' in this stash would overwrite the folder '{0}'; move the folder first.")]
    UntrackedOverFolder(String),
    /// Stashing without untracked files resets `tracked`, which deletes the untracked file
    /// `untracked` in its way (a file where `tracked`'s folder was, or a file inside a folder
    /// that replaced the file `tracked`). git deletes it; Browsitory refuses (#186).
    #[error("Stashing would delete the untracked file '{untracked}', which is in the way of '{tracked}'. Turn on Include untracked files, or move '{untracked}' first.")]
    UntrackedInTheWay { untracked: String, tracked: String },
    /// Stash all resets `tracked`, which deletes the ignored file or folder `ignored` in its
    /// way, with or without untracked files (ignored files are never stashed). git deletes it;
    /// Browsitory refuses (#208).
    #[error("Stashing would delete the ignored '{ignored}', which is in the way of the tracked '{tracked}'. Move '{ignored}' first.")]
    IgnoredInTheWay { ignored: String, tracked: String },
    /// Stash all resets to the committed version of a path the index no longer has, which
    /// overwrites the ignored file or folder there: the stash doesn't record it (`git rm
    /// --cached`, then ignored, then edited). git records it; Browsitory refuses (#208).
    #[error("Stashing would overwrite the ignored '{0}' with its committed version. Commit the removal of '{0}' first, or move it.")]
    IgnoredOverwritten(String),
    /// Stash all resets `tracked`, which deletes the untracked repository in `path` (its
    /// history included), with or without untracked files (a nested repository is never
    /// stashed). git deletes it; Browsitory refuses (#211).
    #[error("Stashing would delete the repository in '{path}', which is in the way of the tracked '{tracked}'. Move '{path}' first.")]
    RepositoryInTheWay { path: String, tracked: String },
    /// The stash holds the untracked file `untracked` below the path `file`, which the index
    /// has as a file (the stash also holds the file's deletion, or the file was committed
    /// later). libgit2 merges the untracked files against the index, so it would leave
    /// `<file>~Updated upstream` and report success (#206).
    #[error("This stash can't be applied here: its untracked file '{untracked}' needs a folder where '{file}' is tracked. Commit the deletion of '{file}' first, then apply it; the stash is kept.")]
    UntrackedBelowTrackedFile { untracked: String, file: String },
    /// The index has the gitlink `0` that HEAD doesn't (`git add lib` with `lib` holding its own
    /// repository) and `.gitmodules` doesn't register it. libgit2's stash can't record it ("cannot
    /// create blob from '…/lib': it is a directory"); git can (#244).
    #[error("'{0}' is a repository inside this one. Add it as a submodule or remove it from the index first.")]
    EmbeddedRepository(String),
    /// The same for a submodule `.gitmodules` registers but HEAD doesn't have yet (#244).
    #[error("Can't stash the new submodule '{0}'. Commit it first, or remove it from the index.")]
    NewSubmodule(String),
    /// The same for a new gitlink whose folder is gone or holds no repository (#244).
    #[error("Can't stash '{0}': the index links it to another repository, and that link isn't committed yet. Remove '{0}' from the index first.")]
    NewGitlink(String),
    /// Stash file… on the new path `path` of the staged rename of `old`, while the worktree has
    /// a new file (or folder) at `old`: stashing the rename would also take it.
    #[error("Can't stash '{path}' on its own: it was renamed from '{old}', and '{old}' exists again. Stash '{old}' too, or move it first.")]
    RenameSourceRecreated { path: String, old: String },
    /// Where names fold: a stashed path differs only in case from `other` in HEAD or the
    /// index (a case-only rename). The stash couldn't revert it.
    #[error("Can't stash '{path}': it differs from '{other}' only in case. Commit the rename first, or undo it.")]
    CaseOnlyRename { path: String, other: String },
    /// Where names fold: HEAD or the index holds both `path` and `other`, which libgit2's
    /// ignore-case index takes for one entry (`A` and `a`, committed on a case-sensitive drive).
    /// The forced checkout after libgit2's stash dropped one of them from the index, and
    /// depending on directory order from the worktree too (#392). On macOS and Windows also two
    /// index names only the drive folds, when Stash file… picks one (#389).
    #[error("Can't stash: '{path}' and '{other}' differ only in case, and stashing would remove one of them. Rename one of the two in the repository first, or stash only other files.")]
    CasePair { path: String, other: String },
    #[error("{0}")]
    LinkedFolder(#[from] crate::LinkedFolder),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
    /// `core.ignorecase` can't be read, so the case checks can't run and libgit2's checkout
    /// could stop half-way (#225, #369). Nothing is written.
    #[error(transparent)]
    CaseConfig(#[from] crate::CaseConfigUnreadable),
    /// Windows: a stash apply (or pop) stopped at a file another program holds open; what its
    /// checkout had written was put back and the stash is kept (#321).
    #[error(transparent)]
    HeldOpen(#[from] crate::HeldOpen),
}

/// Refuses when `stash@{index}` is not the stash `expected` names: the caller's list is stale
/// (another tool stashed, dropped or popped), and acting on the index would hit another stash.
fn ensure_stash_at(
    repo: &Repository,
    index: usize,
    expected: Option<&str>,
) -> Result<(), StashError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let actual = repo
        .reflog("refs/stash")
        .ok()
        .and_then(|reflog| reflog.get(index).map(|entry| entry.id_new().to_string()));
    if actual.as_deref() == Some(expected) {
        Ok(())
    } else {
        Err(StashError::StashListChanged)
    }
}

impl From<crate::diff::DiffError> for StashError {
    fn from(error: crate::diff::DiffError) -> Self {
        match error {
            crate::diff::DiffError::Diff(error) => StashError::Git(error),
            // Compare-only variants; the stash code never calls a compare, but keep the message.
            other => StashError::Git(git2::Error::from_str(&other.to_string())),
        }
    }
}

/// Result of applying (or popping) a stash. `Conflicted` means the stash applied but left
/// conflicts in the index (and markers in the files); `repo.state()` stays `Clean`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StashApplyOutcome {
    Applied,
    Conflicted { files: Vec<String> },
}

/// The files a stash changed: `tracked` against the HEAD it was made on, `untracked` from its
/// untracked-files commit (parentless, so `diff::commit_diff` on `untracked_commit_id` shows each
/// as a whole-file add).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashFiles {
    pub tracked: Vec<String>,
    pub untracked: Vec<String>,
    pub untracked_commit_id: Option<String>,
}

/// What to stash, like `git stash push [-m <message>] [--keep-index] [-u] [-- <paths>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashOptions {
    /// `None`, empty or whitespace-only → libgit2's default "WIP on <branch>: …".
    pub message: Option<String>,
    /// Stash staged changes but also leave them staged in the index and worktree.
    pub keep_index: bool,
    pub include_untracked: bool,
    /// Literal file paths limiting the stash; empty stashes everything.
    pub paths: Vec<String>,
}

impl Default for StashOptions {
    fn default() -> Self {
        Self {
            message: None,
            keep_index: false,
            include_untracked: true,
            paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
    pub commit_id: String,
}

/// Saves a stash with `opts`.
///
/// Without paths this is libgit2's `stash_save2`. With paths Browsitory builds the stash itself
/// (`save_path_limited_stash`): libgit2 1.9.6's path-limited stash (`stash_save_ext` with a
/// pathspec) resets the *whole* index and worktree to HEAD afterwards, silently discarding
/// changes to files outside the paths, and git2 0.21's `StashSaveOptions` cannot pass a message.
pub fn save_stash(repo: &mut Repository, opts: &StashOptions) -> Result<(), StashError> {
    let message = opts
        .message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty());
    // First, strictly and on every OS (#369): the case checks below need the value, and the
    // path-limited route writes the stash commit and `refs/stash`, then removes the links it
    // stashed, before its checkout, which refuses an invalid value while parsing attributes and
    // ignore rules. Refusing then would leave the stash written and the worktree half reset.
    crate::worktree_guard::core_ignorecase(repo)?;
    // Before any route: the path-limited stash (Stash file…, and Stash all when it takes that
    // route) would record the conflicted file with its markers and put HEAD's entry back in
    // the index, leaving the merge in progress, so the next commit would drop the other side.
    let mut index = repo.index()?;
    // Reload if another tool (a terminal merge) changed it since the repository was opened.
    index.read(false)?;
    if index.has_conflicts() {
        return Err(StashError::ConflictsBeforeStash);
    }
    refuse_staged_repository(repo, &index, &opts.paths)?;
    if !opts.paths.is_empty() {
        return save_path_limited_stash(repo, opts, message, true);
    }
    // Before `entry_in_the_way`, which on macOS and Windows takes the deleted `Old` of a
    // case-only rename for an ignored file removed from the index (#370).
    refuse_staged_case_only_rename(repo, &index)?;
    // The reset after stashing would delete an untracked file (#186), an ignored file or folder
    // (#208) or a nested repository (#211) in the way of a tracked path. The path-limited stash
    // checks only its own paths.
    if let Some(InTheWay {
        path,
        tracked,
        kind,
    }) = entry_in_the_way(repo, opts.keep_index, opts.include_untracked)?
    {
        return Err(match kind {
            Kind::Untracked => StashError::UntrackedInTheWay {
                untracked: path,
                tracked,
            },
            Kind::Ignored => StashError::IgnoredInTheWay {
                ignored: path,
                tracked,
            },
            Kind::Repository => StashError::RepositoryInTheWay { path, tracked },
            Kind::IgnoredOverwritten => StashError::IgnoredOverwritten(path),
        });
    }
    // libgit2's stash records and resets in one call, and its forced reset writes through a
    // symlink that replaced a tracked file or folder (#114). With such a link, stash every
    // changed path through Browsitory's path-limited stash instead: a file link is stashed and
    // then replaced by the file, a folder link is refused (`BeyondSymlink`).
    // The same route keeps a nested repository or ignored files in an untracked folder, which
    // libgit2's reset with untracked files would delete (see `kept_entry_in_untracked_folder`),
    // and an edit to a file removed from the index, which libgit2 loses or records twice (see
    // `edit_removed_from_index`).
    if crate::worktree_guard::has_hazard(repo)?
        || (opts.include_untracked && kept_entry_in_untracked_folder(repo)?)
        || edit_removed_from_index(repo)?
    {
        let paths = changed_paths(repo, opts.include_untracked)?;
        if paths.is_empty() {
            return Err(StashError::NothingToStash);
        }
        let routed = StashOptions {
            paths,
            ..opts.clone()
        };
        return save_path_limited_stash(repo, &routed, message, false);
    }
    let signature = repo.signature()?;
    let mut flags = StashFlags::empty();
    if opts.keep_index {
        flags |= StashFlags::KEEP_INDEX;
    }
    if opts.include_untracked {
        flags |= StashFlags::INCLUDE_UNTRACKED;
    }
    // `stash_save2` (not `stash_save`) because it accepts a `None` message, letting libgit2
    // generate its own default ("WIP on <branch>: <short> <summary>").
    repo.stash_save2(&signature, message, Some(flags))
        .map_err(|error| {
            if error.code() == ErrorCode::NotFound && error.class() == ErrorClass::Stash {
                StashError::NothingToStash
            } else {
                StashError::Git(error)
            }
        })?;
    Ok(())
}

/// Refuses a stash of everything (`paths` empty), or of a selected path, when the index has a
/// gitlink there that HEAD doesn't: a submodule added but not committed yet (`NewSubmodule`,
/// registered in `.gitmodules`), `git add lib` with `lib` holding its own `.git`
/// (`EmbeddedRepository`), or else (the folder is gone or holds no `.git`) `NewGitlink`. With the
/// folder there, libgit2 1.9.6's stash fails with the raw "cannot create blob from '…/lib': it
/// is a directory" (it adds the folder by path to build its worktree tree); without it, the
/// stash succeeds but leaves the gitlink staged (`AD lib`), so Pop refuses with
/// `StagedChanges`. The path-limited stash, which skips submodules, said "There are no changes
/// to stash" for `lib`. Nothing is written. git stashes the gitlink and keeps the folder (#244).
/// A gitlink HEAD already has, changed or not, stashes fine, and the nested repository's
/// checkout is never touched (`a_committed_gitlink_stashes_and_leaves_the_nested_repository_alone`).
fn refuse_staged_repository(
    repo: &Repository,
    index: &git2::Index,
    paths: &[String],
) -> Result<(), StashError> {
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    let head = repo.head().and_then(|head| head.peel_to_tree()).ok();
    for entry in index.iter() {
        if entry.mode != 0o160000 || (entry.flags >> 12) & 0x3 != 0 {
            continue;
        }
        let path = String::from_utf8_lossy(&entry.path).into_owned();
        if !paths.is_empty() && !paths.contains(&path) {
            continue;
        }
        let in_head = head.as_ref().is_some_and(|head| {
            head.get_path(std::path::Path::new(&path))
                .is_ok_and(|entry| entry.filemode() == 0o160000)
        });
        if in_head {
            continue;
        }
        let is_repository = !has_symlinked_ancestor(workdir, &path)
            && std::fs::symlink_metadata(workdir.join(&path)).is_ok_and(|meta| meta.is_dir())
            && std::fs::symlink_metadata(workdir.join(&path).join(".git")).is_ok();
        return Err(if registered_submodule(workdir, &path) {
            StashError::NewSubmodule(path)
        } else if is_repository {
            StashError::EmbeddedRepository(path)
        } else {
            StashError::NewGitlink(path)
        });
    }
    Ok(())
}

/// True when the worktree's `.gitmodules` has a `submodule.<name>.path` of `path`.
fn registered_submodule(workdir: &std::path::Path, path: &str) -> bool {
    let file = workdir.join(".gitmodules");
    if !std::fs::symlink_metadata(&file).is_ok_and(|meta| meta.is_file()) {
        return false;
    }
    let Ok(config) = git2::Config::open(&file) else {
        return false;
    };
    let Ok(mut entries) = config.entries(Some(r"^submodule\..*\.path$")) else {
        return false;
    };
    while let Some(Ok(entry)) = entries.next() {
        if entry.value().is_ok_and(|value| value == path) {
            return true;
        }
    }
    false
}

/// Every changed file path (staged, unstaged and, with `include_untracked`, untracked; never
/// ignored files or untracked nested repositories), as `save_path_limited_stash` selects them.
fn changed_paths(repo: &Repository, include_untracked: bool) -> Result<Vec<String>, StashError> {
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(include_untracked)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true);
    let mut paths = Vec::new();
    for entry in repo.statuses(Some(&mut options))?.iter() {
        let Ok(path) = entry.path() else { continue };
        if entry.status().contains(git2::Status::IGNORED) || path.ends_with('/') {
            continue;
        }
        paths.push(path.to_string());
    }
    Ok(paths)
}

/// `git stash push [-m <message>] [--keep-index] [-u] -- <paths>` with `paths` as literal file
/// paths. Writes the same commit shape libgit2 and git do (worktree commit with parents HEAD,
/// index commit and, when untracked files were stashed, an untracked-files commit) and the same
/// messages, pushes it on `refs/stash`, and only then reverts the stashed paths, so a failure
/// part-way never loses anything: the changes are already in the stash.
///
/// - Worktree commit: HEAD's tree with the matched tracked paths' worktree contents.
/// - Index commit: HEAD's tree plus the selected paths' index entries (git stores the whole
///   index; see below).
/// - Untracked commit (parentless): only the matched untracked files.
///
/// Reverting touches only the matched paths: tracked ones go back to HEAD in index and worktree
/// (to the index version in the worktree with `keep_index`), untracked ones are deleted.
///
/// `picked`: the user chose `opts.paths` (Stash file…), rather than Stash all routing every
/// change here; see `with_rename_partners`.
fn save_path_limited_stash(
    repo: &Repository,
    opts: &StashOptions,
    message: Option<&str>,
    picked: bool,
) -> Result<(), StashError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| git2::Error::from_str("cannot stash in a bare repository"))?
        .to_path_buf();
    let signature = repo.signature()?;
    let head = repo.head()?;
    let base = head.peel_to_commit()?;
    let branch = if repo.head_detached()? {
        "(no branch)".to_string()
    } else {
        head.shorthand().unwrap_or("(no branch)").to_string()
    };
    let short_id = base
        .as_object()
        .short_id()?
        .as_str()
        .unwrap_or_default()
        .to_string();
    let base_message = format!(
        "{branch}: {short_id} {}",
        base.summary().ok().flatten().unwrap_or_default()
    );

    let paths = with_rename_partners(repo, &opts.paths, picked)?;
    let mut status_options = git2::StatusOptions::new();
    status_options
        .include_untracked(opts.include_untracked)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true)
        .disable_pathspec_match(true);
    for path in &paths {
        status_options.pathspec(path.as_str());
    }
    let mut tracked = Vec::new();
    let mut untracked = Vec::new();
    let mut deleted = std::collections::HashSet::new();
    for entry in repo.statuses(Some(&mut status_options))?.iter() {
        let status = entry.status();
        let Ok(path) = entry.path() else { continue };
        if status.contains(git2::Status::INDEX_DELETED) {
            // Ignored or not: with untracked files left out, status can't tell.
            deleted.insert(path.to_string());
        }
        if status.contains(git2::Status::IGNORED) || path.ends_with('/') {
            continue;
        }
        if status == git2::Status::WT_NEW {
            untracked.push(path.to_string());
        } else {
            tracked.push(path.to_string());
        }
    }
    // First: the staged deletion `Old` of a case-only rename finds a file at `Old` (the new
    // `old` on a drive that folds names, or an ignored `Old` beside it), which the check below
    // would take for an ignored file removed from the index.
    refuse_case_only_rename(repo, &base, &tracked)?;
    // A selected file removed from the index and then ignored: refused by the same rule as
    // Stash all (#259).
    if !deleted.is_empty() {
        let scan = WorktreeScan::new(repo)?;
        for (status, raw) in &scan.entries {
            if !deleted.contains(raw) || has_symlinked_ancestor(&workdir, raw) {
                continue;
            }
            if let Some(path) = scan.ignored_overwritten(Some(&workdir), *status, raw) {
                return Err(StashError::IgnoredOverwritten(path));
            }
        }
    }
    if tracked.is_empty() && untracked.is_empty() {
        return Err(StashError::NothingToStash);
    }
    // A tracked path that is now a real directory but not a tree in what the revert restores
    // would have its directory (and whatever the stash does not hold in it) replaced.
    for path in &tracked {
        let full = workdir.join(path);
        if !std::fs::symlink_metadata(&full).is_ok_and(|meta| meta.is_dir()) {
            continue;
        }
        let restored_mode = if opts.keep_index {
            repo.index()?
                .get_path(std::path::Path::new(path), 0)
                .map(|entry| entry.mode)
        } else {
            base.tree()?
                .get_path(std::path::Path::new(path))
                .ok()
                .map(|entry| entry.filemode() as u32)
        };
        let was = match restored_mode {
            Some(0o120000) => "a symbolic link",
            Some(0o160000) => "a submodule",
            Some(0o040000) | None => continue,
            Some(_) => "a file",
        };
        return Err(StashError::DirectoryReplacedNonDirectory {
            path: path.clone(),
            was,
        });
    }
    // The mirror case: `d/f` is tracked but `d` is now a file. The revert can't restore `d/f`
    // without deleting `d`; refuse before anything is recorded, unless `d` is stashed too (the
    // user picked `d`, or everything): then the revert removes it first, as git does (#186).
    for path in &tracked {
        if let Some(parent) = crate::worktree_guard::file_in_place_of_folder(&workdir, path)? {
            if untracked.contains(&parent) {
                continue;
            }
            if !opts.include_untracked && is_untracked(repo, &parent)? {
                return Err(StashError::UntrackedInTheWay {
                    untracked: parent,
                    tracked: path.clone(),
                });
            }
            return Err(StashError::FolderIsNowAFile {
                path: path.clone(),
                parent,
            });
        }
    }
    // Reading or reverting `d/f` while `d` is a symlink would read and write *through* it,
    // possibly outside the repository. Refuse before anything is written, like git does.
    for path in paths.iter().chain(&tracked).chain(&untracked) {
        if has_symlinked_ancestor(&workdir, path) {
            return Err(StashError::BeyondSymlink(path.clone()));
        }
    }

    let mut index = repo.index()?;
    let base_tree = base.tree()?;
    // Index commit: HEAD's tree plus only the selected paths' index entries. (git stores the
    // whole index; then "Restore staged changes" would also re-stage, and `git stash apply
    // --index` would refuse over, files this stash never took.)
    let mut staged_overlay = git2::Index::new()?;
    staged_overlay.read_tree(&base_tree)?;
    for path in &tracked {
        match index.get_path(std::path::Path::new(path), 0) {
            Some(entry) => staged_overlay.add(&entry)?,
            None => {
                let _ = staged_overlay.remove_path(std::path::Path::new(path));
            }
        }
    }
    let index_tree = repo.find_tree(staged_overlay.write_tree_to(repo)?)?;
    let index_commit = repo.find_commit(repo.commit(
        None,
        &signature,
        &signature,
        &format!("index on {base_message}\n"),
        &index_tree,
        &[&base],
    )?)?;

    let config = crate::undo::worktree_config(repo).map_err(undo_error)?;
    let untracked_commit = if untracked.is_empty() {
        None
    } else {
        let mut overlay = git2::Index::new()?;
        for path in &untracked {
            if let Some((id, mode)) = worktree_blob(repo, &workdir, path, &config, None)? {
                overlay.add(&crate::undo::index_entry(path, id, mode))?;
            }
        }
        let tree = repo.find_tree(overlay.write_tree_to(repo)?)?;
        Some(repo.find_commit(repo.commit(
            None,
            &signature,
            &signature,
            &format!("untracked files on {base_message}\n"),
            &tree,
            &[],
        )?)?)
    };

    let mut overlay = git2::Index::new()?;
    overlay.read_tree(&base_tree)?;
    for path in &tracked {
        let previous_mode = overlay
            .get_path(std::path::Path::new(path), 0)
            .map(|entry| entry.mode);
        match worktree_blob(repo, &workdir, path, &config, previous_mode)? {
            Some((id, mode)) => overlay.add(&crate::undo::index_entry(path, id, mode))?,
            None => {
                let _ = overlay.remove_path(std::path::Path::new(path));
            }
        }
    }
    let worktree_tree = repo.find_tree(overlay.write_tree_to(repo)?)?;
    let reflog_message = match message {
        Some(message) => format!("On {branch}: {message}"),
        None => format!("WIP on {base_message}"),
    };
    let mut parents = vec![&base, &index_commit];
    if let Some(commit) = &untracked_commit {
        parents.push(commit);
    }
    let stash_id = repo.commit(
        None,
        &signature,
        &signature,
        &format!("{reflog_message}\n"),
        &worktree_tree,
        &parents,
    )?;
    // Without an existing log libgit2 would not write a reflog for `refs/stash` (it is not
    // under refs/heads), and the reflog *is* the stash list.
    repo.reference_ensure_log("refs/stash")?;
    repo.reference("refs/stash", stash_id, true, &reflog_message)?;

    // The stash is saved; now revert exactly the stashed paths. Every filesystem change below
    // re-checks, at that moment, that no directory on the way is a symlink and skips the path if
    // one is: removing or writing `d/f` with `d` a symlink would act on the link's target,
    // possibly outside the repository. Skipping only leaves a change in place; it is in the
    // stash either way.
    //
    // 1. Untracked files first, while the directories they live in are still real: reverting a
    //    tracked path can turn a directory back into HEAD's symlink (a tracked symlink `l` that
    //    was replaced by a directory holding untracked `l/f`), after which `l/f` would resolve
    //    into the link's target.
    for path in &untracked {
        // Removes the file and the folders the stash emptied, like git's clean -d; never a
        // symlink's target, never through one, and through held folders so a swap meanwhile
        // can't redirect it (#149).
        crate::worktree_guard::remove_and_prune_beneath(&workdir, std::path::Path::new(path))?;
    }
    // 2. A tracked file replaced by a symlink (typechange) must be removed before its checkout:
    //    a forced checkout writes the file's content *through* the symlink, overwriting its
    //    target, which may be another file with unsaved edits or a file outside the repository.
    //    The symlink itself is in the stash.
    let mut checkout_paths = Vec::new();
    for path in &tracked {
        if has_symlinked_ancestor(&workdir, path) {
            continue;
        }
        crate::worktree_guard::clear_hazards(
            &workdir,
            &[crate::worktree_guard::Hazard::Leaf(path.into())],
        )?;
        checkout_paths.push(path.as_str());
    }
    // 3. Tracked paths back to HEAD (or to the index with `keep_index`).
    if !checkout_paths.is_empty() {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().disable_pathspec_match(true);
        for path in &checkout_paths {
            checkout.path(*path);
        }
        if opts.keep_index {
            index.read(true)?;
            repo.checkout_index(Some(&mut index), Some(&mut checkout))?;
        } else {
            repo.checkout_tree(base_tree.as_object(), Some(&mut checkout))?;
            // The checkout updates the index only for the files it writes: a path removed from
            // the index whose worktree file already matches HEAD (`git rm --cached f`, or the
            // unedited `d/g` of `git rm -r --cached d`) would stay removed, and Pop would then
            // refuse with `StagedChanges` (#248). Put HEAD's entry back for every reverted
            // path. (Not `reset_default`: it takes pathspecs, so a name like `[ab]` would also
            // unstage `a` and `b`.) Only files and links: HEAD may have a folder (or a
            // submodule) at a reverted path whose index entry was a staged file.
            index.read(true)?;
            let mut changed = false;
            for path in &checkout_paths {
                let Ok(entry) = base_tree.get_path(std::path::Path::new(path)) else {
                    continue;
                };
                let mode = entry.filemode() as u32;
                if !matches!(mode, 0o100644 | 0o100755 | 0o120000) {
                    continue;
                }
                let current = index.get_path(std::path::Path::new(path), 0);
                if current
                    .as_ref()
                    .is_some_and(|current| current.id == entry.id() && current.mode == mode)
                {
                    continue;
                }
                let mut restored = crate::undo::index_entry(path, entry.id(), mode);
                // Keep assume-unchanged and skip-worktree of an entry it replaces; drop
                // intent-to-add (HEAD's blob is real content).
                if let Some(current) = current {
                    const ASSUME_VALID: u16 = 0x8000;
                    const EXTENDED: u16 = 0x4000;
                    const SKIP_WORKTREE: u16 = 1 << 14;
                    restored.flags |= current.flags & ASSUME_VALID;
                    restored.flags_extended = current.flags_extended & SKIP_WORKTREE;
                    if restored.flags_extended != 0 {
                        restored.flags |= EXTENDED;
                    }
                }
                index.add(&restored)?;
                changed = true;
            }
            if changed {
                index.write()?;
            }
        }
    }
    Ok(())
}

/// `paths` plus the other side of every staged rename one of them names (or a folder of them
/// holds), so Stash file… on a staged rename's new path also stashes the old path's deletion,
/// instead of leaving it staged (#152). Renames are detected between HEAD and the index as
/// `status::status` detects them, so they match what the file list shows. (git's `stash push
/// -- new` leaves the deletion staged, and `-- old new` fails: `old` matches no file it knows.)
///
/// When only the new path is selected and the worktree has something at the old path again (a
/// new file the user made there), this refuses (`RenameSourceRecreated`): stashing the deletion
/// would record that file and write the committed version over it, though the user picked only
/// the rename, and even with untracked files left out. Selecting the old path stashes it too.
/// `picked_alone` is false for Stash all on the path-limited route (`save_stash` selects every
/// change): nothing was picked on its own there, so the new file at the old path is stashed
/// like any edit to a file removed from the index (#247) and Pop brings it back.
///
/// Not paired: submodule renames (left out like everywhere in the path-limited stash; the new
/// gitlink is refused first, see `refuse_staged_repository`) and paths that aren't UTF-8 (status
/// paths that aren't UTF-8 are skipped everywhere here, and the frontend can't send them).
fn with_rename_partners(
    repo: &Repository,
    paths: &[String],
    picked_alone: bool,
) -> Result<Vec<String>, StashError> {
    let workdir = repo.workdir();
    let folds_case = crate::worktree_guard::try_folds_case(repo)?;
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(false)
        .include_ignored(false)
        .exclude_submodules(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(false);
    let selects = |path: &str| {
        paths.iter().any(|picked| {
            path.strip_prefix(picked.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        })
    };
    let mut all = paths.to_vec();
    for entry in repo.statuses(Some(&mut options))?.iter() {
        if !entry.status().contains(git2::Status::INDEX_RENAMED) {
            continue;
        }
        let Some(delta) = entry.head_to_index() else {
            continue;
        };
        let (Some(old), Some(new)) = (
            delta.old_file().path().and_then(std::path::Path::to_str),
            delta.new_file().path().and_then(std::path::Path::to_str),
        ) else {
            continue;
        };
        for (picked, partner) in [(new, old), (old, new)] {
            if !selects(picked) || selects(partner) || all.iter().any(|path| path == partner) {
                continue;
            }
            // A case-only rename on a drive that folds names finds the new file at the old
            // path; `refuse_case_only_rename` refuses it later.
            if picked_alone
                && partner == old
                && !(folds_case && stash_fold(old.as_bytes()) == stash_fold(new.as_bytes()))
                && workdir
                    .is_some_and(|workdir| std::fs::symlink_metadata(workdir.join(old)).is_ok())
            {
                return Err(StashError::RenameSourceRecreated {
                    path: new.to_string(),
                    old: old.to_string(),
                });
            }
            all.push(partner.to_string());
        }
    }
    Ok(all)
}

/// The key under which libgit2's ignore-case index (`core.ignorecase`) takes two paths for one
/// entry: it compares with `strcasecmp`/`git__strcasecmp` and hashes with `git__tolower`
/// (index.c, index_map.c), byte by byte with ASCII letters folded only (the C locale;
/// `git__tolower` on Windows). So `README`/`readme` are one entry, but `Straße`/`Strasse` or
/// NFC/NFD spellings are two, unlike `worktree_guard::fold_name`, which follows the file systems.
fn icase_index_key(path: &[u8]) -> Vec<u8> {
    path.to_ascii_lowercase()
}

/// The key under which two tracked paths are one name to a stash where names fold
/// (`try_folds_case`): libgit2's ignore-case index (`icase_index_key`, ASCII only) and, on
/// macOS and Windows, the drive (#389). APFS takes `Straße`/`Strasse`, `Café`/`CAFÉ` and NFC/NFD
/// spellings for one file (`worktree_guard::fold_name`); NTFS only upper-cases letter by letter
/// (`Café`/`CAFÉ`, not `ß`/`ss` and not NFC/NFD; `ntfs_fold`). On Linux with `core.ignorecase`
/// the drive keeps such names apart, and so does libgit2's index, so only ASCII case counts.
fn stash_fold(path: &[u8]) -> Vec<u8> {
    if cfg!(target_os = "macos") {
        crate::worktree_guard::fold_name(path)
    } else if cfg!(windows) {
        ntfs_fold(path)
    } else {
        icase_index_key(path)
    }
}

/// `path` as NTFS compares names: each letter mapped through its simple upper-case form (the
/// volume's upcase table), so `é`/`É` meet but `ß` (whose upper case is the two letters `SS`) and
/// a decomposed `é` stay apart. Not valid UTF-8: ASCII upper case only.
fn ntfs_fold(path: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(path) else {
        return path.to_ascii_uppercase();
    };
    text.chars()
        .map(|c| {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(single), None) => single,
                _ => c,
            }
        })
        .collect::<String>()
        .into_bytes()
}

/// Where names fold (`try_folds_case`), refuses when a stashed tracked path differs only in
/// case from a path HEAD or the index has (a case-only rename, `git mv Old old`). libgit2's index
/// then finds one as the other (`icase_index_key`), so the stash's index tree would hold both
/// names and the revert would find nothing to change: the stash is written but the rename
/// stays. Stash all checks the whole index instead (`refuse_staged_case_only_rename`).
///
/// On macOS and Windows "only in case" is what the drive folds (`stash_fold`, #389): there
/// `Straße` → `Strasse` (macOS) or `Café` → `CAFÉ` (both) are two entries to libgit2 but one
/// file, and Stash file… was refused with a misleading `IgnoredOverwritten` or
/// `RenameSourceRecreated`.
///
/// When HEAD or the index alone holds both names (`A` and `a`, committed on a case-sensitive
/// drive), it is a case pair, not a rename, and refused as `CasePair` (#392): the revert's
/// ignore-case checkout dropped one of the pair from the index, or deleted its file, and Pop
/// then refused with `StagedChanges`. A pair the stash doesn't touch stays as it is. The same
/// holds for two index names only the drive folds (`git add -A` after `mv Straße Strasse` on
/// macOS stages `Strasse` as an addition and keeps `Straße`, #389): stashing one deleted the
/// shared file until Pop, or wrote a stash that changed nothing and Pop refused.
fn refuse_case_only_rename(
    repo: &Repository,
    base: &git2::Commit,
    tracked: &[String],
) -> Result<(), StashError> {
    if !crate::worktree_guard::try_folds_case(repo)? {
        return Ok(());
    }
    let fold = stash_fold;
    // The paths of HEAD (`.0`) and of the index (`.1`) under each key.
    type Variants = (
        std::collections::BTreeSet<Vec<u8>>,
        std::collections::BTreeSet<Vec<u8>>,
    );
    let mut known = std::collections::HashMap::<Vec<u8>, Variants>::new();
    base.tree()?
        .walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() != Some(git2::ObjectType::Tree) {
                let path = [root.as_bytes(), entry.name_bytes()].concat();
                known.entry(fold(&path)).or_default().0.insert(path);
            }
            git2::TreeWalkResult::Ok
        })?;
    for entry in repo.index()?.iter() {
        known
            .entry(fold(&entry.path))
            .or_default()
            .1
            .insert(entry.path);
    }
    for path in tracked {
        let Some((in_head, in_index)) = known.get(&fold(path.as_bytes())) else {
            continue;
        };
        let Some(other) = in_head
            .iter()
            .chain(in_index)
            .find(|other| other.as_slice() != path.as_bytes())
        else {
            continue;
        };
        let other = String::from_utf8_lossy(other).into_owned();
        // HEAD or the index alone holds two of the names: a case pair, not a rename (#392).
        return Err(if in_head.len() > 1 || in_index.len() > 1 {
            StashError::CasePair {
                path: path.clone(),
                other,
            }
        } else {
            StashError::CaseOnlyRename {
                path: path.clone(),
                other,
            }
        });
    }
    Ok(())
}

/// Stash all's form of `refuse_case_only_rename` (#370): where names fold, refuses when the
/// index has a path HEAD doesn't while HEAD has a path differing from it only in case
/// (`stash_fold`) that the index doesn't (`git mv Old old`, or `git mv Dir dir` for `Dir/f`;
/// on macOS also `mv Straße Strasse` staged by libgit2, #389). libgit2's stash found
/// one name as the other: on Linux with `core.ignorecase` it wrote a stash whose index tree held
/// both names and left the rename in place; on macOS and Windows the deleted `Old` found the
/// file `old` and was refused as an ignored file removed from the index. git stashes the rename
/// and puts `Old` back, which libgit2's stash can't. Runs before the checks that look at the
/// worktree; nothing is written.
///
/// HEAD or the index holding a case pair (`A` and `a`, committed on a case-sensitive drive) is
/// no rename, but refused too (`CasePair`, #392): libgit2's forced checkout after the stash
/// (`reset_index_and_workdir`) keeps only one of them in its ignore-case index, so the other
/// became a staged deletion, and depending on the directory's read order its file was deleted
/// as well. Only whole paths count, as in `icase_index_key` (`D/f` and `d/g` are kept).
fn refuse_staged_case_only_rename(
    repo: &Repository,
    index: &git2::Index,
) -> Result<(), StashError> {
    if !crate::worktree_guard::try_folds_case(repo)? {
        return Ok(());
    }
    let Ok(head) = repo.head().and_then(|head| head.peel_to_tree()) else {
        return Ok(()); // Unborn: nothing to rename from.
    };
    // HEAD's paths, by the key a rename compares with (`stash_fold`).
    let mut in_head: std::collections::HashMap<Vec<u8>, Vec<Vec<u8>>> =
        std::collections::HashMap::new();
    head.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() != Some(git2::ObjectType::Tree) {
            let path = [root.as_bytes(), entry.name_bytes()].concat();
            in_head.entry(stash_fold(&path)).or_default().push(path);
        }
        git2::TreeWalkResult::Ok
    })?;
    let paths: Vec<Vec<u8>> = index.iter().map(|entry| entry.path).collect();
    let in_index: std::collections::HashSet<&[u8]> = paths.iter().map(Vec::as_slice).collect();
    for path in &paths {
        let Some(variants) = in_head.get(&stash_fold(path)) else {
            continue;
        };
        if variants.contains(path) {
            continue;
        }
        if let Some(other) = variants
            .iter()
            .find(|other| !in_index.contains(other.as_slice()))
        {
            return Err(StashError::CaseOnlyRename {
                path: String::from_utf8_lossy(path).into_owned(),
                other: String::from_utf8_lossy(other).into_owned(),
            });
        }
    }
    // No rename, but HEAD or the index holds two paths libgit2's index takes for one (#392).
    // Only ASCII case counts here: a pair only the drive folds (`Straße` and `Strasse` in the
    // index on macOS) stashes fine, since libgit2's checkout removes before it writes (#389).
    let mut names =
        std::collections::BTreeMap::<Vec<u8>, std::collections::BTreeSet<Vec<u8>>>::new();
    for path in in_head.into_values().flatten().chain(paths) {
        names
            .entry(icase_index_key(&path))
            .or_default()
            .insert(path);
    }
    if let Some(pair) = names.into_values().find(|names| names.len() > 1) {
        let mut pair = pair
            .into_iter()
            .map(|path| String::from_utf8_lossy(&path).into_owned());
        if let (Some(path), Some(other)) = (pair.next(), pair.next()) {
            return Err(StashError::CasePair { path, other });
        }
    }
    Ok(())
}

/// True when the worktree file `path` is untracked: not in the index and not ignored.
/// (`status_file` can't tell: the path is ambiguous while the index has entries below it.)
fn is_untracked(repo: &Repository, path: &str) -> Result<bool, StashError> {
    let path = std::path::Path::new(path);
    Ok(repo.index()?.get_path(path, 0).is_none() && !repo.is_path_ignored(path)?)
}

/// True when an untracked repository (a folder with a `.git`) or an ignored file or folder sits
/// below an untracked folder: a proper ancestor folder with no path in the index or in HEAD.
/// libgit2's reset after a stash with untracked files (`GIT_CHECKOUT_REMOVE_UNTRACKED`) removes
/// such an ancestor whole (`checkout_action_wd_only`), nested repository, history and ignored
/// files included: `wd_item_is_removable` (checkout.c) only protects a folder that itself holds
/// a `.git`. git's `stash -u` keeps them (`git clean -d` skips nested repositories and ignored
/// files). Such a stash takes the path-limited route, which deletes only the untracked files it
/// stashed; its own refusals then apply (a folder that replaced a tracked file is
/// `DirectoryReplacedNonDirectory`). An untracked folder holding only ignored entries doesn't
/// count: libgit2 sees it as ignored and leaves it.
fn kept_entry_in_untracked_folder(repo: &Repository) -> Result<bool, StashError> {
    let Some(workdir) = repo.workdir() else {
        return Ok(false);
    };
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(true)
        .recurse_ignored_dirs(false)
        .exclude_submodules(true);
    let mut kept = Vec::new();
    // Every untracked (not ignored) path, nested repositories included.
    let mut untracked = Vec::new();
    for entry in repo.statuses(Some(&mut options))?.iter() {
        let Ok(raw) = entry.path() else { continue };
        let path = raw.trim_end_matches('/');
        let is_kept = match entry.status() {
            git2::Status::IGNORED => true,
            git2::Status::WT_NEW => {
                untracked.push(path.to_string());
                raw.ends_with('/') && workdir.join(path).join(".git").exists()
            }
            _ => false,
        };
        if is_kept && path.contains('/') {
            kept.push(path.to_string());
        }
    }
    if kept.is_empty() {
        return Ok(false);
    }
    // The folders holding a path of the index, and of HEAD. A folder missing from either counts
    // as untracked: taking the path-limited route when it wasn't needed costs nothing.
    let folders_of = |paths: &mut dyn Iterator<Item = String>| {
        let mut folders = std::collections::HashSet::new();
        for path in paths {
            for (slash, _) in path.match_indices('/') {
                folders.insert(path[..slash].to_string());
            }
        }
        folders
    };
    let index = repo.index()?;
    let in_index = folders_of(
        &mut index
            .iter()
            .map(|entry| String::from_utf8_lossy(&entry.path).into_owned()),
    );
    let mut head_paths = Vec::new();
    if let Ok(head) = repo.head().and_then(|head| head.peel_to_tree()) {
        head.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() != Some(git2::ObjectType::Tree) {
                if let Ok(name) = entry.name() {
                    head_paths.push(format!("{root}{name}"));
                }
            }
            git2::TreeWalkResult::Ok
        })?;
    }
    let in_head = folders_of(&mut head_paths.into_iter());
    // libgit2 removes the outermost untracked folder only when it holds an untracked path (one
    // holding only ignored files counts as ignored and stays).
    Ok(kept.iter().any(|path| {
        let Some(folder) = path
            .match_indices('/')
            .map(|(slash, _)| &path[..slash])
            .find(|folder| !in_index.contains(*folder) || !in_head.contains(*folder))
        else {
            return false;
        };
        untracked.iter().any(|untracked| {
            untracked
                .strip_prefix(folder)
                .is_some_and(|rest| rest.starts_with('/'))
        })
    }))
}

/// True when a file the index no longer has is still in the worktree and not ignored (`git rm
/// --cached`, often followed by an edit; status `INDEX_DELETED | WT_NEW`). libgit2 1.9.6's whole
/// stash mishandles it (#247):
///
/// - Its worktree tree comes from an index-to-worktree diff with untracked files but without
///   `GIT_DIFF_RECURSE_UNTRACKED_DIRS` (stash.c `build_workdir_tree`, l.391). For `d/f` in a
///   folder the index no longer has, that diff reports `d/` as untracked, and
///   `stash_delta_merge` (l.360) only joins a deletion and an untracked entry of the same path,
///   so `d/f` is recorded as deleted: the edit is lost (Pop deletes the file; with keep-index
///   the reset deletes it at once).
/// - With untracked files, `build_untracked_tree` (l.292) does recurse, so the edit lands in
///   the untracked commit (and a top-level `f` in both trees). Apply then merges the untracked
///   file against the restored `f`: conflict markers and "would overwrite".
///
/// Such a stash takes the path-limited route instead, which records the file once, as a
/// tracked change (its edit in the worktree tree, the index commit without it), then puts
/// HEAD's version back in the worktree and HEAD's entry back in the index (with keep-index it
/// deletes the file, as git does). Pop then brings the edit back, and "Restore staged changes"
/// the removal from the index too. git records it in the worktree tree as well (and, with
/// `-u`, also in the untracked commit, which its own pop can't apply).
///
/// Cost: a HEAD-to-index diff (no worktree scan), then one no-follow stat and one ignore check
/// per staged deletion. A folder at the path, or a path beyond a symlinked folder, is left to
/// the other checks.
fn edit_removed_from_index(repo: &Repository) -> Result<bool, StashError> {
    let Some(workdir) = repo.workdir() else {
        return Ok(false);
    };
    let Ok(head) = repo.head().and_then(|head| head.peel_to_tree()) else {
        return Ok(false);
    };
    let index = repo.index()?;
    let diff = repo.diff_tree_to_index(Some(&head), Some(&index), None)?;
    for delta in diff.deltas() {
        if delta.status() != git2::Delta::Deleted {
            continue;
        }
        let Some(path) = delta.old_file().path() else {
            continue;
        };
        let Some(name) = path.to_str() else { continue };
        if has_symlinked_ancestor(workdir, name) {
            continue;
        }
        if std::fs::symlink_metadata(workdir.join(path)).is_ok_and(|meta| !meta.is_dir())
            && !repo.is_path_ignored(path)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The whole-worktree status Stash all checks before stashing (`entry_in_the_way`), and Stash
/// file… for a staged deletion: untracked and ignored entries included, ignored folders not
/// recursed, submodules left out.
struct WorktreeScan {
    entries: Vec<(git2::Status, String)>,
}

impl WorktreeScan {
    fn new(repo: &Repository) -> Result<Self, StashError> {
        let mut options = git2::StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(true)
            .recurse_ignored_dirs(false)
            .exclude_submodules(true);
        let entries = repo
            .statuses(Some(&mut options))?
            .iter()
            .filter_map(|entry| Some((entry.status(), entry.path().ok()?.to_string())))
            .collect();
        Ok(Self { entries })
    }

    /// For the entry `raw` with `status`: a file the index no longer has but the worktree still
    /// does, and that status doesn't report as `WT_NEW` (`git rm --cached`, then ignored, alone
    /// or as part of an ignored folder). A stash doesn't record it (libgit2's skips ignored
    /// files; the path-limited stash would record it but put HEAD's entry back in the index, so
    /// the file would be tracked again after a pop, #259), and the revert writes the committed
    /// version over it (with keep-index it deletes it): `IgnoredOverwritten`, naming the folder
    /// ignored as a whole that holds it, or else `raw`. Status reports such a folder's files one
    /// by one only when the index still had them (`INDEX_DELETED`), without the `IGNORED` flag.
    fn ignored_overwritten(
        &self,
        workdir: Option<&std::path::Path>,
        status: git2::Status,
        raw: &str,
    ) -> Option<String> {
        if !status.contains(git2::Status::INDEX_DELETED)
            || status.contains(git2::Status::WT_NEW)
            || !workdir.is_some_and(|workdir| {
                std::fs::symlink_metadata(workdir.join(raw)).is_ok_and(|meta| !meta.is_dir())
            })
        {
            return None;
        }
        let folder = self.entries.iter().find_map(|(status, folder)| {
            let folder = folder.strip_suffix('/')?;
            (status.contains(git2::Status::IGNORED)
                && raw
                    .strip_prefix(folder)
                    .is_some_and(|rest| rest.starts_with('/')))
            .then_some(folder)
        });
        Some(folder.unwrap_or(raw).to_string())
    }
}

/// What `entry_in_the_way` found: the worktree entry `path` the reset would delete, the
/// restored path `tracked` it is in the way of, and what kind of entry it is.
struct InTheWay {
    path: String,
    tracked: String,
    kind: Kind,
}

enum Kind {
    /// An untracked file (only reported when untracked files are left out of the stash).
    Untracked,
    /// An ignored file or folder: never stashed.
    Ignored,
    /// An untracked folder holding a `.git`: never stashed.
    Repository,
    /// An ignored file (or one in the ignored folder `path`) the index no longer has, at a
    /// restored file's path: the reset overwrites it.
    IgnoredOverwritten,
}

/// Before a whole stash: the first worktree entry the reset would delete because it is in the
/// way of a restored path, with that path. libgit2's forced checkout (`GIT_CHECKOUT_FORCE`)
/// removes a non-folder where the restored tree has a folder (`checkout_action_with_wd_blocker`)
/// and a folder where it has a file (`checkout_action_with_wd_dir`, removed with everything in
/// it, a nested `.git` included), ignored or not. (`GIT_CHECKOUT_REMOVE_UNTRACKED` removing a
/// whole untracked folder is left to `kept_entry_in_untracked_folder`.) So in the way are:
///
/// - an ignored file or folder, or an untracked nested repository, in either mode;
/// - an untracked file only when `include_untracked` is off (otherwise it is in the stash);
/// - a non-folder at a restored file's path that the index no longer has and status doesn't
///   report as `WT_NEW` (`git rm --cached`, then ignored, alone or as part of an ignored
///   folder): the stash doesn't record it and the reset overwrites it (`IgnoredOverwritten`).
///
/// The restored tree is HEAD's; with `keep_index` the index's too: libgit2 resets once to the
/// index commit, but with HEAD as the checkout baseline, so it still removes what is in the way
/// of a path HEAD has and the index deletes (git does too). Where names fold (`try_folds_case`:
/// the repository sets `core.ignorecase`, or always on macOS and Windows) paths compare as
/// `fold_name` folds them, like the apply/pop check (`refuse_untracked_below_tracked_file`) and
/// the case-collision walker: `docs` is in the way of `Docs/f`, `STRASSE` of `Straße/f` and an
/// NFD `Café` of an NFC `Café/f` (#337). Untracked symlinks are left to the hazard route
/// (`BeyondSymlink`).
fn entry_in_the_way(
    repo: &Repository,
    keep_index: bool,
    include_untracked: bool,
) -> Result<Option<InTheWay>, StashError> {
    let ignore_case = crate::worktree_guard::try_folds_case(repo)?;
    let fold = |path: &str| {
        if ignore_case {
            // `path` is UTF-8 and `fold_name` keeps it so; the lossy branch never runs.
            String::from_utf8(crate::worktree_guard::fold_name(path.as_bytes()))
                .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned())
        } else {
            path.to_string()
        }
    };
    // Folded path → the restored path: every file, and every folder with its first file.
    let mut files = std::collections::HashMap::new();
    let mut folders = std::collections::HashMap::new();
    let mut add = |path: &str| {
        let folded = fold(path);
        for (slash, _) in folded.match_indices('/') {
            folders
                .entry(folded[..slash].to_string())
                .or_insert_with(|| path.to_string());
        }
        files.insert(folded, path.to_string());
    };
    if let Ok(head) = repo.head().and_then(|head| head.peel_to_tree()) {
        head.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() != Some(git2::ObjectType::Tree) {
                if let Ok(name) = entry.name() {
                    add(&format!("{root}{name}"));
                }
            }
            git2::TreeWalkResult::Ok
        })?;
    }
    if keep_index {
        for entry in repo.index()?.iter() {
            if (entry.flags >> 12) & 0x3 == 0 {
                add(&String::from_utf8_lossy(&entry.path));
            }
        }
    }
    let workdir = repo.workdir().map(std::path::Path::to_path_buf);
    let scan = WorktreeScan::new(repo)?;
    for (status, raw) in &scan.entries {
        let status = *status;
        if files.contains_key(&fold(raw)) {
            if let Some(path) = scan.ignored_overwritten(workdir.as_deref(), status, raw) {
                return Ok(Some(InTheWay {
                    path,
                    tracked: raw.clone(),
                    kind: Kind::IgnoredOverwritten,
                }));
            }
        }
        let ignored = status.contains(git2::Status::IGNORED);
        if status != git2::Status::WT_NEW && !ignored {
            continue;
        }
        let raw = raw.as_str();
        // Untracked folders are only reported whole when they hold a repository (empty ones
        // aren't reported); ignored ones whenever the folder is ignored as a whole.
        let is_dir = raw.ends_with('/');
        let path = raw.trim_end_matches('/');
        let kind = if ignored {
            Kind::Ignored
        } else if is_dir
            && workdir
                .as_ref()
                .is_some_and(|workdir| workdir.join(path).join(".git").exists())
        {
            Kind::Repository
        } else if include_untracked {
            continue;
        } else {
            if workdir.as_ref().is_some_and(|workdir| {
                std::fs::symlink_metadata(workdir.join(path)).is_ok_and(|meta| meta.is_symlink())
            }) {
                continue;
            }
            Kind::Untracked
        };
        let folded = fold(path);
        let in_the_way_of = if is_dir {
            // A folder where the restored tree has a file: the reset replaces the folder.
            files.get(&folded)
        } else {
            // A file where the restored tree has a folder: the reset checks out its files.
            folders.get(&folded)
        }
        .or_else(|| {
            // Anything below a path the restored tree has as a file: the reset replaces the
            // folder that holds it.
            folded
                .match_indices('/')
                .find_map(|(slash, _)| files.get(&folded[..slash]))
        });
        if let Some(tracked) = in_the_way_of {
            return Ok(Some(InTheWay {
                path: path.to_string(),
                tracked: tracked.clone(),
                kind,
            }));
        }
    }
    Ok(None)
}

/// Blob id and mode of a worktree file, with clean filters applied (as `git add` would);
/// `None` when the file is gone.
fn worktree_blob(
    repo: &Repository,
    workdir: &std::path::Path,
    path: &str,
    config: &crate::undo::WorktreeConfig,
    previous_mode: Option<u32>,
) -> Result<Option<(git2::Oid, u32)>, StashError> {
    let Some((bytes, mode)) =
        crate::undo::read_worktree_file(workdir, path, config, previous_mode)?
    else {
        return Ok(None);
    };
    let id = if mode == crate::undo::MODE_LINK {
        repo.blob(&bytes)?
    } else {
        let mut writer = repo.blob_writer(Some(std::path::Path::new(path)))?;
        std::io::Write::write_all(&mut writer, &bytes)?;
        writer.commit()?
    };
    Ok(Some((id, mode)))
}

fn undo_error(error: crate::undo::UndoError) -> StashError {
    match error {
        crate::undo::UndoError::Git(error) => StashError::Git(error),
        crate::undo::UndoError::Io(error) => StashError::Io(error),
        other => StashError::Git(git2::Error::from_str(&other.to_string())),
    }
}

pub fn list_stashes(repo: &mut Repository) -> Result<Vec<StashEntry>, StashError> {
    let mut entries = Vec::new();
    // `stash_foreach` requires `&mut self` despite being read-only — a libgit2 API constraint,
    // not a design choice (see the design spec's "&mut Repository wrinkle" note).
    repo.stash_foreach(|index, message, oid| {
        entries.push(StashEntry {
            index,
            message: message.to_string(),
            commit_id: oid.to_string(),
        });
        true
    })?;
    Ok(entries)
}

/// The trees of `stash@{index}`: its worktree commit and its index and untracked parents.
fn stash_trees(repo: &Repository, index: usize) -> Result<Vec<git2::Tree<'_>>, StashError> {
    let Some(id) = repo
        .reflog("refs/stash")
        .ok()
        .and_then(|reflog| reflog.get(index).map(|entry| entry.id_new()))
    else {
        return Ok(Vec::new());
    };
    let stash = repo.find_commit(id)?;
    let mut trees = vec![stash.tree()?];
    for parent in stash.parents().skip(1) {
        trees.push(parent.tree()?);
    }
    Ok(trees)
}

/// Applies `stash@{index}`. `restore_index` also restages what was staged (libgit2's
/// `REINSTATE_INDEX`, `git stash apply --index`). libgit2 refuses outright while the index
/// differs from HEAD; its errors are mapped to user-facing variants.
///
/// `expected`, when given, is the stash's commit id as the caller listed it; a mismatch with
/// `stash@{index}` is `StashListChanged`.
pub fn apply_stash(
    repo: &mut Repository,
    index: usize,
    expected: Option<&str>,
    restore_index: bool,
) -> Result<StashApplyOutcome, StashError> {
    ensure_stash_at(repo, index, expected)?;
    // Strictly, on every OS, before the checks below and libgit2's checkout (#369), as
    // `prepare_forced_checkout` does: `try_folds_case` doesn't read it on macOS and Windows.
    crate::worktree_guard::core_ignorecase(repo)?;
    if repo.index()?.has_conflicts() {
        return Err(StashError::UnresolvedConflicts);
    }
    // Safe checkouts create files through a symlinked folder (#114): the stash's worktree,
    // index and untracked trees are what the apply writes.
    {
        let incoming = stash_trees(repo, index)?;
        let incoming: Vec<_> = incoming
            .iter()
            .map(|tree| {
                crate::worktree_guard::Incoming::Merge(crate::worktree_guard::GuardSource::Tree(
                    tree,
                ))
            })
            .collect();
        crate::worktree_guard::refuse_linked_folders::<StashError>(repo, &incoming)?;
    }
    refuse_untracked_file_over_folder(repo, index)?;
    refuse_untracked_below_tracked_file(repo, index)?;
    let mut options = git2::StashApplyOptions::new();
    if restore_index {
        options.reinstantiate_index();
    }
    // Default (safe) checkout strategy: refuses to overwrite a locally modified file, like
    // `branch::switch_branch`'s `checkout_tree(..., None)`. A checkout that stops part-way (a
    // file another program holds open, a read-only file) is rolled back, so the apply can be
    // tried again (#321): libgit2 checks out from the index, which it writes only at the end.
    let written = crate::partial_checkout::Written::default();
    let mut checkout = git2::build::CheckoutBuilder::new();
    written.record(&mut checkout);
    options.checkout_options(checkout);
    let applied = repo.stash_apply(index, Some(&mut options));
    let repo: &Repository = repo;
    applied.map_err(|error| {
        let restored = crate::partial_checkout::roll_back(repo, &written);
        match (error.code(), error.class()) {
            (ErrorCode::Uncommitted, _) => StashError::StagedChanges,
            (ErrorCode::Conflict, ErrorClass::Checkout) => StashError::WouldOverwrite,
            (ErrorCode::Conflict, _) => StashError::IndexConflict,
            // libgit2's stash apply trips over a stashed file whose directory is a symlink in
            // HEAD (the directory had replaced a tracked symlink when the file was stashed) and
            // looks up a null blob id. Nothing has been written at that point.
            (ErrorCode::NotFound, ErrorClass::Odb) if error.message().contains("null OID") => {
                StashError::SymlinkReplacedByDirectory
            }
            _ => crate::partial_checkout::report(
                repo,
                restored,
                crate::HeldDuring::RolledBack,
                error,
            ),
        }
    })?;
    let mut files = Vec::new();
    let repo_index = repo.index()?;
    if repo_index.has_conflicts() {
        for conflict in repo_index.conflicts()? {
            if let Some(path) = crate::merge::conflict_path(&conflict?) {
                if !files.contains(&path) {
                    files.push(path);
                }
            }
        }
    }
    if files.is_empty() {
        Ok(StashApplyOutcome::Applied)
    } else {
        Ok(StashApplyOutcome::Conflicted { files })
    }
}

/// Refuses when `stash@{index}` holds an untracked file where the worktree has a real folder;
/// nothing is written. `FileReplacedFolder` when the stash itself recorded the swap (its base has
/// the folder, its worktree tree doesn't): libgit2 checks the untracked files out before the
/// tracked changes (git does it the other way round), so it can't put the file back, and it would
/// report success with the file left as `<name>~Stashed changes` (#186); `git stash apply` can.
/// Otherwise the folder is new since the stash: `UntrackedOverFolder`.
fn refuse_untracked_file_over_folder(repo: &Repository, index: usize) -> Result<(), StashError> {
    let Some(workdir) = repo.workdir() else {
        return Ok(());
    };
    let Some(id) = repo
        .reflog("refs/stash")
        .ok()
        .and_then(|reflog| reflog.get(index).map(|entry| entry.id_new()))
    else {
        return Ok(());
    };
    let stash = repo.find_commit(id)?;
    let Ok(untracked) = stash.parent(2) else {
        return Ok(());
    };
    let mut blocked = None;
    untracked
        .tree()?
        .walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() == Some(git2::ObjectType::Tree) {
                return git2::TreeWalkResult::Ok;
            }
            let Ok(name) = entry.name() else {
                return git2::TreeWalkResult::Ok;
            };
            if std::fs::symlink_metadata(workdir.join(root).join(name))
                .is_ok_and(|meta| meta.is_dir())
            {
                blocked = Some(format!("{root}{name}"));
                return git2::TreeWalkResult::Abort;
            }
            git2::TreeWalkResult::Ok
        })
        .or_else(|error| {
            // `Abort` surfaces as a `User` error.
            if blocked.is_some() {
                Ok(())
            } else {
                Err(error)
            }
        })?;
    let Some(path) = blocked else {
        return Ok(());
    };
    let is_folder = |tree: &git2::Tree| {
        tree.get_path(std::path::Path::new(&path))
            .is_ok_and(|entry| entry.kind() == Some(git2::ObjectType::Tree))
    };
    if is_folder(&stash.parent(0)?.tree()?) && !is_folder(&stash.tree()?) {
        Err(StashError::FileReplacedFolder)
    } else {
        Err(StashError::UntrackedOverFolder(path))
    }
}

/// Refuses when `stash@{index}` holds an untracked file below a path the repository index has
/// as a file (`UntrackedBelowTrackedFile`); nothing is written and the stash is kept. libgit2
/// merges the untracked tree against the index (`merge_index_and_tree(NULL, repo_index,
/// untracked_tree)`, stash.c), so `d/x` over the indexed file `d` is a directory/file conflict
/// it checks out as `d~Updated upstream` while reporting success, and a pop would drop the
/// stash (#206). Typically the stash itself recorded the swap (the untracked `d/x` with the
/// file `d` deleted); the deletion is applied only after the untracked files. libgit2's merge
/// compares exact case. Where names fold (`try_folds_case`: the repository sets
/// `core.ignorecase`, or always on macOS and Windows, like the case-collision walker) paths
/// compare as `fold_name` folds them, so `d/x` is refused over the indexed file `D` too (#234). On a case-insensitive
/// drive `d` is `D` and `git stash pop` refuses as well. On a case-sensitive drive with
/// `core.ignorecase` set, git would pop it; Browsitory refuses on purpose, since the setting may
/// be right (a Linux casefold folder or a vfat mount folds names too).
fn refuse_untracked_below_tracked_file(repo: &Repository, index: usize) -> Result<(), StashError> {
    let Some(id) = repo
        .reflog("refs/stash")
        .ok()
        .and_then(|reflog| reflog.get(index).map(|entry| entry.id_new()))
    else {
        return Ok(());
    };
    let Ok(untracked) = repo.find_commit(id)?.parent(2) else {
        return Ok(());
    };
    let ignore_case = crate::worktree_guard::try_folds_case(repo)?;
    let fold = |path: &[u8]| {
        let folded = if ignore_case {
            crate::worktree_guard::fold_name(path)
        } else {
            path.to_vec()
        };
        String::from_utf8_lossy(&folded).into_owned()
    };
    // Folded path → the index's own spelling. When two index paths fold together (only a
    // case-sensitive drive holds both), the first in index order is the one reported.
    let mut files = std::collections::HashMap::new();
    for entry in repo.index()?.iter() {
        if (entry.flags >> 12) & 0x3 == 0 {
            files
                .entry(fold(&entry.path))
                .or_insert_with(|| String::from_utf8_lossy(&entry.path).into_owned());
        }
    }
    let mut blocked = None;
    untracked
        .tree()?
        .walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            if entry.kind() == Some(git2::ObjectType::Tree) {
                return git2::TreeWalkResult::Ok;
            }
            let Ok(name) = entry.name() else {
                return git2::TreeWalkResult::Ok;
            };
            let path = format!("{root}{name}");
            let folded = fold(path.as_bytes());
            if let Some(file) = folded
                .match_indices('/')
                .find_map(|(slash, _)| files.get(&folded[..slash]))
            {
                blocked = Some((path.clone(), file.clone()));
                return git2::TreeWalkResult::Abort;
            }
            git2::TreeWalkResult::Ok
        })
        .or_else(|error| {
            // `Abort` surfaces as a `User` error.
            if blocked.is_some() {
                Ok(())
            } else {
                Err(error)
            }
        })?;
    match blocked {
        Some((untracked, file)) => Err(StashError::UntrackedBelowTrackedFile { untracked, file }),
        None => Ok(()),
    }
}

/// Applies `stash@{index}` and drops it only when it applied cleanly; a conflicted apply keeps
/// the stash, like `git stash pop`. Not `git2::Repository::stash_pop`: libgit2 drops the stash
/// even when the apply left conflicts, losing it.
pub fn pop_stash(
    repo: &mut Repository,
    index: usize,
    expected: Option<&str>,
    restore_index: bool,
) -> Result<StashApplyOutcome, StashError> {
    let outcome = apply_stash(repo, index, expected, restore_index)?;
    if outcome == StashApplyOutcome::Applied {
        repo.stash_drop(index)?;
    }
    Ok(outcome)
}

/// The files of the stash whose commit is `commit_id`; `NotAStash` when no stash entry has that
/// id (keyed by id, not index, because indices shift as stashes come and go).
pub fn stash_files(repo: &mut Repository, commit_id: &str) -> Result<StashFiles, StashError> {
    if !list_stashes(repo)?
        .iter()
        .any(|entry| entry.commit_id == commit_id)
    {
        return Err(StashError::NotAStash(commit_id.to_string()));
    }
    let tracked = crate::diff::commit_files(repo, commit_id)?;
    let stash = repo.find_commit(git2::Oid::from_str(commit_id)?)?;
    let (untracked, untracked_commit_id) = match stash.parent(2) {
        Ok(untracked_commit) => {
            let id = untracked_commit.id().to_string();
            (crate::diff::commit_files(repo, &id)?, Some(id))
        }
        Err(_) => (Vec::new(), None),
    };
    Ok(StashFiles {
        tracked,
        untracked,
        untracked_commit_id,
    })
}

/// `expected` as for `apply_stash`.
pub fn drop_stash(
    repo: &mut Repository,
    index: usize,
    expected: Option<&str>,
) -> Result<(), StashError> {
    ensure_stash_at(repo, index, expected)?;
    // Indices shift after a drop (git2's own behavior) — callers always re-fetch the full list
    // afterward, so a stale index is never held across calls.
    repo.stash_drop(index)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ntfs_fold_meets_letter_case_only() {
        // #389, as NTFS showed on CI: `Café`/`CAFÉ` are one file name, `Straße`/`STRASSE` and
        // NFC/NFD spellings are two.
        let same = |a: &str, b: &str| ntfs_fold(a.as_bytes()) == ntfs_fold(b.as_bytes());
        assert!(same("README", "readme"));
        assert!(same("Caf\u{e9}", "CAF\u{c9}"));
        assert!(!same("Stra\u{df}e", "STRASSE"));
        assert!(!same("Stra\u{df}e", "Strasse"));
        assert!(!same("Caf\u{e9}", "Cafe\u{301}"));
        assert!(!same("a", "b"));
        assert_eq!(
            ntfs_fold("Stra\u{df}e".as_bytes()),
            "STRA\u{df}E".as_bytes()
        );
        // Not UTF-8: ASCII letters only.
        assert_eq!(ntfs_fold(b"ab\xff"), b"AB\xff");
    }

    #[test]
    fn stash_fold_follows_the_drive() {
        let same = |a: &str, b: &str| stash_fold(a.as_bytes()) == stash_fold(b.as_bytes());
        assert!(same("Dir/Old", "dir/old"));
        assert!(!same("Old", "new"));
        assert_eq!(same("Stra\u{df}e", "Strasse"), cfg!(target_os = "macos"));
        assert_eq!(same("Caf\u{e9}", "Cafe\u{301}"), cfg!(target_os = "macos"));
        assert_eq!(
            same("Caf\u{e9}", "CAF\u{c9}"),
            cfg!(any(target_os = "macos", windows))
        );
    }
}
