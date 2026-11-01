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
    /// Stash all resets `tracked`, which deletes the untracked repository in `path` (its
    /// history included), with or without untracked files (a nested repository is never
    /// stashed). git deletes it; Browsitory refuses (#211).
    #[error("Stashing would delete the repository in '{path}', which is in the way of the tracked '{tracked}'. Move '{path}' first.")]
    RepositoryInTheWay { path: String, tracked: String },
    /// The stash holds the untracked file `untracked` below the path `file`, which the index
    /// has as a file (the stash also holds the file's deletion, or the file was committed
    /// later). libgit2 merges the untracked files against the index, so it would leave
    /// `<file>~Updated upstream` and report success (#206).
    #[error("This stash can't be applied here: its untracked file '{untracked}' needs a folder where '{file}' is a tracked file. Commit the deletion of '{file}' first, then apply it; the stash is kept.")]
    UntrackedBelowTrackedFile { untracked: String, file: String },
    #[error("{0}")]
    LinkedFolder(#[from] crate::LinkedFolder),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
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
    if !opts.paths.is_empty() {
        return save_path_limited_stash(repo, opts, message);
    }
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
        });
    }
    // libgit2's stash records and resets in one call, and its forced reset writes through a
    // symlink that replaced a tracked file or folder (#114). With such a link, stash every
    // changed path through Browsitory's path-limited stash instead: a file link is stashed and
    // then replaced by the file, a folder link is refused (`BeyondSymlink`).
    // The same route keeps a repository nested in an untracked folder, which libgit2's reset
    // with untracked files would delete (see `nested_repository_in_untracked_folder`).
    if crate::worktree_guard::has_hazard(repo)?
        || (opts.include_untracked && nested_repository_in_untracked_folder(repo)?)
    {
        let paths = changed_paths(repo, opts.include_untracked)?;
        if paths.is_empty() {
            return Err(StashError::NothingToStash);
        }
        let routed = StashOptions {
            paths,
            ..opts.clone()
        };
        return save_path_limited_stash(repo, &routed, message);
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
fn save_path_limited_stash(
    repo: &Repository,
    opts: &StashOptions,
    message: Option<&str>,
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

    let mut status_options = git2::StatusOptions::new();
    status_options
        .include_untracked(opts.include_untracked)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true)
        .disable_pathspec_match(true);
    for path in &opts.paths {
        status_options.pathspec(path.as_str());
    }
    let mut tracked = Vec::new();
    let mut untracked = Vec::new();
    for entry in repo.statuses(Some(&mut status_options))?.iter() {
        let status = entry.status();
        let Ok(path) = entry.path() else { continue };
        if status.contains(git2::Status::IGNORED) || path.ends_with('/') {
            continue;
        }
        if status == git2::Status::WT_NEW {
            untracked.push(path.to_string());
        } else {
            tracked.push(path.to_string());
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
    for path in opts.paths.iter().chain(&tracked).chain(&untracked) {
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
        crate::worktree_guard::remove_and_prune_beneath(&workdir, path)?;
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

/// True when an untracked repository (a folder with a `.git`) sits below an untracked folder:
/// a proper ancestor folder with no path in the index or in HEAD. libgit2's reset after a stash
/// with untracked files (`GIT_CHECKOUT_REMOVE_UNTRACKED`) removes such an ancestor whole,
/// nested repository and history included: `wd_item_is_removable` (checkout.c) only protects a
/// folder that itself holds a `.git`. git's `stash -u` keeps it (`git clean -d` skips nested
/// repositories). Such a stash takes the path-limited route, which deletes only the untracked
/// files it stashed.
fn nested_repository_in_untracked_folder(repo: &Repository) -> Result<bool, StashError> {
    let Some(workdir) = repo.workdir() else {
        return Ok(false);
    };
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true);
    let mut repositories = Vec::new();
    for entry in repo.statuses(Some(&mut options))?.iter() {
        if entry.status() != git2::Status::WT_NEW {
            continue;
        }
        let Ok(path) = entry.path() else { continue };
        if let Some(path) = path.strip_suffix('/') {
            if path.contains('/') && workdir.join(path).join(".git").exists() {
                repositories.push(path.to_string());
            }
        }
    }
    if repositories.is_empty() {
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
    Ok(repositories.iter().any(|path| {
        path.match_indices('/').any(|(slash, _)| {
            let folder = &path[..slash];
            !in_index.contains(folder) || !in_head.contains(folder)
        })
    }))
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
}

/// Before a whole stash: the first worktree entry the reset would delete because it is in the
/// way of a restored path, with that path. libgit2's forced checkout (`GIT_CHECKOUT_FORCE`)
/// removes a non-folder where the restored tree has a folder (`checkout_action_with_wd_blocker`)
/// and a folder where it has a file (`checkout_action_with_wd_dir`, removed with everything in
/// it, a nested `.git` included), ignored or not; `GIT_CHECKOUT_REMOVE_UNTRACKED` only removes
/// what the stash holds and leaves nested repositories alone. So in the way are:
///
/// - an ignored file or folder, or an untracked nested repository, in either mode;
/// - an untracked file only when `include_untracked` is off (otherwise it is in the stash).
///
/// The restored tree is HEAD's; with `keep_index` the index's too: libgit2 resets once to the
/// index commit, but with HEAD as the checkout baseline, so it still removes what is in the way
/// of a path HEAD has and the index deletes (git does too). With `core.ignorecase` paths
/// compare case-insensitively (`docs` is in the way of `Docs/f` on such a drive). Untracked
/// symlinks are left to the hazard route (`BeyondSymlink`).
fn entry_in_the_way(
    repo: &Repository,
    keep_index: bool,
    include_untracked: bool,
) -> Result<Option<InTheWay>, StashError> {
    let ignore_case = repo.config()?.get_bool("core.ignorecase").unwrap_or(false);
    let fold = |path: &str| {
        if ignore_case {
            path.to_lowercase()
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
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(true)
        .recurse_ignored_dirs(false)
        .exclude_submodules(true);
    for entry in repo.statuses(Some(&mut options))?.iter() {
        let status = entry.status();
        if status != git2::Status::WT_NEW && status != git2::Status::IGNORED {
            continue;
        }
        let Ok(raw) = entry.path() else { continue };
        // Untracked folders are only reported whole when they hold a repository (empty ones
        // aren't reported); ignored ones whenever the folder is ignored as a whole.
        let is_dir = raw.ends_with('/');
        let path = raw.trim_end_matches('/');
        let kind = if status == git2::Status::IGNORED {
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
    // `branch::switch_branch`'s `checkout_tree(..., None)`.
    repo.stash_apply(index, Some(&mut options))
        .map_err(|error| match (error.code(), error.class()) {
            (ErrorCode::Uncommitted, _) => StashError::StagedChanges,
            (ErrorCode::Conflict, ErrorClass::Checkout) => StashError::WouldOverwrite,
            (ErrorCode::Conflict, _) => StashError::IndexConflict,
            // libgit2's stash apply trips over a stashed file whose directory is a symlink in
            // HEAD (the directory had replaced a tracked symlink when the file was stashed) and
            // looks up a null blob id. Nothing has been written at that point.
            (ErrorCode::NotFound, ErrorClass::Odb) if error.message().contains("null OID") => {
                StashError::SymlinkReplacedByDirectory
            }
            _ => StashError::Git(error),
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
/// file `d` deleted); the deletion is applied only after the untracked files. Paths compare
/// exactly, as libgit2's merge does.
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
    let files: std::collections::HashSet<String> = repo
        .index()?
        .iter()
        .filter(|entry| (entry.flags >> 12) & 0x3 == 0)
        .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
        .collect();
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
            if let Some(file) = path
                .match_indices('/')
                .map(|(slash, _)| &path[..slash])
                .find(|prefix| files.contains(*prefix))
            {
                blocked = Some((path.clone(), file.to_string()));
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
