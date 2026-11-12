use thiserror::Error;

#[derive(Debug, Error)]
pub enum StatusError {
    #[error("failed to read repository status: {0}")]
    Read(#[from] git2::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum StatusKind {
    New,
    Modified,
    Deleted,
    Renamed,
    TypeChange,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// For a rename, the new path.
    pub path: String,
    /// The path before a staged rename; `None` for every other entry.
    pub old_path: Option<String>,
    pub staged: bool,
    pub kind: StatusKind,
    /// An untracked folder with more than [`UNTRACKED_FOLDER_THRESHOLD`] files, listed as one
    /// entry (`path` ends in `/`) instead of file by file (#845); see [`untracked_folder_files`].
    pub untracked_folder: bool,
}

/// An untracked folder with more files than this is listed as one entry (#845).
pub const UNTRACKED_FOLDER_THRESHOLD: usize = 500;

/// The files of the untracked folder `folder` (`dir/`), as unstaged new entries (#845).
/// Refuses a path that isn't a folder of the work tree (`dir/`, relative, no `..`, not through a
/// symbolic link) or is a nested repository.
pub fn untracked_folder_files(
    repo: &git2::Repository,
    folder: &str,
) -> Result<Vec<StatusEntry>, StatusError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| git2::Error::from_str("the repository has no work tree"))?;
    let relative = std::path::Path::new(folder);
    let plain = relative
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)));
    let is_folder = workdir
        .join(relative)
        .symlink_metadata()
        .is_ok_and(|meta| meta.is_dir());
    if !folder.ends_with('/')
        || !plain
        || !is_folder
        || crate::worktree_guard::has_symlinked_ancestor(workdir, folder.trim_end_matches('/'))
        || crate::untracked::is_repository(workdir, folder)
    {
        return Err(
            git2::Error::from_str(&format!("'{folder}' is not an untracked folder")).into(),
        );
    }
    let crate::untracked::Folder::Files(files) =
        crate::untracked::walk(repo, workdir, folder, None)
    else {
        unreachable!("a walk without a limit lists every file")
    };
    Ok(files.into_iter().map(new_file).collect())
}

/// An unstaged new file (or nested repository) entry.
fn new_file(path: String) -> StatusEntry {
    StatusEntry {
        path,
        old_path: None,
        staged: false,
        kind: StatusKind::New,
        untracked_folder: false,
    }
}

/// The options `status` reads with; `change_stamp` reads the same entries (#650).
///
/// Explicit options, because libgit2's defaults both do too much and too little: they scan
/// ignored files (whose flags then resolve to `None`/`None` and get dropped by `status`) and
/// they leave rename detection off. Renames are detected between HEAD and the index only, like
/// `git status`: a file moved but not staged is "deleted" + "new", so per-file stage and discard
/// keep acting on one path (#137).
pub(crate) fn status_options() -> git2::StatusOptions {
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        // A wholly untracked folder is one `dir/` entry; `untracked::expand` lists the small
        // ones file by file (#845).
        .recurse_untracked_dirs(false)
        .include_ignored(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(false);
    options
}

/// The status of `repo`, read only: the index's stat data stays as it is (the internal reads of
/// write commands use it). The app's own status reads use [`status_refreshing_index`].
pub fn status(repo: &git2::Repository) -> Result<Vec<StatusEntry>, StatusError> {
    let statuses = repo.statuses(Some(&mut status_options()))?;
    let expansion = crate::untracked::expand(repo, &statuses);
    Ok(entries(&statuses, &expansion))
}

/// The entries of a scan made with [`status_options`].
pub(crate) fn entries(
    statuses: &git2::Statuses<'_>,
    expansion: &crate::untracked::Expansion,
) -> Vec<StatusEntry> {
    let mut entries = Vec::new();

    for entry in statuses.iter() {
        // `entry.path()` is a rename's *old* path; each side's delta names its own paths.
        let Ok(path) = entry.path() else { continue };
        let flags = entry.status();
        if expansion.small.contains(path) {
            continue;
        }
        if expansion.large.contains(path) {
            entries.push(StatusEntry {
                untracked_folder: true,
                ..new_file(path.to_string())
            });
            continue;
        }

        if flags.is_conflicted() {
            entries.push(StatusEntry {
                path: path.to_string(),
                old_path: None,
                staged: false,
                kind: StatusKind::Conflicted,
                untracked_folder: false,
            });
            continue;
        }

        if let Some(kind) = staged_kind(flags) {
            let delta = entry.head_to_index();
            let (new, old) = delta_paths(delta.as_ref(), path);
            entries.push(StatusEntry {
                path: new,
                old_path: if kind == StatusKind::Renamed {
                    old
                } else {
                    None
                },
                staged: true,
                kind,
                untracked_folder: false,
            });
        }
        if let Some(kind) = unstaged_kind(flags) {
            let delta = entry.index_to_workdir();
            let (new, _) = delta_paths(delta.as_ref(), path);
            entries.push(StatusEntry {
                path: new,
                old_path: None,
                staged: false,
                kind,
                untracked_folder: false,
            });
        }
    }

    if !expansion.files.is_empty() {
        entries.extend(expansion.files.iter().cloned().map(new_file));
        // Stable: a path's staged entry stays before its unstaged one.
        entries.sort_by(|a, b| a.path.cmp(&b.path));
    }
    entries
}

/// [`status`], with the index's stat data refreshed on the way (#841); see `index_refresh`.
pub fn status_refreshing_index(repo: &git2::Repository) -> Result<Vec<StatusEntry>, StatusError> {
    let (statuses, _) = crate::index_refresh::scan(repo)?;
    let expansion = crate::untracked::expand(repo, &statuses);
    Ok(entries(&statuses, &expansion))
}

/// The delta's new path (falling back to `fallback`) and its old path when it differs.
fn delta_paths(delta: Option<&git2::DiffDelta<'_>>, fallback: &str) -> (String, Option<String>) {
    let utf8 = |file: git2::DiffFile<'_>| {
        file.path_bytes()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .map(str::to_string)
    };
    let Some(delta) = delta else {
        return (fallback.to_string(), None);
    };
    let new = utf8(delta.new_file()).unwrap_or_else(|| fallback.to_string());
    let old = utf8(delta.old_file()).filter(|old| *old != new);
    (new, old)
}

/// A rename with edits carries both the renamed and the modified flag; it is a rename.
fn staged_kind(flags: git2::Status) -> Option<StatusKind> {
    if flags.is_index_renamed() {
        Some(StatusKind::Renamed)
    } else if flags.is_index_new() {
        Some(StatusKind::New)
    } else if flags.is_index_modified() {
        Some(StatusKind::Modified)
    } else if flags.is_index_deleted() {
        Some(StatusKind::Deleted)
    } else if flags.is_index_typechange() {
        Some(StatusKind::TypeChange)
    } else {
        None
    }
}

fn unstaged_kind(flags: git2::Status) -> Option<StatusKind> {
    if flags.is_wt_new() {
        Some(StatusKind::New)
    } else if flags.is_wt_modified() {
        Some(StatusKind::Modified)
    } else if flags.is_wt_deleted() {
        Some(StatusKind::Deleted)
    } else if flags.is_wt_renamed() {
        Some(StatusKind::Renamed)
    } else if flags.is_wt_typechange() {
        Some(StatusKind::TypeChange)
    } else {
        None
    }
}
