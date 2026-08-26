use std::sync::mpsc::Sender;

use git_core::reflog::ReflogEntry;

use super::{Command, WorkerHandle};

pub(super) fn list_refs(repo: &git2::Repository, reply: Sender<Result<Vec<String>, String>>) {
    let result = git_core::reflog::list_reflog_refs(repo).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn get(
    repo: &git2::Repository,
    reference: String,
    reply: Sender<Result<Vec<ReflogEntry>, String>>,
) {
    let result = git_core::reflog::read_reflog(repo, &reference).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn restore(
    repo: &git2::Repository,
    reference: String,
    new_id: String,
    reply: Sender<Result<(), String>>,
) {
    let result = git_core::reflog::restore_reflog_entry(repo, &reference, &new_id)
        .map_err(|error| error.to_string());
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn list_reflog_refs(&self) -> Result<Vec<String>, String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListReflogRefs { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_reflog(&self, reference: String) -> Result<Vec<ReflogEntry>, String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetReflog {
                reference,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn restore_reflog_entry(&self, reference: String, new_id: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::RestoreReflogEntry {
                reference,
                new_id,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
