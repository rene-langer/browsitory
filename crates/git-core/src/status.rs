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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: String,
    pub staged: bool,
    pub kind: StatusKind,
}

pub fn status(repo: &git2::Repository) -> Result<Vec<StatusEntry>, StatusError> {
    let statuses = repo.statuses(None)?;
    let mut entries = Vec::new();

    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        let flags = entry.status();

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
