use thiserror::Error;

#[derive(Debug, Error)]
pub enum StatusError {
    #[error("failed to read repository status: {0}")]
    Read(#[from] git2::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

pub fn status(repo: &git2::Repository) -> Result<Vec<StatusEntry>, StatusError> {
    // Explicit options, because libgit2's defaults both do too much and too little: they
    // scan ignored files (whose flags then resolve to `None`/`None` and get dropped below)
    // and they leave rename detection off. Renames are detected between HEAD and the index
    // only, like `git status`: a file moved but not staged is "deleted" + "new", so per-file
    // stage and discard keep acting on one path (#137).
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(false);

    let statuses = repo.statuses(Some(&mut options))?;
    let mut entries = Vec::new();

    for entry in statuses.iter() {
        // `entry.path()` is a rename's *old* path; each side's delta names its own paths.
        let Ok(path) = entry.path() else { continue };
        let flags = entry.status();

        if flags.is_conflicted() {
            entries.push(StatusEntry {
                path: path.to_string(),
                old_path: None,
                staged: false,
                kind: StatusKind::Conflicted,
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
            });
        }
    }

    Ok(entries)
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
