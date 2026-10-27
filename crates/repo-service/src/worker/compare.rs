//! Read-only compares: two commits, a commit and the working tree, and a branch since it
//! diverged from HEAD. None of these record an undo entry.

use std::sync::mpsc::Sender;

use git_core::diff::{ChangedFile, CompareRange, DiffHunk, DiffOptions};

use super::{Command, WorkerHandle};

pub(super) fn get_range_files(
    repo: &git2::Repository,
    from_id: String,
    to_id: String,
    reply: Sender<Result<Vec<ChangedFile>, String>>,
) {
    let _ = reply.send(
        git_core::diff::range_diff_files(repo, &from_id, &to_id).map_err(|error| error.to_string()),
    );
}

pub(super) fn get_range_diff(
    repo: &git2::Repository,
    from_id: String,
    to_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply.send(
        git_core::diff::range_diff(repo, &from_id, &to_id, &path, old_path.as_deref(), &opts)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_working_tree_compare_files(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<Vec<ChangedFile>, String>>,
) {
    let _ = reply.send(
        git_core::diff::compare_working_tree_files(repo, &commit_id)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_working_tree_compare_diff(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply.send(
        git_core::diff::compare_working_tree(repo, &commit_id, &path, old_path.as_deref(), &opts)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_branch_compare_range(
    repo: &git2::Repository,
    ref_name: String,
    reply: Sender<Result<CompareRange, String>>,
) {
    let _ = reply.send(
        git_core::diff::branch_compare_range(repo, &ref_name).map_err(|error| error.to_string()),
    );
}

impl WorkerHandle {
    fn request<T>(
        &self,
        command: impl FnOnce(Sender<Result<T, String>>) -> Command,
    ) -> Result<T, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(command(tx))
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_range_files(
        &self,
        from_id: String,
        to_id: String,
    ) -> Result<Vec<ChangedFile>, String> {
        self.request(|reply| Command::GetRangeFiles {
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
    ) -> Result<Vec<DiffHunk>, String> {
        self.request(|reply| Command::GetRangeDiff {
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
    ) -> Result<Vec<ChangedFile>, String> {
        self.request(|reply| Command::GetWorkingTreeCompareFiles { commit_id, reply })
    }

    pub fn get_working_tree_compare_diff(
        &self,
        commit_id: String,
        path: String,
        old_path: Option<String>,
        opts: DiffOptions,
    ) -> Result<Vec<DiffHunk>, String> {
        self.request(|reply| Command::GetWorkingTreeCompareDiff {
            commit_id,
            path,
            old_path,
            opts,
            reply,
        })
    }

    pub fn get_branch_compare_range(&self, ref_name: String) -> Result<CompareRange, String> {
        self.request(|reply| Command::GetBranchCompareRange { ref_name, reply })
    }
}
