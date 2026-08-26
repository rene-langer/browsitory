use std::sync::mpsc::Sender;

use git_core::submodule::SubmoduleInfo;

use super::{Command, WorkerHandle};

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<SubmoduleInfo>, String>>) {
    let result = git_core::submodule::list_submodules(repo).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn init(repo: &git2::Repository, path: String, reply: Sender<Result<(), String>>) {
    let result =
        git_core::submodule::init_submodule(repo, &path).map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn update(
    repo: &git2::Repository,
    path: String,
    recursive: bool,
    reply: Sender<Result<(), String>>,
) {
    let result = git_core::submodule::update_submodule(repo, &path, recursive)
        .map_err(|error| error.to_string());
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn list_submodules(&self) -> Result<Vec<SubmoduleInfo>, String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListSubmodules { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn init_submodule(&self, path: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::InitSubmodule {
                path,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn update_submodule(&self, path: String, recursive: bool) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::UpdateSubmodule {
                path,
                recursive,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
