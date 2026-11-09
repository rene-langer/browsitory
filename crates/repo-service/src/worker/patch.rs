//! #448: a commit's file or hunk reverted or applied in the working tree, patch text, and a
//! patch file applied. See `git_core::patch`.
use std::sync::mpsc::Sender;

use git_core::patch::{ApplyTarget, CommitHunk};

use super::{Command, WorkerHandle};

/// A commit's file (or one hunk of it) to revert or apply in the working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitChange {
    pub commit_id: String,
    pub path: String,
    /// The rename's old path, when the commit view paired one.
    pub old_path: Option<String>,
    /// Only this hunk of the file's diff; `None` for the whole file.
    pub hunk: Option<CommitHunk>,
    /// Revert (undo the change) rather than apply it.
    pub reverse: bool,
    /// Also stage the result (`git apply --index`).
    pub stage: bool,
}

/// What "Save as patch…" writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchSource {
    /// Commits, oldest first, as `git format-patch` mails.
    Commits(Vec<String>),
    /// One file's change in a commit.
    CommitFile {
        commit_id: String,
        path: String,
        old_path: Option<String>,
    },
    /// The staged changes.
    Staged,
}

pub(super) fn apply_commit_change(
    repo: &git2::Repository,
    change: CommitChange,
    reply: Sender<Result<(), String>>,
) {
    let target = if change.stage {
        ApplyTarget::WorkTreeAndIndex
    } else {
        ApplyTarget::WorkTree
    };
    let _ = reply.send(
        git_core::patch::apply_commit_change(
            repo,
            &change.commit_id,
            &change.path,
            change.old_path.as_deref(),
            change.hunk,
            change.reverse,
            target,
        )
        .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_patch(
    repo: &git2::Repository,
    source: PatchSource,
    reply: Sender<Result<Vec<u8>, String>>,
) {
    let result = match source {
        PatchSource::Commits(ids) => git_core::patch::commits_patch(repo, &ids),
        PatchSource::CommitFile {
            commit_id,
            path,
            old_path,
        } => git_core::patch::commit_file_patch(repo, &commit_id, &path, old_path.as_deref()),
        PatchSource::Staged => git_core::patch::staged_patch(repo),
    };
    let _ = reply.send(result.map_err(|error| error.to_string()));
}

pub(super) fn apply_patch(
    repo: &git2::Repository,
    content: Vec<u8>,
    reply: Sender<Result<(), String>>,
) {
    let _ =
        reply.send(git_core::patch::apply_patch(repo, &content).map_err(|error| error.to_string()));
}

impl WorkerHandle {
    /// Reverts or applies `change` in the working tree; never commits. Recorded for Undo.
    pub fn apply_commit_change(&self, change: CommitChange) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ApplyCommitChange { change, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// The patch text of `source`.
    pub fn get_patch(&self, source: PatchSource) -> Result<Vec<u8>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetPatch { source, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// Applies a patch file's `content` to the working tree, all of it or nothing. Recorded for
    /// Undo.
    pub fn apply_patch(&self, content: Vec<u8>) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ApplyPatch { content, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
