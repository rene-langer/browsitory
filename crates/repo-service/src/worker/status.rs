use super::{Command, WorkerHandle};
use git_core::blame::BlameLine;
use git_core::diff::DiffHunk;
use git_core::graph::GraphCommit;
use git_core::status::StatusEntry;
use std::sync::mpsc::Sender;

pub(super) fn get_status(repo: &git2::Repository, reply: Sender<Result<Vec<StatusEntry>, String>>) {
    let _ = reply.send(git_core::status::status(repo).map_err(|error| error.to_string()));
}

pub(super) fn get_commit_graph(
    repo: &git2::Repository,
    limit: usize,
    selected_branches: Option<Vec<String>>,
    reply: Sender<Result<Vec<GraphCommit>, String>>,
) {
    let _ = reply.send(
        git_core::graph::graph_log(repo, limit, selected_branches.as_deref())
            .map_err(|error| error.to_string()),
    );
}

/// With `staged` and `old_path` (a staged rename), the HEAD → index diff of both paths with
/// rename detection, so only the edits show (#137).
pub(super) fn get_working_diff(
    repo: &git2::Repository,
    path: String,
    staged: bool,
    opts: git_core::diff::DiffOptions,
    old_path: Option<String>,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let result = match old_path.filter(|_| staged) {
        Some(old_path) => git_core::diff::staged_rename_diff(repo, &old_path, &path, &opts),
        None => git_core::diff::working_diff(repo, &path, staged, &opts),
    };
    let _ = reply.send(result.map_err(|error| error.to_string()));
}

pub(super) fn get_commit_diff(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    opts: git_core::diff::DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply.send(
        git_core::diff::commit_diff(repo, &commit_id, &path, &opts)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn get_commit_files(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<Vec<String>, String>>,
) {
    let _ = reply
        .send(git_core::diff::commit_files(repo, &commit_id).map_err(|error| error.to_string()));
}

pub(super) fn get_commit_signature(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<git_core::signing::CommitSignature, String>>,
) {
    let _ = reply.send(
        git_core::signing::verify_commit(repo, &commit_id).map_err(|error| error.to_string()),
    );
}

pub(super) fn get_commit_message(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<String, String>>,
) {
    let _ = reply
        .send(git_core::graph::commit_message(repo, &commit_id).map_err(|error| error.to_string()));
}

pub(super) fn get_blame(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    reply: Sender<Result<Vec<BlameLine>, String>>,
) {
    let _ = reply.send(
        git_core::blame::blame_file(repo, &commit_id, &path).map_err(|error| error.to_string()),
    );
}

pub(super) fn get_file_history(
    repo: &git2::Repository,
    path: String,
    limit: usize,
    reply: Sender<Result<Vec<git_core::history::FileHistoryEntry>, String>>,
) {
    let _ = reply.send(
        git_core::history::file_history(repo, &path, limit).map_err(|error| error.to_string()),
    );
}

pub(super) fn get_commit_rename_diff(
    repo: &git2::Repository,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: git_core::diff::DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, String>>,
) {
    let _ = reply.send(
        git_core::diff::commit_rename_diff(repo, &commit_id, &old_path, &new_path, &opts)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn stage_file(repo: &git2::Repository, path: String, reply: Sender<Result<(), String>>) {
    let _ = reply.send(git_core::stage::stage_file(repo, &path).map_err(|error| error.to_string()));
}

pub(super) fn unstage_file(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<(), String>>,
) {
    let _ =
        reply.send(git_core::stage::unstage_file(repo, &path).map_err(|error| error.to_string()));
}

pub(super) fn stage_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::stage_hunk(repo, &path, old_start, new_start, context_lines)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn unstage_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::unstage_hunk(repo, &path, old_start, new_start, context_lines)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn discard_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::discard_hunk(repo, &path, old_start, new_start, context_lines)
            .map_err(|error| error.to_string()),
    );
}

// Mirrors its `Command` variant field for field.
#[allow(clippy::too_many_arguments)]
pub(super) fn stage_lines(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::stage_lines(
            repo,
            &path,
            old_start,
            new_start,
            context_lines,
            &line_indices,
            &expected_lines,
        )
        .map_err(|error| error.to_string()),
    );
}

// Mirrors its `Command` variant field for field.
#[allow(clippy::too_many_arguments)]
pub(super) fn unstage_lines(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::unstage_lines(
            repo,
            &path,
            old_start,
            new_start,
            context_lines,
            &line_indices,
            &expected_lines,
        )
        .map_err(|error| error.to_string()),
    );
}

// Mirrors its `Command` variant field for field.
#[allow(clippy::too_many_arguments)]
pub(super) fn discard_lines(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    context_lines: u32,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::discard_lines(
            repo,
            &path,
            old_start,
            new_start,
            context_lines,
            &line_indices,
            &expected_lines,
        )
        .map_err(|error| error.to_string()),
    );
}

pub(super) fn discard_file(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<(), String>>,
) {
    let _ =
        reply.send(git_core::stage::discard_file(repo, &path).map_err(|error| error.to_string()));
}

pub(super) fn discard_all_unstaged(
    repo: &git2::Repository,
    paths: Vec<String>,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::stage::discard_all_unstaged(repo, &paths).map_err(|error| error.to_string()),
    );
}

pub(super) fn ignore_path(
    repo: &git2::Repository,
    pattern: String,
    reply: Sender<Result<(), String>>,
) {
    let _ =
        reply.send(git_core::stage::ignore_path(repo, &pattern).map_err(|error| error.to_string()));
}

/// The reply both commit and amend send: a hook failure is an outcome the UI renders, every
/// other failure stays an error message.
pub(super) fn commit_reply(
    result: Result<String, git_core::commit::CommitError>,
) -> Result<git_core::commit::CommitOutcome, String> {
    use git_core::commit::{CommitError, CommitOutcome};
    match result {
        Ok(commit_id) => Ok(CommitOutcome::Committed { commit_id }),
        Err(CommitError::HookFailed { hook, output }) => {
            Ok(CommitOutcome::HookFailed { hook, output })
        }
        Err(error) => Err(error.to_string()),
    }
}

/// Commits and replies; returns whether a commit landed. With `CommitHookSet::CleanMerge` (the
/// loop remembers this exact merge as clean, see `merge::CleanMerge`) it runs `git merge`'s
/// hooks and, once the commit landed, `post-merge 0`; both only if the merge is still in
/// progress when the commit starts. `post-merge` runs on the commit's operation, before the
/// reply: reported (the commit box shows it running) and cancellable like `post-commit` (#178).
/// Its result, a cancel included, never changes the outcome, and "Skip hooks" doesn't skip it
/// (as `git merge --no-verify`).
#[allow(clippy::too_many_arguments)]
pub(super) fn commit(
    repo: &mut git2::Repository,
    message: String,
    no_verify: bool,
    trailers: Vec<git_core::commit::Trailer>,
    set: git_core::commit::CommitHookSet,
    op: super::remote::TransferOp<'_>,
    events: Sender<super::HookEvent>,
    reply: Sender<Result<git_core::commit::CommitOutcome, String>>,
) -> bool {
    let cancel = op.cancel_check();
    let mut reporter = super::hooks::ChannelHookReporter {
        events,
        operation_id: op.id.clone(),
        source: super::hooks::HookSource::Commit,
    };
    // Before the hooks, so `prepare-commit-msg`/`commit-msg` see them (`git commit --trailer`).
    let result = git_core::commit::append_trailers(&message, &trailers).and_then(|message| {
        git_core::commit::commit_with(
            repo,
            &message,
            set,
            git_core::hooks::HookOptions {
                no_verify,
                cancel: Some(&cancel),
                reporter: Some(&mut reporter),
                ..Default::default()
            },
        )
    });
    let committed = result.is_ok();
    // Only when `git merge`'s hooks really ran: the merge may have been aborted (another
    // window, a terminal) between the loop choosing `set` and the commit. The cancel flag is the
    // commit's: a Cancel that landed after the last pre hook but before the commit stops
    // post-merge at once (still reported); the commit stands either way.
    if result.as_ref().is_ok_and(|committed| committed.merge_hooks) {
        super::hooks::run_post_hook_reported(
            repo,
            "post-merge",
            &["0"],
            &mut git_core::hooks::HookOptions {
                cancel: Some(&cancel),
                reporter: Some(&mut reporter),
                ..Default::default()
            },
        );
    }
    op.clear();
    let _ = reply.send(commit_reply(result.map(|committed| committed.commit_id)));
    committed
}

impl WorkerHandle {
    pub fn get_status(&self) -> Result<Vec<StatusEntry>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetStatus { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_graph(
        &self,
        limit: usize,
        selected_branches: Option<Vec<String>>,
    ) -> Result<Vec<GraphCommit>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitGraph {
                limit,
                selected_branches,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// `old_path`: a staged rename's old path (see `status::get_working_diff`).
    pub fn get_working_diff(
        &self,
        path: String,
        staged: bool,
        opts: git_core::diff::DiffOptions,
        old_path: Option<String>,
    ) -> Result<Vec<DiffHunk>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetWorkingDiff {
                path,
                staged,
                opts,
                old_path,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_diff(
        &self,
        commit_id: String,
        path: String,
        opts: git_core::diff::DiffOptions,
    ) -> Result<Vec<DiffHunk>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitDiff {
                commit_id,
                path,
                opts,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_files(&self, commit_id: String) -> Result<Vec<String>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitFiles {
                commit_id,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_message(&self, commit_id: String) -> Result<String, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitMessage {
                commit_id,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_signature(
        &self,
        commit_id: String,
    ) -> Result<git_core::signing::CommitSignature, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitSignature {
                commit_id,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_blame(&self, commit_id: String, path: String) -> Result<Vec<BlameLine>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetBlame {
                commit_id,
                path,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_file_history(
        &self,
        path: String,
        limit: usize,
    ) -> Result<Vec<git_core::history::FileHistoryEntry>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetFileHistory {
                path,
                limit,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_rename_diff(
        &self,
        commit_id: String,
        old_path: String,
        new_path: String,
        opts: git_core::diff::DiffOptions,
    ) -> Result<Vec<DiffHunk>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCommitRenameDiff {
                commit_id,
                old_path,
                new_path,
                opts,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn stage_file(&self, path: String) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::StageFile { path, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn unstage_file(&self, path: String) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::UnstageFile { path, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn stage_hunk(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::StageHunk {
                path,
                old_start,
                new_start,
                context_lines,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn unstage_hunk(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::UnstageHunk {
                path,
                old_start,
                new_start,
                context_lines,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn discard_hunk(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::DiscardHunk {
                path,
                old_start,
                new_start,
                context_lines,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn stage_lines(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::StageLines {
                path,
                old_start,
                new_start,
                context_lines,
                line_indices,
                expected_lines,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn unstage_lines(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::UnstageLines {
                path,
                old_start,
                new_start,
                context_lines,
                line_indices,
                expected_lines,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn discard_lines(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        context_lines: u32,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::DiscardLines {
                path,
                old_start,
                new_start,
                context_lines,
                line_indices,
                expected_lines,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn discard_file(&self, path: String) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::DiscardFile { path, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// Discards exactly `paths` (the list the user confirmed), never a fresh status read.
    pub fn discard_all_unstaged(&self, paths: Vec<String>) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::DiscardAllUnstaged { paths, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn ignore_path(&self, pattern: String) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::IgnorePath { pattern, reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    /// Commits the index, running the commit hooks (`pre-commit` and `commit-msg` skipped with
    /// `no_verify`). Hook progress goes to `events`, tagged with this commit's operation id,
    /// which `cancel_transfer` accepts. A failing gate hook is `Ok(CommitOutcome::HookFailed)`.
    pub fn commit(
        &self,
        message: String,
        no_verify: bool,
        trailers: Vec<git_core::commit::Trailer>,
        events: Sender<super::HookEvent>,
    ) -> Result<git_core::commit::CommitOutcome, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Commit {
                message,
                no_verify,
                trailers,
                operation_id: super::remote::next_operation_id("commit"),
                events,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
