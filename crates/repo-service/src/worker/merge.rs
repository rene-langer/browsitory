use super::{Command, WorkerHandle};
use git_core::merge::{
    ConflictSegment, ConflictSide, FileConflictChoice, MarkResolvedOutcome, MergeOptions,
    MergeOutcome,
};
use std::sync::mpsc::Sender;
/// A merge this worker started and that stopped without conflicts (`MergeOutcome::Merged`),
/// remembered so that the commit concluding it runs `git merge`'s hooks
/// (`CommitHookSet::CleanMerge`: `pre-merge-commit`, then `post-merge 0` after the commit,
/// #124). It is only trusted while the repository still shows that very merge: state `Merge`,
/// the same HEAD and the same `MERGE_HEAD` commits.
///
/// Kept in memory only (D2-A). After a restart, or for a merge started by the other window or
/// outside Browsitory, there is none, and the commit runs `git commit`'s hooks, which is what
/// git does for `git merge --no-commit` followed by `git commit`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CleanMerge {
    head: git2::Oid,
    merge_heads: Vec<git2::Oid>,
}

impl CleanMerge {
    /// The merge `repo` shows now, if any.
    pub(super) fn capture(repo: &mut git2::Repository) -> Option<Self> {
        if repo.state() != git2::RepositoryState::Merge {
            return None;
        }
        let head = repo.head().ok()?.peel_to_commit().ok()?.id();
        let mut merge_heads = Vec::new();
        repo.mergehead_foreach(|oid| {
            merge_heads.push(*oid);
            true
        })
        .ok()?;
        Some(Self { head, merge_heads })
    }

    /// `repo` still shows this merge.
    pub(super) fn matches(&self, repo: &mut git2::Repository) -> bool {
        Self::capture(repo).as_ref() == Some(self)
    }

    /// The hook set for the commit concluding this merge. `commit_with` checks the repository
    /// against it before and after the hooks (#183).
    pub(super) fn hook_set(&self) -> git_core::commit::CommitHookSet {
        git_core::commit::CommitHookSet::CleanMerge {
            head: self.head,
            merge_heads: self.merge_heads.clone(),
        }
    }
}

/// Starts a merge and replies; returns the result too, so the loop can remember a clean merge.
/// A fast-forward's or squash's `post-merge` runs reported and cancellable on `op` (as
/// `HookSource::Merge`) before the reply; a cancel stops only the hook (#185).
pub(super) fn start(
    repo: &git2::Repository,
    branch_name: String,
    options: MergeOptions,
    op: super::remote::TransferOp<'_>,
    events: Sender<super::HookEvent>,
    reply: Sender<Result<MergeOutcome, String>>,
) -> Result<MergeOutcome, String> {
    let result = git_core::merge::start_merge(repo, &branch_name, options)
        .map_err(|error| error.to_string());
    // git runs post-merge after a merge that completed (or a squash, with "1"); not after a
    // conflicted or no-op one. A clean non-fast-forward merge (`Merged`) is only staged here:
    // its post-merge runs after the commit that concludes it (`status::commit`).
    let squash = match result {
        Ok(MergeOutcome::FastForwarded) => Some("0"),
        Ok(MergeOutcome::Squashed) => Some("1"),
        _ => None,
    };
    if let Some(squash) = squash {
        super::hooks::run_post_hook_on(
            repo,
            "post-merge",
            &[squash],
            &op,
            events,
            super::hooks::HookSource::Merge,
        );
    }
    op.clear();
    let _ = reply.send(result.clone());
    result
}
pub(super) fn defaults(repo: &git2::Repository, reply: Sender<Result<MergeOptions, String>>) {
    let _ = reply.send(Ok(git_core::merge::merge_defaults(repo)));
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
pub(super) fn mark_resolved(
    repo: &git2::Repository,
    path: String,
    force: bool,
    reply: Sender<Result<MarkResolvedOutcome, String>>,
) {
    let _ = reply.send(
        git_core::merge::mark_resolved(repo, &path, force).map_err(|error| error.to_string()),
    );
}
pub(super) fn resolve_whole_file(
    repo: &git2::Repository,
    path: String,
    side: ConflictSide,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::merge::resolve_whole_file(repo, &path, side).map_err(|error| error.to_string()),
    );
}
impl WorkerHandle {
    /// Resolves a conflicted file by taking one side's whole file (`git checkout --ours/--theirs`
    /// + `git add`).
    pub fn resolve_whole_file(&self, path: String, side: ConflictSide) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ResolveWholeFile {
                path,
                side,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// Stages a conflicted file; without `force`, reports leftover conflict markers instead.
    pub fn mark_resolved(&self, path: String, force: bool) -> Result<MarkResolvedOutcome, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::MarkResolved {
                path,
                force,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// Merges `branch_name` (a local branch name or a full ref) with `options`. The
    /// `post-merge` of a fast-forward or squash is reported on `events` (`HookSource::Merge`)
    /// under a `merge-<n>` id that `cancel_transfer` stops (#185).
    pub fn start_merge(
        &self,
        branch_name: String,
        options: MergeOptions,
        events: Sender<super::HookEvent>,
    ) -> Result<MergeOutcome, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::StartMerge {
                branch_name,
                options,
                operation_id: super::remote::next_operation_id("merge"),
                events,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    /// The merge dialog's default options, from `merge.ff`.
    pub fn merge_defaults(&self) -> Result<MergeOptions, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetMergeDefaults { reply: tx })
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
