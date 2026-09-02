use std::sync::mpsc::Sender;

use git_core::remote::TagInfo;

use super::{Command, WorkerHandle};

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<TagInfo>, String>>) {
    let result = git_core::remote::list_tags(repo).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn create(
    repo: &git2::Repository,
    name: String,
    message: Option<String>,
    reply: Sender<Result<(), String>>,
) {
    let result = git_core::remote::create_tag(repo, &name, message.as_deref())
        .map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn delete(repo: &git2::Repository, name: String, reply: Sender<Result<(), String>>) {
    let result = git_core::remote::delete_tag(repo, &name).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn list_tags(&self) -> Result<Vec<TagInfo>, String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListTags { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn create_tag(&self, name: String, message: Option<String>) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::CreateTag {
                name,
                message,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn delete_tag(&self, name: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::DeleteTag {
                name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
