use std::path::{Path, PathBuf};

use git2::{BranchType, ErrorCode, Repository, WorktreeLockStatus};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("cannot remove the main worktree")]
    MainWorktree,
    #[error("worktree path already exists")]
    PathExists,
    #[error("a start point is required to create a new branch")]
    StartPointRequired,
    #[error("worktree has uncommitted changes")]
    Dirty,
    #[error("worktree is locked")]
    Locked,
    #[error("git operation failed")]
    Git,
    /// A relative destination in a repository without a working folder (#231).
    #[error(
        "Enter the worktree's full path: this repository has no working folder to start a \
         relative path from."
    )]
    RelativePath,
    /// A Windows path tied to a drive or a root but not both (`C:feature`, `\feature`): it
    /// would depend on the app's current drive or folder (#231).
    #[error(
        "Enter the worktree's full path, with the drive and the folders from its top (for \
         example C:\\work\\feature)."
    )]
    PartlyAnchoredPath,
    /// The destination is inside the repository's git folder (#231).
    #[error("A worktree can't be inside the repository's .git folder. Choose another folder.")]
    InsideGitFolder,
    /// The folder at a worktree's place doesn't point back at the worktree's entry: it was
    /// replaced (by another repository, another worktree or a plain folder), so it is never
    /// deleted (#252).
    #[error(
        "The folder '{}' is no longer this worktree. Move it away or delete it, then remove \
         the worktree again.",
        path.display()
    )]
    NotThisWorktree { path: PathBuf },
    #[error("Can't create the folder '{}': {reason}", path.display())]
    CreateFolder { path: PathBuf, reason: String },
    /// libgit2 failed to add the worktree; what it had written was removed again (#245).
    #[error("Couldn't create the worktree: {reason}.")]
    AddFailed { reason: String },
    /// As [`WorktreeError::AddFailed`], but something else was in the new folder, so the folder
    /// was kept.
    #[error(
        "Couldn't create the worktree: {reason}. Its folder '{}' couldn't be removed completely; \
         delete it by hand.",
        path.display()
    )]
    AddFailedFolderLeft { reason: String, path: PathBuf },
    /// On Windows, the worktree's name or a folder of its destination can't be used there
    /// (#261).
    /// `in_path` is set for a folder of the destination, unset for the worktree's name.
    #[error(
        "Windows can't use '{name}' as a folder name because {problem}. Choose another {}.",
        if *in_path { "folder" } else { "name" }
    )]
    UnusableWindowsName {
        name: String,
        problem: WindowsNameProblem,
        in_path: bool,
    },
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
    /// The branch is being rebased in a terminal, in this or another worktree (#263).
    #[error(transparent)]
    BeingRebased(#[from] crate::repo::BranchBeingRebased),
    /// Another worktree holds the branch: checked out, being rebased or bisected, or used by a
    /// paused cherry-pick or revert there (#291, #293).
    #[error(transparent)]
    Held(#[from] crate::repo::BranchHeld),
}

impl From<git2::Error> for WorktreeError {
    fn from(error: git2::Error) -> Self {
        match error.code() {
            ErrorCode::Uncommitted | ErrorCode::Modified | ErrorCode::IndexDirty => Self::Dirty,
            ErrorCode::Locked => Self::Locked,
            _ => Self::Git,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub head: String,
    pub is_main: bool,
    pub is_locked: bool,
    pub is_prunable: bool,
}

pub fn list_worktrees(repo: &Repository) -> Result<Vec<WorktreeInfo>, WorktreeError> {
    let main_path = main_worktree_path(repo)?;
    let main_repo = Repository::open(&main_path)?;
    let mut worktrees = vec![WorktreeInfo {
        name: "main".to_string(),
        path: main_path,
        head: head_identity(&main_repo),
        is_main: true,
        is_locked: false,
        is_prunable: false,
    }];

    // One entry's state (a missing or unreadable folder, a broken entry) never fails the list
    // (#252). Two entries can name one folder (an old one whose folder was deleted by hand and a
    // newer one made there); both are listed, as `git worktree list` does.
    for name in repo.worktrees()?.iter().flatten().flatten() {
        let Ok(worktree) = repo.find_worktree(name) else {
            continue;
        };
        let path = display_path(worktree.path());
        if path == worktrees[0].path {
            continue;
        }
        let head = if folder_points_back(repo, name, &path) {
            Repository::open(&path)
                .map(|worktree_repo| head_identity(&worktree_repo))
                .unwrap_or_default()
        } else {
            String::new()
        };
        worktrees.push(WorktreeInfo {
            name: name.to_string(),
            head,
            path,
            is_main: false,
            is_locked: matches!(worktree.is_locked(), Ok(WorktreeLockStatus::Locked(_))),
            is_prunable: worktree.is_prunable(None).unwrap_or(false),
        });
    }

    Ok(worktrees)
}

/// How a destination path, as entered, is tied to the file system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Anchoring {
    Absolute,
    Relative,
    /// Tied to a drive or a root but not both (Windows `C:feature`, `\feature`).
    Partial,
}

fn anchoring(path: &Path) -> Anchoring {
    if path.is_absolute() {
        Anchoring::Absolute
    } else if path.components().any(|component| {
        matches!(
            component,
            std::path::Component::Prefix(_) | std::path::Component::RootDir
        )
    }) {
        Anchoring::Partial
    } else {
        Anchoring::Relative
    }
}

/// The folder a new worktree goes to, given `path` as entered (#231). A relative path starts
/// from the open worktree's top folder, as with `git worktree add` run there, never from the
/// app's working directory. Refused: a relative path in a bare repository, a Windows path tied
/// to a drive or a root but not both, and a folder inside the repository's git folder (which
/// git allows, but where git keeps its own data).
#[doc(hidden)]
pub fn resolve_destination(repo: &Repository, path: &Path) -> Result<PathBuf, WorktreeError> {
    let resolved = match anchoring(path) {
        Anchoring::Absolute => path.to_path_buf(),
        Anchoring::Partial => return Err(WorktreeError::PartlyAnchoredPath),
        Anchoring::Relative => repo
            .workdir()
            .ok_or(WorktreeError::RelativePath)?
            .join(path),
    };
    let destination = comparable_path(&resolved);
    let inside_git_folder = [repo.path(), repo.commondir()]
        .into_iter()
        .filter_map(|git_dir| git_dir.canonicalize().ok())
        .any(|git_dir| destination.starts_with(git_dir));
    if inside_git_folder {
        return Err(WorktreeError::InsideGitFolder);
    }
    Ok(resolved)
}

/// Why Windows can't use a folder name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum WindowsNameProblem {
    #[error("it ends in a dot")]
    TrailingDot,
    #[error("it ends in a space")]
    TrailingSpace,
    /// `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9` (and their superscript-digit
    /// forms), `CONIN$` and `CONOUT$`, with or without an extension, in any case.
    #[error("it is reserved for a device")]
    Reserved,
}

/// Why Windows can't use `name` as a file or folder name, if it can't (#261). Win32 paths drop
/// a trailing dot or space (`wt.` names `wt`), while libgit2 writes through `\\?\` paths and
/// keeps it, so the two would name different folders. A reserved device name names the device,
/// not a folder.
fn windows_name_problem(name: &str) -> Option<WindowsNameProblem> {
    if name.ends_with('.') {
        return Some(WindowsNameProblem::TrailingDot);
    }
    if name.ends_with(' ') {
        return Some(WindowsNameProblem::TrailingSpace);
    }
    // As in git's `is_valid_win32_path`: the device name, then optional spaces, then the end, an
    // extension or a stream (`:`).
    let base = name
        .split(['.', ':'])
        .next()
        .unwrap_or(name)
        .trim_end_matches(' ');
    // Device names are ASCII: `to_uppercase` would also turn a dotless `ı` into `I`.
    let upper = base.to_ascii_uppercase();
    let reserved = matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|device| {
        upper.strip_prefix(device).is_some_and(|digit| {
            let mut chars = digit.chars();
            matches!(
                (chars.next(), chars.next()),
                (Some('1'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}'), None)
            )
        })
    });
    reserved.then_some(WindowsNameProblem::Reserved)
}

/// Refuses a worktree `name`, or a folder of the destination `path` as entered, that Windows
/// can't use (see [`windows_name_problem`]). Called only on Windows: git allows these names
/// elsewhere, and so does Browsitory.
fn refuse_unusable_windows_names(name: &str, path: &Path) -> Result<(), WorktreeError> {
    // A drive, a share, a verbatim (`\\?\`) prefix, the root, `.` and `..` aren't folder names.
    let folders = path.components().filter_map(|component| match component {
        std::path::Component::Normal(folder) => Some((folder.to_string_lossy(), true)),
        _ => None,
    });
    for (part, in_path) in std::iter::once((std::borrow::Cow::Borrowed(name), false)).chain(folders)
    {
        if let Some(problem) = windows_name_problem(&part) {
            return Err(WorktreeError::UnusableWindowsName {
                name: part.into_owned(),
                problem,
                in_path,
            });
        }
    }
    Ok(())
}

pub fn create_worktree(
    repo: &Repository,
    name: &str,
    path: &Path,
    branch: &str,
    start_point: Option<&str>,
) -> Result<(), WorktreeError> {
    create_worktree_with(
        repo,
        name,
        path,
        branch,
        start_point,
        crate::worktree_guard::dir_folds_case,
    )
}

/// [`create_worktree`] with the probe of the destination's case folding given (`probe`, called
/// with the nearest existing folder of `path`), so tests can stand in for a case-insensitive
/// drive.
#[doc(hidden)]
pub fn create_worktree_with(
    repo: &Repository,
    name: &str,
    path: &Path,
    branch: &str,
    start_point: Option<&str>,
    probe: impl FnOnce(&Path) -> std::io::Result<bool>,
) -> Result<(), WorktreeError> {
    if cfg!(windows) {
        refuse_unusable_windows_names(name, path)?;
    }
    let resolved = resolve_destination(repo, path)?;
    let path = resolved.as_path();
    crate::repo::check_branch_not_rebased(repo, branch)?;
    if let Some(held) = crate::repo::branch_held(
        repo,
        branch,
        crate::repo::HoldRule::CheckOut { include_this: true },
    )? {
        return Err(held.into());
    }
    match std::fs::symlink_metadata(path) {
        Ok(_) => return Err(WorktreeError::PathExists),
        // A file where a parent folder should be is named when the parents are created.
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) => {}
        Err(_) => return Err(WorktreeError::Git),
    }
    // The worktree may live on another file system than the repository (#162): probed only
    // when the repository's setting doesn't already say case folds. A failed probe leaves the
    // repository's setting to decide, as before.
    let case_matters = crate::worktree_guard::try_folds_case(repo)?
        || crate::worktree_guard::nearest_existing(path)
            .is_some_and(|dir| probe(dir).unwrap_or(false));

    let (existing, commit) = match repo.find_branch(branch, BranchType::Local) {
        Ok(branch) => {
            let commit = branch.get().peel_to_commit()?;
            (Some(branch), commit)
        }
        Err(error) if error.code() == ErrorCode::NotFound => {
            let start_point = start_point.ok_or(WorktreeError::StartPointRequired)?;
            (None, repo.revparse_single(start_point)?.peel_to_commit()?)
        }
        Err(error) => return Err(error.into()),
    };
    // git2 has no checkout options for worktree add, so the tree is checked up front (#156),
    // before anything (a new branch included) is created: when the repository's config says
    // case matters, or the destination's file system folds case (#162).
    let tree = commit.tree()?;
    crate::worktree_guard::refuse_case_collision_if::<WorktreeError, &str>(
        case_matters,
        &crate::worktree_guard::GuardSource::Tree(&tree),
        None,
    )?;
    // Like `git worktree add`, missing parent folders of the destination are created (#200);
    // libgit2 only creates the last one. Whatever this call creates is removed again when a
    // later step fails, so a retry with the same name and branch starts clean.
    let mut created = Vec::new();
    if let Err(error) = create_parent_folders(path, &mut created) {
        remove_created_folders(&created);
        return Err(error);
    }
    let admin = single_component(name).map(|name| repo.commondir().join("worktrees").join(name));
    let admin_existed = admin
        .as_deref()
        .is_none_or(|admin| std::fs::symlink_metadata(admin).is_ok());
    let mut created_branch = false;
    let added = (|| {
        let reference = match existing {
            Some(branch) => branch.into_reference(),
            None => {
                let new_branch = repo.branch(branch, &commit, false)?;
                created_branch = true;
                new_branch.into_reference()
            }
        };
        let mut options = git2::WorktreeAddOptions::new();
        options.reference(Some(&reference));
        repo.worktree(name, path, Some(&options))
            .map_err(|error| WorktreeError::AddFailed {
                reason: plain_reason(&error),
            })?;
        Ok::<(), WorktreeError>(())
    })();
    let Err(mut error) = added else {
        return Ok(());
    };
    // libgit2 makes the entry and the destination (each `mkdir` exclusive), writes `.git`,
    // `commondir` and `gitdir`, then HEAD, then checks out, and cleans up nothing when a step
    // fails (#245). An entry made by this call whose `gitdir` names this destination (or that has
    // no `gitdir` yet) is ours: it goes, with what the add wrote into the destination, so a
    // retry with the same name works and the branch isn't held as checked out.
    let ours = admin
        .filter(|_| !admin_existed)
        .filter(|admin| entry_names_destination(admin, path));
    if let Some(admin) = ours {
        if let WorktreeError::AddFailed { reason } = error {
            error = if destination_written_by(&admin, path) {
                roll_back_destination(repo, path, &tree, reason)
            } else {
                WorktreeError::AddFailed { reason }
            };
        }
        let _ = std::fs::remove_dir_all(&admin);
    }
    if created_branch {
        if let Ok(mut new_branch) = repo.find_branch(branch, BranchType::Local) {
            let _ = new_branch.delete();
        }
    }
    // libgit2 creates the destination itself; when it isn't known to be ours, it goes only while
    // still empty. It didn't exist when the call started, so one left behind is named.
    error = match error {
        WorktreeError::AddFailed { reason } => folder_left(path, reason),
        other => {
            let _ = std::fs::remove_dir(path);
            other
        }
    };
    remove_created_folders(&created);
    Err(error)
}

/// libgit2's message as the reason of [`WorktreeError::AddFailed`], without a final period
/// (the error's text adds one).
fn plain_reason(error: &git2::Error) -> String {
    error.message().trim().trim_end_matches('.').to_string()
}

/// The entry `admin` has no `gitdir` file yet, or its `gitdir` names `path`'s `.git`.
fn entry_names_destination(admin: &Path, path: &Path) -> bool {
    let gitdir = match std::fs::read_to_string(admin.join("gitdir")) {
        Ok(gitdir) => gitdir,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return true,
        Err(_) => return false,
    };
    same_path(Path::new(gitdir.trim()), &path.join(".git"))
}

/// The destination holds what libgit2 wrote for the entry `admin`: its `.git` file points at
/// `admin`. libgit2 writes that file only into a destination it just created, so without it
/// the folder may be someone else's and nothing in it is touched.
fn destination_written_by(admin: &Path, path: &Path) -> bool {
    let Ok(Some(crate::worktree_guard::Leaf::File { bytes, .. })) =
        crate::worktree_guard::read_beneath(path, ".git")
    else {
        return false;
    };
    String::from_utf8_lossy(&bytes)
        .trim()
        .strip_prefix("gitdir:")
        // A relative `gitdir:` (`git worktree add --relative-paths`) starts from the folder.
        .is_some_and(|target| same_path(&path.join(target.trim()), admin))
}

/// The folder `path` is the worktree of the entry `name`: its `.git` file points back at
/// `<commondir>/worktrees/<name>`, as `git worktree remove` checks before deleting anything.
fn folder_points_back(repo: &Repository, name: &str, path: &Path) -> bool {
    single_component(name).is_some_and(|name| {
        destination_written_by(&repo.commondir().join("worktrees").join(name), path)
    })
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Removes from `path` what a failed worktree add wrote there: each file, link or submodule
/// folder of `tree` (then the folders that emptied), the tree's folders while empty, the `.git`
/// file, and `path` itself once empty. Anything else is kept, and so is `path`, which the
/// returned error then names.
#[doc(hidden)]
pub fn roll_back_destination(
    repo: &Repository,
    path: &Path,
    tree: &git2::Tree<'_>,
    reason: String,
) -> WorktreeError {
    let mut leaves = Vec::new();
    let mut folders = Vec::new();
    tree_paths(repo, tree, Path::new(""), &mut leaves, &mut folders);
    for rel in &leaves {
        let _ = crate::worktree_guard::remove_and_prune_beneath(path, rel);
    }
    // A file the checkout never wrote (a name the file system refuses) stops the pass above
    // before its folder is pruned, so the tree's folders go too: deepest first, each only while
    // empty.
    for rel in folders.iter().rev() {
        let _ = crate::worktree_guard::remove_and_prune_beneath(path, rel);
    }
    let _ = crate::worktree_guard::remove_beneath(path, ".git");
    folder_left(path, reason)
}

/// Collects the paths of `tree` below `prefix`: files, links and submodules on `leaves`,
/// folders on `folders` (each before what it holds). On Unix a name is taken as raw bytes, so
/// one that isn't UTF-8 is removed too (#262).
fn tree_paths(
    repo: &Repository,
    tree: &git2::Tree<'_>,
    prefix: &Path,
    leaves: &mut Vec<PathBuf>,
    folders: &mut Vec<PathBuf>,
) {
    for entry in tree.iter() {
        let Some(name) = tree_entry_name(entry.name_bytes()) else {
            continue;
        };
        let rel = prefix.join(name);
        if entry.kind() == Some(git2::ObjectType::Tree) {
            folders.push(rel.clone());
            if let Ok(subtree) = repo.find_tree(entry.id()) {
                tree_paths(repo, &subtree, &rel, leaves, folders);
            }
        } else {
            leaves.push(rel);
        }
    }
}

/// A tree entry's name as one plain path component, or `None` to skip it. Git never writes an
/// empty name, `.`, `..`, or one holding `/` or NUL into a tree, but such a raw name is skipped
/// here rather than left for `Path::components` to split or drop: only a name that is exactly
/// one normal component, spelled as given, is joined onto the destination.
fn tree_entry_name(bytes: &[u8]) -> Option<&Path> {
    if matches!(bytes, b"" | b"." | b"..") || bytes.contains(&b'/') || bytes.contains(&0) {
        return None;
    }
    let name = raw_name(bytes)?;
    let mut components = name.components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(only)), None) if only == name.as_os_str() => Some(name),
        _ => None,
    }
}

/// The name's raw bytes on Unix, so a name that isn't UTF-8 is removed too (#262).
#[cfg(unix)]
fn raw_name(bytes: &[u8]) -> Option<&Path> {
    use std::os::unix::ffi::OsStrExt;
    Some(Path::new(std::ffi::OsStr::from_bytes(bytes)))
}

/// On Windows a name that isn't UTF-8 is skipped, not guessed at: libgit2 never wrote it.
/// Every path it writes goes through `git_win32_path_from_utf8` (`src/util/win32/path_w32.c`),
/// whose `git_utf8_to_16` (`src/util/win32/utf-conv.c`) calls `MultiByteToWideChar` with
/// `MB_ERR_INVALID_CHARS` and fails on invalid UTF-8.
#[cfg(not(unix))]
fn raw_name(bytes: &[u8]) -> Option<&Path> {
    std::str::from_utf8(bytes).ok().map(Path::new)
}

/// [`WorktreeError::AddFailed`] once `path` is removed (or was never made), else
/// [`WorktreeError::AddFailedFolderLeft`] naming it.
fn folder_left(path: &Path, reason: String) -> WorktreeError {
    match std::fs::remove_dir(path) {
        Ok(()) => WorktreeError::AddFailed { reason },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            WorktreeError::AddFailed { reason }
        }
        Err(_) => WorktreeError::AddFailedFolderLeft {
            reason,
            path: path.to_path_buf(),
        },
    }
}

/// Creates the missing ancestors of `path` below its nearest existing folder, top-down, pushing
/// each folder made here on `created`.
fn create_parent_folders(path: &Path, created: &mut Vec<PathBuf>) -> Result<(), WorktreeError> {
    let (Some(parent), Some(base)) = (path.parent(), crate::worktree_guard::nearest_existing(path))
    else {
        return Ok(());
    };
    let missing: Vec<&Path> = parent
        .ancestors()
        .take_while(|ancestor| *ancestor != base && !ancestor.as_os_str().is_empty())
        .collect();
    for folder in missing.into_iter().rev() {
        match std::fs::create_dir(folder) {
            Ok(()) => created.push(folder.to_path_buf()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && folder.is_dir() => {}
            Err(error) => {
                let reason = if error.kind() == std::io::ErrorKind::AlreadyExists {
                    "a file with that name is in the way.".to_string()
                } else {
                    error.to_string()
                };
                return Err(WorktreeError::CreateFolder {
                    path: folder.to_path_buf(),
                    reason,
                });
            }
        }
    }
    Ok(())
}

/// Removes the folders [`create_parent_folders`] made, deepest first, each only while empty.
fn remove_created_folders(created: &[PathBuf]) {
    for folder in created.iter().rev() {
        let _ = std::fs::remove_dir(folder);
    }
}

/// `name` when it is one plain path component, so joining it can't leave `worktrees/`.
fn single_component(name: &str) -> Option<&str> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None) => Some(name),
        _ => None,
    }
}

/// Removes the linked worktree at `path`: its folder and its entry. Like `git worktree remove`,
/// a worktree whose folder is gone loses just its entry, and a locked one is refused either way.
/// Other worktrees' state (a missing folder, a broken entry) never stops the removal (#252).
///
/// When two entries name the folder, the one the folder points back at is taken; prefer
/// [`remove_worktree_named`], which says which entry.
pub fn remove_worktree(repo: &Repository, path: &Path) -> Result<(), WorktreeError> {
    let target_path = comparable_path(path);
    if target_path == main_worktree_path(repo)? {
        return Err(WorktreeError::MainWorktree);
    }

    let mut matching = Vec::new();
    for name in repo.worktrees()?.iter().flatten().flatten() {
        // An entry that can't be read can't be the one asked for.
        let Ok(worktree) = repo.find_worktree(name) else {
            continue;
        };
        if comparable_path(worktree.path()) == target_path {
            matching.push((name.to_string(), worktree));
        }
    }
    let chosen = matching
        .iter()
        .position(|(name, _)| folder_points_back(repo, name, path))
        .unwrap_or(0);
    if matching.is_empty() {
        return Err(WorktreeError::Git);
    }
    let (name, worktree) = matching.swap_remove(chosen);
    remove_entry(repo, &name, &worktree, path)
}

/// [`remove_worktree`] of the entry `name`, which must name the folder `path`.
pub fn remove_worktree_named(
    repo: &Repository,
    name: &str,
    path: &Path,
) -> Result<(), WorktreeError> {
    let target_path = comparable_path(path);
    if target_path == main_worktree_path(repo)? {
        return Err(WorktreeError::MainWorktree);
    }
    let worktree = repo.find_worktree(name).map_err(|_| WorktreeError::Git)?;
    if comparable_path(worktree.path()) != target_path {
        return Err(WorktreeError::Git);
    }
    remove_entry(repo, name, &worktree, path)
}

/// Removes the entry `name` and, when its folder `path` is there and points back at the
/// entry, the folder. A folder that doesn't point back is never deleted (#252).
fn remove_entry(
    repo: &Repository,
    name: &str,
    worktree: &git2::Worktree,
    path: &Path,
) -> Result<(), WorktreeError> {
    if matches!(worktree.is_locked()?, WorktreeLockStatus::Locked(_)) {
        return Err(WorktreeError::Locked);
    }
    let folder_present = match std::fs::symlink_metadata(path) {
        Ok(_) => true,
        // A file where a parent folder was: the folder is gone too.
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            false
        }
        Err(_) => return Err(WorktreeError::Git),
    };
    if folder_present {
        // libgit2 deletes whatever folder is at the entry's place, even another repository;
        // `git worktree remove` first checks the folder's `.git` points back, and so does this.
        if !folder_points_back(repo, name, path) {
            return Err(WorktreeError::NotThisWorktree {
                path: path.to_path_buf(),
            });
        }
        let linked_repo = Repository::open(path)?;
        if !crate::status::status(&linked_repo)
            .map_err(|_| WorktreeError::Git)?
            .is_empty()
        {
            return Err(WorktreeError::Dirty);
        }
    }
    let mut options = git2::WorktreePruneOptions::new();
    options.valid(true).working_tree(folder_present);
    worktree.prune(Some(&mut options))?;
    Ok(())
}

/// `path` spelled so that two spellings of one folder compare equal, even once it is gone: the
/// nearest existing ancestor resolved (links, `.`, `..`), the missing rest appended as written
/// without `.` and `..`.
fn comparable_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let mut rest = Vec::new();
    let mut base = path;
    loop {
        if let Ok(canonical) = base.canonicalize() {
            let mut resolved = canonical;
            for component in rest.into_iter().rev() {
                match component {
                    std::path::Component::CurDir => {}
                    std::path::Component::ParentDir => {
                        resolved.pop();
                    }
                    other => resolved.push(other),
                }
            }
            return resolved;
        }
        let (Some(parent), Some(last)) = (base.parent(), base.components().next_back()) else {
            return path.to_path_buf();
        };
        rest.push(last);
        base = parent;
    }
}

pub fn prune_worktrees(repo: &Repository) -> Result<(), WorktreeError> {
    let names: Vec<String> = repo
        .worktrees()?
        .iter()
        .flatten()
        .flatten()
        .map(str::to_string)
        .collect();
    for name in names {
        let worktree = repo.find_worktree(&name)?;
        if worktree.is_prunable(None)? {
            worktree.prune(None)?;
        }
    }
    Ok(())
}

fn main_worktree_path(repo: &Repository) -> Result<PathBuf, WorktreeError> {
    let main_repo = Repository::open(repo.commondir())?;
    match main_repo.workdir() {
        Some(path) => canonical_path(path),
        // A bare repository with linked worktrees: its own folder is the main row, as in
        // `git worktree list`.
        None if main_repo.is_bare() => canonical_path(repo.commondir()),
        None => Err(WorktreeError::Git),
    }
}

fn canonical_path(path: &Path) -> Result<PathBuf, WorktreeError> {
    path.canonicalize().map_err(|_| WorktreeError::Git)
}

/// The path resolved, or as recorded when it can't be (gone, a file in place of a parent
/// folder, no access): one entry's folder never fails the list (#252).
fn display_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn head_identity(repo: &Repository) -> String {
    let Ok(head) = repo.head() else {
        return String::new();
    };
    if head.is_branch() {
        return head.shorthand().unwrap_or_default().to_string();
    }
    head.target().map(|oid| oid.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_relative_path_is_relative() {
        for path in ["feature", "a/b", "../feature", "./x", ".git/x"] {
            assert_eq!(anchoring(Path::new(path)), Anchoring::Relative, "{path}");
        }
        assert_eq!(anchoring(&std::env::temp_dir()), Anchoring::Absolute);
    }

    #[cfg(unix)]
    #[test]
    fn a_rooted_path_is_absolute_on_unix() {
        assert_eq!(anchoring(Path::new("/work/feature")), Anchoring::Absolute);
    }

    /// `C:feature` depends on drive C's current folder, `\feature` on the current drive.
    #[cfg(windows)]
    #[test]
    fn a_path_tied_to_a_drive_or_a_root_but_not_both_is_refused_on_windows() {
        for path in [r"C:feature", r"\feature", "/feature", r"C:..\feature"] {
            assert_eq!(anchoring(Path::new(path)), Anchoring::Partial, "{path}");
        }
        for path in [r"C:\work\feature", r"\\server\share\feature"] {
            assert_eq!(anchoring(Path::new(path)), Anchoring::Absolute, "{path}");
        }
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        for path in [r"C:feature", r"\feature"] {
            assert!(matches!(
                resolve_destination(&repo, Path::new(path)),
                Err(WorktreeError::PartlyAnchoredPath)
            ));
        }
    }

    /// Git never writes these names into a tree, but a raw name that isn't exactly one plain
    /// component must never reach the remove helper, not even aliased by `Path::components`.
    #[test]
    fn a_tree_entry_name_that_is_not_one_plain_component_is_skipped() {
        for name in [
            &b""[..],
            b".",
            b"..",
            b"/x",
            b"/",
            b"a/b",
            b"a/",
            b"a/.",
            b"./a",
            b"a\0b",
        ] {
            assert_eq!(tree_entry_name(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_plain_tree_entry_name_is_kept() {
        for name in ["file.txt", "...", ".git-like", "caf\u{e9}"] {
            assert_eq!(tree_entry_name(name.as_bytes()), Some(Path::new(name)));
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_tree_entry_name_that_is_not_utf8_is_kept_on_unix() {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            tree_entry_name(b"a\x80"),
            Some(Path::new(std::ffi::OsStr::from_bytes(b"a\x80")))
        );
    }

    #[test]
    fn a_name_ending_in_a_dot_or_a_space_is_unusable_on_windows() {
        for (name, problem) in [
            ("wt.", WindowsNameProblem::TrailingDot),
            ("wt...", WindowsNameProblem::TrailingDot),
            ("wt ", WindowsNameProblem::TrailingSpace),
            ("wt. ", WindowsNameProblem::TrailingSpace),
            ("wt .", WindowsNameProblem::TrailingDot),
        ] {
            assert_eq!(windows_name_problem(name), Some(problem), "{name:?}");
        }
    }

    #[test]
    fn a_reserved_device_name_is_unusable_on_windows() {
        for name in [
            "CON",
            "con",
            "Prn",
            "aux",
            "NUL",
            "nul.txt",
            "COM1",
            "com9",
            "LPT1",
            "lpt9.tar.gz",
            "conin$",
            "CONOUT$",
            "COM\u{b9}",
            "lpt\u{b3}",
            "aux .txt",
            "con:x",
        ] {
            assert_eq!(
                windows_name_problem(name),
                Some(WindowsNameProblem::Reserved),
                "{name:?}"
            );
        }
    }

    #[test]
    fn an_ordinary_name_is_usable_on_windows() {
        for name in [
            "wt",
            "feature-x",
            ".hidden",
            "a.b",
            "con1",
            "COM0",
            "COM10",
            "LPT",
            "auxiliary",
            "nul-branch",
            "my con",
            "x.con",
            "...a",
            " wt",
        ] {
            assert_eq!(windows_name_problem(name), None, "{name:?}");
        }
    }

    /// Device names are ASCII: a look-alike that only Unicode case mapping turns into one
    /// (dotless `ı` uppercases to `I`) is an ordinary name.
    #[test]
    fn a_non_ascii_look_alike_of_a_device_name_is_usable() {
        for name in ["con\u{131}n$", "CON\u{131}N$", "\u{131}aux"] {
            assert_eq!(windows_name_problem(name), None, "{name:?}");
        }
    }

    fn refused(name: &str, path: &str) -> Option<(String, WindowsNameProblem, bool)> {
        match refuse_unusable_windows_names(name, Path::new(path)) {
            Err(WorktreeError::UnusableWindowsName {
                name,
                problem,
                in_path,
            }) => Some((name, problem, in_path)),
            Ok(()) => None,
            Err(other) => panic!("{other}"),
        }
    }

    #[test]
    fn every_part_of_the_name_and_the_destination_is_checked() {
        assert_eq!(refused("wt", "work/wt"), None);
        assert_eq!(refused("wt", "../work/./wt/"), None);
        assert_eq!(
            refused("wt.", "work/wt"),
            Some(("wt.".to_string(), WindowsNameProblem::TrailingDot, false))
        );
        assert_eq!(
            refused("wt", "work./wt"),
            Some(("work.".to_string(), WindowsNameProblem::TrailingDot, true))
        );
        assert_eq!(
            refused("wt", "work/wt /"),
            Some(("wt ".to_string(), WindowsNameProblem::TrailingSpace, true))
        );
        assert_eq!(
            refused("wt", "aux/wt"),
            Some(("aux".to_string(), WindowsNameProblem::Reserved, true))
        );
    }

    /// The root, `.` and `..` aren't folder names and pass.
    #[test]
    fn roots_and_dot_components_are_accepted() {
        for path in ["/work/wt", "/work/./wt", "./wt", ".", "..", "../wt", "/"] {
            assert_eq!(refused("wt", path), None, "{path:?}");
        }
    }

    /// A drive, a share and a verbatim prefix aren't folder names either.
    #[cfg(windows)]
    #[test]
    fn windows_prefixes_and_dot_components_are_accepted() {
        for path in [
            r"C:\work\wt",
            r"C:\work\.\wt",
            r".\wt",
            r"..\wt",
            r"\\server\share\wt",
            r"\\?\C:\work\wt",
            r"\\?\UNC\server\share\wt",
            r"C:\",
        ] {
            assert_eq!(refused("wt", path), None, "{path:?}");
        }
    }

    #[test]
    fn the_refusal_names_the_part_and_the_problem() {
        let message = |name: &str, problem, in_path| {
            WorktreeError::UnusableWindowsName {
                name: name.to_string(),
                problem,
                in_path,
            }
            .to_string()
        };
        assert_eq!(
            message("wt.", WindowsNameProblem::TrailingDot, false),
            "Windows can't use 'wt.' as a folder name because it ends in a dot. Choose another \
             name."
        );
        assert_eq!(
            message("wt ", WindowsNameProblem::TrailingSpace, true),
            "Windows can't use 'wt ' as a folder name because it ends in a space. Choose another \
             folder."
        );
        assert_eq!(
            message("aux", WindowsNameProblem::Reserved, true),
            "Windows can't use 'aux' as a folder name because it is reserved for a device. \
             Choose another folder."
        );
    }

    /// On Windows a backslash or a drive prefix would also split or anchor the path.
    #[cfg(windows)]
    #[test]
    fn a_tree_entry_name_with_a_windows_separator_or_prefix_is_skipped() {
        for name in [&b"a\\b"[..], b"\\x", b"C:", b"C:x"] {
            assert_eq!(tree_entry_name(name), None, "{name:?}");
        }
    }
}
