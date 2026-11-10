use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;

use git_core::remote::TagInfo;

use super::{Command, WorkerHandle};

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<TagInfo>, ErrorDto>>) {
    let result = git_core::remote::list_tags(repo).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn create(
    repo: &git2::Repository,
    name: String,
    message: Option<String>,
    target: Option<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let result = git_core::remote::create_tag(repo, &name, message.as_deref(), target.as_deref())
        .map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn delete(repo: &git2::Repository, name: String, reply: Sender<Result<(), ErrorDto>>) {
    let result = git_core::remote::delete_tag(repo, &name).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn list_tags(&self) -> Result<Vec<TagInfo>, ErrorDto> {
        self.request(|reply_tx| Command::ListTags { reply: reply_tx })
    }

    /// Tags `target` (a full commit id), or HEAD's commit when `None`.
    pub fn create_tag(
        &self,
        name: String,
        message: Option<String>,
        target: Option<String>,
    ) -> Result<(), ErrorDto> {
        self.request(|reply_tx| Command::CreateTag {
            name,
            message,
            target,
            reply: reply_tx,
        })
    }

    pub fn delete_tag(&self, name: String) -> Result<(), ErrorDto> {
        self.request(|reply_tx| Command::DeleteTag {
            name,
            reply: reply_tx,
        })
    }
}
