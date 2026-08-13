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
    pub path: String,
    pub staged: bool,
    pub kind: StatusKind,
}

pub fn status(repo: &git2::Repository) -> Result<Vec<StatusEntry>, StatusError> {
    // Explicit options, because libgit2's defaults both do too much and too little: they
    // scan ignored files (whose flags then resolve to `None`/`None` and get dropped below)
    // and they leave rename detection off, which would make `StatusKind::Renamed`
    // unreachable — renames would surface as a Deleted + New pair instead.
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo.statuses(Some(&mut options))?;
    let mut entries = Vec::new();

    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        let flags = entry.status();

        if flags.is_conflicted() {
            entries.push(StatusEntry {
                path: path.to_string(),
                staged: false,
                kind: StatusKind::Conflicted,
            });
            continue;
        }

        if let Some(kind) = staged_kind(flags) {
            entries.push(StatusEntry {
                path: path.to_string(),
                staged: true,
                kind,
            });
        }
        if let Some(kind) = unstaged_kind(flags) {
            entries.push(StatusEntry {
                path: path.to_string(),
                staged: false,
                kind,
            });
        }
    }

    Ok(entries)
}

fn staged_kind(flags: git2::Status) -> Option<StatusKind> {
    if flags.is_index_new() {
        Some(StatusKind::New)
    } else if flags.is_index_modified() {
        Some(StatusKind::Modified)
    } else if flags.is_index_deleted() {
        Some(StatusKind::Deleted)
    } else if flags.is_index_renamed() {
        Some(StatusKind::Renamed)
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
