use git2::{Repository, Status, StatusOptions};

use crate::repo::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Modified,
    New,
    Deleted,
    Renamed,
    Typechange,
    Conflicted,
}

#[derive(Debug, Clone)]
pub struct FileStatus {
    pub path: String,
    pub staged: Option<FileState>,
    pub unstaged: Option<FileState>,
}

/// Equivalent of the old `statusMatrix` call: one entry per path that
/// differs from HEAD and/or the working tree, split into its staged
/// (index-vs-HEAD) and unstaged (workdir-vs-index) states.
pub fn status(repo: &Repository) -> Result<Vec<FileStatus>> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo.statuses(Some(&mut opts))?;
    let mut out = Vec::with_capacity(statuses.len());
    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        let s = entry.status();
        out.push(FileStatus {
            path: path.to_string(),
            staged: staged_state(s),
            unstaged: unstaged_state(s),
        });
    }
    Ok(out)
}

fn staged_state(s: Status) -> Option<FileState> {
    if s.contains(Status::CONFLICTED) {
        Some(FileState::Conflicted)
    } else if s.contains(Status::INDEX_NEW) {
        Some(FileState::New)
    } else if s.contains(Status::INDEX_MODIFIED) {
        Some(FileState::Modified)
    } else if s.contains(Status::INDEX_DELETED) {
        Some(FileState::Deleted)
    } else if s.contains(Status::INDEX_RENAMED) {
        Some(FileState::Renamed)
    } else if s.contains(Status::INDEX_TYPECHANGE) {
        Some(FileState::Typechange)
    } else {
        None
    }
}

fn unstaged_state(s: Status) -> Option<FileState> {
    if s.contains(Status::WT_NEW) {
        Some(FileState::New)
    } else if s.contains(Status::WT_MODIFIED) {
        Some(FileState::Modified)
    } else if s.contains(Status::WT_DELETED) {
        Some(FileState::Deleted)
    } else if s.contains(Status::WT_RENAMED) {
        Some(FileState::Renamed)
    } else if s.contains(Status::WT_TYPECHANGE) {
        Some(FileState::Typechange)
    } else {
        None
    }
}
