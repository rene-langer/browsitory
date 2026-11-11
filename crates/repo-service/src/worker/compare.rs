//! Read-only compares: two commits, a commit and the working tree, and a branch since it
//! diverged from HEAD. None of these record an undo entry.

use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;

use git_core::diff::{ChangedFile, CompareRange, DiffHunk, DiffOptions};

use super::{Command, ReadHandle};

pub(super) fn get_range_files(
    repo: &git2::Repository,
    from_id: String,
    to_id: String,
    reply: Sender<Result<Vec<ChangedFile>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::diff::range_diff_files(repo, &from_id, &to_id).map_err(ErrorDto::from_error),
    );
}

pub(super) fn get_range_diff(
    repo: &git2::Repository,
    from_id: String,
    to_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::diff::range_diff(repo, &from_id, &to_id, &path, old_path.as_deref(), &opts)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn get_working_tree_compare_files(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<Vec<ChangedFile>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::diff::compare_working_tree_files(repo, &commit_id).map_err(ErrorDto::from_error),
    );
}

pub(super) fn get_working_tree_compare_diff(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::diff::compare_working_tree(repo, &commit_id, &path, old_path.as_deref(), &opts)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn get_branch_compare_range(
    repo: &git2::Repository,
    ref_name: String,
    reply: Sender<Result<CompareRange, ErrorDto>>,
) {
    let _ = reply
        .send(git_core::diff::branch_compare_range(repo, &ref_name).map_err(ErrorDto::from_error));
}

impl ReadHandle {
    pub fn get_range_files(
        &self,
        from_id: String,
        to_id: String,
    ) -> Result<Vec<ChangedFile>, ErrorDto> {
        self.read(|reply| Command::GetRangeFiles {
            from_id,
            to_id,
            reply,
        })
    }

    pub fn get_range_diff(
        &self,
        from_id: String,
        to_id: String,
        path: String,
        old_path: Option<String>,
        opts: DiffOptions,
    ) -> Result<Vec<DiffHunk>, ErrorDto> {
        self.read(|reply| Command::GetRangeDiff {
            from_id,
            to_id,
            path,
            old_path,
            opts,
            reply,
        })
    }

    pub fn get_working_tree_compare_files(
        &self,
        commit_id: String,
    ) -> Result<Vec<ChangedFile>, ErrorDto> {
        self.read(|reply| Command::GetWorkingTreeCompareFiles { commit_id, reply })
    }

    pub fn get_working_tree_compare_diff(
        &self,
        commit_id: String,
        path: String,
        old_path: Option<String>,
        opts: DiffOptions,
    ) -> Result<Vec<DiffHunk>, ErrorDto> {
        self.read(|reply| Command::GetWorkingTreeCompareDiff {
            commit_id,
            path,
            old_path,
            opts,
            reply,
        })
    }

    pub fn get_branch_compare_range(&self, ref_name: String) -> Result<CompareRange, ErrorDto> {
        self.read(|reply| Command::GetBranchCompareRange { ref_name, reply })
    }
}
