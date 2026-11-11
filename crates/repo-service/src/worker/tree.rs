//! A commit's file tree (#446): listing one folder or a filter's matches, a file's content at
//! that commit, saving it elsewhere, and restoring it to the work tree. Only the restore changes
//! the repository; its policy records it as an undo entry.

use crate::wire::ErrorDto;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use git_core::tree::{FileContent, TreeListing, MAX_TREE_MATCHES, MAX_VIEW_BYTES};

use super::{Command, ReadHandle, WorkerHandle};

pub(super) fn get_commit_tree(
    repo: &git2::Repository,
    commit_id: String,
    dir: String,
    filter: Option<String>,
    reply: Sender<Result<TreeListing, ErrorDto>>,
) {
    let result = match filter.as_deref().map(str::trim) {
        Some(query) if !query.is_empty() => {
            git_core::tree::find_in_tree(repo, &commit_id, query, MAX_TREE_MATCHES)
        }
        _ => git_core::tree::list_tree(repo, &commit_id, &dir),
    };
    let _ = reply.send(result.map_err(ErrorDto::from_error));
}

pub(super) fn get_file_at_commit(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    reply: Sender<Result<FileContent, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::tree::file_at_commit(repo, &commit_id, &path, MAX_VIEW_BYTES)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn save_file_version(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    destination: PathBuf,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::tree::save_file_version(repo, &commit_id, &path, &destination)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn restore_file_version(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::stage::restore_file_version(repo, &commit_id, &path)
            .map_err(ErrorDto::from_error),
    );
}

impl WorkerHandle {
    /// Restores `path` as it is in `commit_id` to the work tree (one undo entry).
    pub fn restore_file_version(&self, commit_id: String, path: String) -> Result<(), ErrorDto> {
        self.request(|reply| Command::RestoreFileVersion {
            commit_id,
            path,
            reply,
        })
    }
}

impl ReadHandle {
    /// The entries directly in `dir` (`""`: the root) of `commit_id`'s tree, or, with a
    /// non-blank `filter`, every file whose path contains it (at most `MAX_TREE_MATCHES`).
    pub fn get_commit_tree(
        &self,
        commit_id: String,
        dir: String,
        filter: Option<String>,
    ) -> Result<TreeListing, ErrorDto> {
        self.read(|reply| Command::GetCommitTree {
            commit_id,
            dir,
            filter,
            reply,
        })
    }

    /// `path` at `commit_id` for the read-only viewer (`MAX_VIEW_BYTES` at most).
    pub fn get_file_at_commit(
        &self,
        commit_id: String,
        path: String,
    ) -> Result<FileContent, ErrorDto> {
        self.read(|reply| Command::GetFileAtCommit {
            commit_id,
            path,
            reply,
        })
    }

    /// Writes `path` at `commit_id` to `destination`, a path the user picked in a save dialog.
    pub fn save_file_version(
        &self,
        commit_id: String,
        path: String,
        destination: PathBuf,
    ) -> Result<(), ErrorDto> {
        self.read(|reply| Command::SaveFileVersion {
            commit_id,
            path,
            destination,
            reply,
        })
    }
}
