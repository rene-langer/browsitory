use std::sync::mpsc::Sender;

use git_core::stash::StashEntry;

use super::{Command, WorkerHandle};

pub(super) fn list(repo: &mut git2::Repository, reply: Sender<Result<Vec<StashEntry>, String>>) {
    let result = git_core::stash::list_stashes(repo).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn save(repo: &mut git2::Repository, reply: Sender<Result<(), String>>) {
    let result = git_core::stash::save_stash(repo).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn apply(repo: &mut git2::Repository, index: usize, reply: Sender<Result<(), String>>) {
    let result = git_core::stash::apply_stash(repo, index).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn drop(repo: &mut git2::Repository, index: usize, reply: Sender<Result<(), String>>) {
    let result = git_core::stash::drop_stash(repo, index).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn list_stashes(&self) -> Result<Vec<StashEntry>, String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListStashes { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn save_stash(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SaveStash { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn apply_stash(&self, index: usize) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ApplyStash {
                index,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn drop_stash(&self, index: usize) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::DropStash {
                index,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
