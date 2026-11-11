//! #448: a commit's file or hunk reverted or applied in the working tree, patch text, and a
//! patch file applied. See `git_core::patch`.
use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;

use git_core::patch::{ApplyTarget, CommitHunk, PatchApply, PatchApplyOutcome, PatchPreview};

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
    reply: Sender<Result<(), ErrorDto>>,
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
        .map_err(ErrorDto::from_error),
    );
}

pub(super) fn get_patch(
    repo: &git2::Repository,
    source: PatchSource,
    reply: Sender<Result<Vec<u8>, ErrorDto>>,
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
    let _ = reply.send(result.map_err(ErrorDto::from_error));
}

/// Applies `content`; as commits with `git am`'s hooks (#747), reported on `events` as
/// `HookSource::Patch` and cancellable through `op`. A hook rejecting a mail is an
/// `Ok(PatchApplyOutcome::HookFailed)` the UI renders, like a commit's failing hook.
pub(super) fn apply_patch(
    repo: &git2::Repository,
    content: Vec<u8>,
    how: PatchApply,
    op: super::remote::TransferOp,
    events: Sender<super::HookEvent>,
    reply: Sender<Result<PatchApplyOutcome, ErrorDto>>,
) {
    let cancel = op.cancel_check();
    let mut reporter = super::hooks::ChannelHookReporter {
        events,
        operation_id: op.id.clone(),
        source: super::hooks::HookSource::Patch,
    };
    let result = git_core::patch::apply_patch_with(
        repo,
        &content,
        how,
        git_core::hooks::HookOptions {
            cancel: Some(&cancel),
            reporter: Some(&mut reporter),
            ..Default::default()
        },
    );
    op.clear();
    let _ = reply.send(PatchApplyOutcome::from_result(result).map_err(ErrorDto::from_error));
}

pub(super) fn preview_patch(
    repo: &git2::Repository,
    content: Vec<u8>,
    reply: Sender<Result<PatchPreview, ErrorDto>>,
) {
    let _ =
        reply.send(git_core::patch::preview_patch(repo, &content).map_err(ErrorDto::from_error));
}

impl WorkerHandle {
    /// Reverts or applies `change` in the working tree; never commits. Recorded for Undo.
    pub fn apply_commit_change(&self, change: CommitChange) -> Result<(), ErrorDto> {
        self.request(|tx| Command::ApplyCommitChange { change, reply: tx })
    }

    /// The patch text of `source`.
    pub fn get_patch(&self, source: PatchSource) -> Result<Vec<u8>, ErrorDto> {
        self.request(|tx| Command::GetPatch { source, reply: tx })
    }

    /// Applies a patch file's `content` to the working tree (`how`: also to the index, or as
    /// commits with the mails' authorship), all of it or nothing. Recorded for Undo. As commits
    /// it runs `git am`'s hooks (#747), unreported; see `apply_patch_reported`.
    pub fn apply_patch(
        &self,
        content: Vec<u8>,
        how: PatchApply,
    ) -> Result<PatchApplyOutcome, ErrorDto> {
        let (events, _) = std::sync::mpsc::channel();
        self.apply_patch_reported(content, how, events)
    }

    /// `apply_patch`, reporting the hooks on `events` (`HookSource::Patch`) under an `apply-<n>`
    /// id that `cancel_transfer` stops (#747).
    pub fn apply_patch_reported(
        &self,
        content: Vec<u8>,
        how: PatchApply,
        events: Sender<super::HookEvent>,
    ) -> Result<PatchApplyOutcome, ErrorDto> {
        self.request(|reply| Command::ApplyPatch {
            content,
            how,
            operation_id: self.cancelled.allocate("apply"),
            events,
            reply,
        })
    }

    /// What a patch file's `content` holds and whether it applies; changes nothing.
    pub fn preview_patch(&self, content: Vec<u8>) -> Result<PatchPreview, ErrorDto> {
        self.request(|tx| Command::PreviewPatch { content, reply: tx })
    }
}
