use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;

use git_core::reflog::ReflogEntry;

use super::{Command, ReadHandle, WorkerHandle};

pub(super) fn list_refs(repo: &git2::Repository, reply: Sender<Result<Vec<String>, ErrorDto>>) {
    let result = git_core::reflog::list_reflog_refs(repo).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn get(
    repo: &git2::Repository,
    reference: String,
    reply: Sender<Result<Vec<ReflogEntry>, ErrorDto>>,
) {
    let result = git_core::reflog::read_reflog(repo, &reference).map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

pub(super) fn restore(
    repo: &git2::Repository,
    reference: String,
    new_id: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let result = git_core::reflog::restore_reflog_entry(repo, &reference, &new_id)
        .map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn restore_reflog_entry(&self, reference: String, new_id: String) -> Result<(), ErrorDto> {
        self.request(|reply_tx| Command::RestoreReflogEntry {
            reference,
            new_id,
            reply: reply_tx,
        })
    }
}

impl ReadHandle {
    pub fn list_reflog_refs(&self) -> Result<Vec<String>, ErrorDto> {
        self.read(|reply_tx| Command::ListReflogRefs { reply: reply_tx })
    }

    pub fn get_reflog(&self, reference: String) -> Result<Vec<ReflogEntry>, ErrorDto> {
        self.read(|reply_tx| Command::GetReflog {
            reference,
            reply: reply_tx,
        })
    }
}
