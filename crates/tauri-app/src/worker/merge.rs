use super::{Command, WorkerHandle};
use git_core::merge::{ConflictSegment, FileConflictChoice, MergeOutcome};
use std::sync::mpsc::Sender;
pub(super) fn start(
    repo: &git2::Repository,
    branch_name: String,
    reply: Sender<Result<MergeOutcome, String>>,
) {
    let _ = reply
        .send(git_core::merge::start_merge(repo, &branch_name).map_err(|error| error.to_string()));
}
pub(super) fn conflict_hunks(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<Vec<ConflictSegment>, String>>,
) {
    let _ =
        reply.send(git_core::merge::conflict_hunks(repo, &path).map_err(|error| error.to_string()));
}
pub(super) fn resolve(
    repo: &git2::Repository,
    path: String,
    resolved_content: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::merge::resolve_conflict(repo, &path, &resolved_content)
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn abort(repo: &git2::Repository, reply: Sender<Result<(), String>>) {
    let _ = reply.send(git_core::merge::abort_merge(repo).map_err(|error| error.to_string()));
}
pub(super) fn message(repo: &git2::Repository, reply: Sender<Result<Option<String>, String>>) {
    let _ = reply.send(Ok(git_core::merge::merge_message(repo)));
}
pub(super) fn resolve_add_delete(
    repo: &git2::Repository,
    path: String,
    choice: FileConflictChoice,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::merge::resolve_add_delete_conflict(repo, &path, choice)
            .map_err(|error| error.to_string()),
    );
}
impl WorkerHandle {
    #[allow(dead_code)]
    pub fn start_merge(&self, branch_name: String) -> Result<MergeOutcome, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::StartMerge {
                branch_name,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    #[allow(dead_code)]
    pub fn get_conflict_hunks(&self, path: String) -> Result<Vec<ConflictSegment>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetConflictHunks { path, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    #[allow(dead_code)]
    pub fn resolve_conflict(&self, path: String, resolved_content: String) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ResolveConflict {
                path,
                resolved_content,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    #[allow(dead_code)]
    pub fn abort_merge(&self) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::AbortMerge { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    #[allow(dead_code)]
    pub fn get_merge_message(&self) -> Result<Option<String>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetMergeMessage { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn resolve_add_delete_conflict(
        &self,
        path: String,
        choice: FileConflictChoice,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ResolveAddDeleteConflict {
                path,
                choice,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
