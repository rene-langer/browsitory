use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;

use git_core::submodule::SubmoduleInfo;

use super::{Command, WorkerHandle};

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<SubmoduleInfo>, ErrorDto>>) {
    let result = git_core::submodule::list_submodules(repo).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn init(repo: &git2::Repository, path: String, reply: Sender<Result<(), ErrorDto>>) {
    let result = git_core::submodule::init_submodule(repo, &path).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn update(
    repo: &git2::Repository,
    path: String,
    recursive: bool,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let result =
        git_core::submodule::update_submodule(repo, &path, recursive).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn list_submodules(&self) -> Result<Vec<SubmoduleInfo>, ErrorDto> {
        self.request(|reply_tx| Command::ListSubmodules { reply: reply_tx })
    }

    pub fn init_submodule(&self, path: String) -> Result<(), ErrorDto> {
        self.request(|reply_tx| Command::InitSubmodule {
            path,
            reply: reply_tx,
        })
    }

    pub fn update_submodule(&self, path: String, recursive: bool) -> Result<(), ErrorDto> {
        self.request(|reply_tx| Command::UpdateSubmodule {
            path,
            recursive,
            reply: reply_tx,
        })
    }
}
