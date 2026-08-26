use super::{Command, WorkerHandle};
use git_core::blame::BlameLine;
use git_core::diff::DiffHunk;
use git_core::graph::GraphCommit;
use git_core::status::StatusEntry;
use std::sync::mpsc::Sender;

pub(super) fn get_status(repo: &git2::Repository, reply: Sender<Result<Vec<StatusEntry>, String>>) {
    let _ = reply.send(git_core::status::status(repo).map_err(|error| error.to_string()));
}

pub(super) fn get_commit_graph(
    repo: &git2::Repository,
    limit: usize,
    selected_branches: Option<Vec<String>>,
    reply: Sender<Result<Vec<GraphCommit>, String>>,
) {
    let _ = reply.send(
        git_core::graph::graph_log(repo, limit, selected_branches.as_deref())
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_working_diff(
    repo: &git2::Repository,
    path: String,
    staged: bool,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply
        .send(git_core::diff::working_diff(repo, &path, staged).map_err(|error| error.to_string()));
}

pub(super) fn get_commit_diff(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply.send(
        git_core::diff::commit_diff(repo, &commit_id, &path).map_err(|error| error.to_string()),
    );
}

pub(super) fn get_commit_files(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<Vec<String>, String>>,
) {
    let _ = reply
        .send(git_core::diff::commit_files(repo, &commit_id).map_err(|error| error.to_string()));
}

pub(super) fn get_blame(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    reply: Sender<Result<Vec<BlameLine>, String>>,
) {
    let _ = reply.send(
        git_core::blame::blame_file(repo, &commit_id, &path).map_err(|error| error.to_string()),
    );
}

pub(super) fn stage_file(repo: &git2::Repository, path: String, reply: Sender<Result<(), String>>) {
    let _ = reply.send(git_core::stage::stage_file(repo, &path).map_err(|error| error.to_string()));
}

pub(super) fn unstage_file(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<(), String>>,
) {
    let _ =
        reply.send(git_core::stage::unstage_file(repo, &path).map_err(|error| error.to_string()));
}

pub(super) fn stage_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::stage_hunk(repo, &path, old_start, new_start)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn unstage_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::unstage_hunk(repo, &path, old_start, new_start)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn discard_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::discard_hunk(repo, &path, old_start, new_start)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn commit(
    repo: &mut git2::Repository,
    message: String,
    reply: Sender<Result<String, String>>,
) {
    let _ = reply.send(git_core::commit::commit(repo, &message).map_err(|error| error.to_string()));
}

impl WorkerHandle {
    pub fn get_status(&self) -> Result<Vec<StatusEntry>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetStatus { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_graph(
        &self,
        limit: usize,
        selected_branches: Option<Vec<String>>,
    ) -> Result<Vec<GraphCommit>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitGraph {
                limit,
                selected_branches,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_working_diff(&self, path: String, staged: bool) -> Result<Vec<DiffHunk>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetWorkingDiff {
                path,
                staged,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_diff(
        &self,
        commit_id: String,
        path: String,
    ) -> Result<Vec<DiffHunk>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitDiff {
                commit_id,
                path,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_files(&self, commit_id: String) -> Result<Vec<String>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitFiles {
                commit_id,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_blame(&self, commit_id: String, path: String) -> Result<Vec<BlameLine>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetBlame {
                commit_id,
                path,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn stage_file(&self, path: String) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::StageFile { path, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn unstage_file(&self, path: String) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::UnstageFile { path, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn stage_hunk(&self, path: String, old_start: u32, new_start: u32) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::StageHunk {
                path,
                old_start,
                new_start,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn unstage_hunk(&self, path: String, old_start: u32, new_start: u32) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::UnstageHunk {
                path,
                old_start,
                new_start,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn discard_hunk(&self, path: String, old_start: u32, new_start: u32) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::DiscardHunk {
                path,
                old_start,
                new_start,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn commit(&self, message: String) -> Result<String, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Commit { message, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
