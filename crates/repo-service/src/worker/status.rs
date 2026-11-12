use super::{Command, ReadHandle, WorkerHandle};
use crate::wire::ErrorDto;
use git_core::blame::BlameLine;
use git_core::commit_stats::{commit_stats_batch, CommitStatsEntry, StatsBudget};
use git_core::diff::DiffHunk;
use git_core::graph::{GraphCommit, GraphSelection, WalkCache};
use git_core::status::StatusEntry;
use std::sync::mpsc::Sender;

pub(super) fn get_status(
    repo: &git2::Repository,
    reply: Sender<Result<Vec<StatusEntry>, ErrorDto>>,
) {
    let _ =
        reply.send(git_core::status::status_refreshing_index(repo).map_err(ErrorDto::from_error));
}

pub(super) fn list_untracked_folder(
    repo: &git2::Repository,
    folder: &str,
    reply: Sender<Result<Vec<StatusEntry>, ErrorDto>>,
) {
    let _ = reply
        .send(git_core::status::untracked_folder_files(repo, folder).map_err(ErrorDto::from_error));
}

pub(super) fn get_status_with_stamp(
    repo: &git2::Repository,
    reply: Sender<Result<git_core::change_stamp::StatusWithStamp, ErrorDto>>,
) {
    let _ =
        reply.send(git_core::change_stamp::status_with_stamp(repo).map_err(ErrorDto::from_error));
}

pub(super) fn get_change_stamp(
    repo: &git2::Repository,
    reply: Sender<Result<git_core::change_stamp::ChangeStamp, ErrorDto>>,
) {
    let _ = reply.send(git_core::change_stamp::change_stamp(repo).map_err(ErrorDto::from_error));
}

pub(super) fn get_commit_graph(
    repo: &git2::Repository,
    cache: &mut WalkCache,
    limit: usize,
    selection: GraphSelection,
    cursor: Option<String>,
    reply: Sender<Result<git_core::graph::GraphPage, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::graph::graph_page_cached(repo, cache, &selection, cursor.as_deref(), limit)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn locate_commit(
    repo: &git2::Repository,
    cache: &mut WalkCache,
    revision: String,
    selection: GraphSelection,
    reply: Sender<Result<Option<git_core::locate::CommitLocation>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::locate::locate_commit_cached(repo, cache, &revision, &selection)
            .map_err(ErrorDto::from_error),
    );
}

/// The commits merge `merge_id` brought in, at most `limit` (#617): a collapsed merge expanded.
pub(super) fn get_merged_commits(
    repo: &git2::Repository,
    cache: &mut WalkCache,
    merge_id: String,
    limit: usize,
    reply: Sender<Result<Vec<GraphCommit>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::graph::merged_commits_cached(repo, cache, &merge_id, limit)
            .map_err(ErrorDto::from_error),
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
    reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
) {
    let result = match old_path.filter(|_| staged) {
        Some(old_path) => git_core::diff::staged_rename_diff(repo, &old_path, &path, &opts),
        None => git_core::diff::working_diff(repo, &path, staged, &opts),
    };
    let _ = reply.send(result.map_err(ErrorDto::from_error));
}

pub(super) fn get_commit_diff(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    opts: git_core::diff::DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::diff::commit_diff(repo, &commit_id, &path, &opts).map_err(ErrorDto::from_error),
    );
}

pub(super) fn get_commit_files(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<Vec<String>, ErrorDto>>,
) {
    let _ =
        reply.send(git_core::diff::commit_files(repo, &commit_id).map_err(ErrorDto::from_error));
}

/// The line counts of `commit_ids` (#620), within the default budget so a batch can't hold the
/// worker long: the commits it didn't reach are left out, and the caller asks again.
pub(super) fn get_commit_stats(
    repo: &git2::Repository,
    commit_ids: Vec<String>,
    with_files: bool,
    reply: Sender<Result<Vec<CommitStatsEntry>, ErrorDto>>,
) {
    let entries = commit_stats_batch(repo, &commit_ids, with_files, StatsBudget::default());
    let _ = reply.send(Ok(entries));
}

pub(super) fn get_commit_signature(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<git_core::signing::CommitSignature, ErrorDto>>,
) {
    let _ = reply
        .send(git_core::signing::verify_commit(repo, &commit_id).map_err(ErrorDto::from_error));
}

/// How long `get_commit_details` may spend checking which branches and tags contain a commit
/// (#641); the rest are left unchecked and the answer says it is incomplete.
const CONTAINMENT_BUDGET: std::time::Duration = std::time::Duration::from_millis(300);

/// Author, committer and the refs that contain `commit_id` (#641).
pub(super) fn get_commit_details(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<git_core::commit_details::CommitDetails, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::commit_details::commit_details(repo, &commit_id, CONTAINMENT_BUDGET)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn get_commit_message(
    repo: &git2::Repository,
    commit_id: String,
    reply: Sender<Result<String, ErrorDto>>,
) {
    let _ =
        reply.send(git_core::graph::commit_message(repo, &commit_id).map_err(ErrorDto::from_error));
}

pub(super) fn get_blame(
    repo: &git2::Repository,
    commit_id: String,
    path: String,
    reply: Sender<Result<Vec<BlameLine>, ErrorDto>>,
) {
    let _ = reply
        .send(git_core::blame::blame_file(repo, &commit_id, &path).map_err(ErrorDto::from_error));
}

/// How long one file or line history call walks before it answers what it found (#860), like a
/// search page's budget: a long walk shares the worker, and the next call continues it.
pub(super) const HISTORY_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

pub(super) fn get_file_history(
    repo: &git2::Repository,
    cache: &mut git_core::graph::WalkCache,
    path: String,
    limit: usize,
    reply: Sender<Result<git_core::history::FileHistoryPage, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::history::file_history_cached(repo, cache, &path, limit, HISTORY_BUDGET)
            .map_err(ErrorDto::from_error),
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn get_line_history(
    repo: &git2::Repository,
    cache: &mut git_core::graph::WalkCache,
    revision: String,
    path: String,
    start_line: usize,
    end_line: usize,
    limit: usize,
    reply: Sender<Result<git_core::line_history::LineHistory, ErrorDto>>,
) {
    let lines = git_core::line_history::LineRange {
        start: start_line,
        end: end_line,
    };
    let _ = reply.send(
        git_core::line_history::line_history_cached(
            repo,
            cache,
            &revision,
            &path,
            lines,
            limit,
            HISTORY_BUDGET,
        )
        .map_err(ErrorDto::from_error),
    );
}

pub(super) fn get_commit_rename_diff(
    repo: &git2::Repository,
    commit_id: String,
    old_path: String,
    new_path: String,
    opts: git_core::diff::DiffOptions,
    reply: Sender<Result<Vec<DiffHunk>, ErrorDto>>,
) {
    let _ = reply.send(
        git_core::diff::commit_rename_diff(repo, &commit_id, &old_path, &new_path, &opts)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn stage_file(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(git_core::stage::stage_file(repo, &path).map_err(ErrorDto::from_error));
}

pub(super) fn unstage_file(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(git_core::stage::unstage_file(repo, &path).map_err(ErrorDto::from_error));
}

pub(super) fn stage_paths(
    repo: &git2::Repository,
    paths: Vec<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(git_core::stage::stage_paths(repo, &paths).map_err(ErrorDto::from_error));
}

pub(super) fn unstage_paths(
    repo: &git2::Repository,
    paths: Vec<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(git_core::stage::unstage_paths(repo, &paths).map_err(ErrorDto::from_error));
}

pub(super) fn stage_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    hunk: git_core::diff::HunkOptions,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::stage::stage_hunk(repo, &path, old_start, new_start, hunk)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn unstage_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    hunk: git_core::diff::HunkOptions,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::stage::unstage_hunk(repo, &path, old_start, new_start, hunk)
            .map_err(ErrorDto::from_error),
    );
}

pub(super) fn discard_hunk(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    hunk: git_core::diff::HunkOptions,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::stage::discard_hunk(repo, &path, old_start, new_start, hunk)
            .map_err(ErrorDto::from_error),
    );
}

// Mirrors its `Command` variant field for field.
#[allow(clippy::too_many_arguments)]
pub(super) fn stage_lines(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    hunk: git_core::diff::HunkOptions,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::stage::stage_lines(
            repo,
            &path,
            old_start,
            new_start,
            hunk,
            &line_indices,
            &expected_lines,
        )
        .map_err(ErrorDto::from_error),
    );
}

// Mirrors its `Command` variant field for field.
#[allow(clippy::too_many_arguments)]
pub(super) fn unstage_lines(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    hunk: git_core::diff::HunkOptions,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::stage::unstage_lines(
            repo,
            &path,
            old_start,
            new_start,
            hunk,
            &line_indices,
            &expected_lines,
        )
        .map_err(ErrorDto::from_error),
    );
}

// Mirrors its `Command` variant field for field.
#[allow(clippy::too_many_arguments)]
pub(super) fn discard_lines(
    repo: &git2::Repository,
    path: String,
    old_start: u32,
    new_start: u32,
    hunk: git_core::diff::HunkOptions,
    line_indices: Vec<u32>,
    expected_lines: Vec<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        git_core::stage::discard_lines(
            repo,
            &path,
            old_start,
            new_start,
            hunk,
            &line_indices,
            &expected_lines,
        )
        .map_err(ErrorDto::from_error),
    );
}

pub(super) fn discard_file(
    repo: &git2::Repository,
    path: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(git_core::stage::discard_file(repo, &path).map_err(ErrorDto::from_error));
}

pub(super) fn discard_all_unstaged(
    repo: &git2::Repository,
    paths: Vec<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply
        .send(git_core::stage::discard_all_unstaged(repo, &paths).map_err(ErrorDto::from_error));
}

pub(super) fn ignore_path(
    repo: &git2::Repository,
    pattern: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(git_core::stage::ignore_path(repo, &pattern).map_err(ErrorDto::from_error));
}

pub(super) fn ignore_paths(
    repo: &git2::Repository,
    patterns: Vec<String>,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ =
        reply.send(git_core::stage::ignore_paths(repo, &patterns).map_err(ErrorDto::from_error));
}

/// The reply both commit and amend send: a hook failure is an outcome the UI renders, every
/// other failure stays an error message.
pub(super) fn commit_reply(
    result: Result<String, git_core::commit::CommitError>,
) -> Result<git_core::commit::CommitOutcome, ErrorDto> {
    use git_core::commit::{CommitError, CommitOutcome};
    match result {
        Ok(commit_id) => Ok(CommitOutcome::Committed { commit_id }),
        Err(CommitError::HookFailed { hook, output }) => {
            Ok(CommitOutcome::HookFailed { hook, output })
        }
        Err(error) => Err(ErrorDto::from_error(error)),
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
    op: super::remote::TransferOp,
    events: Sender<super::HookEvent>,
    reply: Sender<Result<git_core::commit::CommitOutcome, ErrorDto>>,
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
    pub fn stage_file(&self, path: String) -> Result<(), ErrorDto> {
        self.request(|tx| Command::StageFile { path, reply: tx })
    }

    pub fn unstage_file(&self, path: String) -> Result<(), ErrorDto> {
        self.request(|tx| Command::UnstageFile { path, reply: tx })
    }

    /// Stages `paths` as one undo step; on a failure none of them is staged (#528).
    pub fn stage_paths(&self, paths: Vec<String>) -> Result<(), ErrorDto> {
        self.request(|tx| Command::StagePaths { paths, reply: tx })
    }

    /// Unstages `paths` as one undo step; on a failure none of them is unstaged (#528).
    pub fn unstage_paths(&self, paths: Vec<String>) -> Result<(), ErrorDto> {
        self.request(|tx| Command::UnstagePaths { paths, reply: tx })
    }

    pub fn stage_hunk(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
    ) -> Result<(), ErrorDto> {
        self.request(|tx| Command::StageHunk {
            path,
            old_start,
            new_start,
            hunk,
            reply: tx,
        })
    }

    pub fn unstage_hunk(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
    ) -> Result<(), ErrorDto> {
        self.request(|tx| Command::UnstageHunk {
            path,
            old_start,
            new_start,
            hunk,
            reply: tx,
        })
    }

    pub fn discard_hunk(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
    ) -> Result<(), ErrorDto> {
        self.request(|tx| Command::DiscardHunk {
            path,
            old_start,
            new_start,
            hunk,
            reply: tx,
        })
    }

    pub fn stage_lines(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
    ) -> Result<(), ErrorDto> {
        self.request(|tx| Command::StageLines {
            path,
            old_start,
            new_start,
            hunk,
            line_indices,
            expected_lines,
            reply: tx,
        })
    }

    pub fn unstage_lines(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
    ) -> Result<(), ErrorDto> {
        self.request(|tx| Command::UnstageLines {
            path,
            old_start,
            new_start,
            hunk,
            line_indices,
            expected_lines,
            reply: tx,
        })
    }

    pub fn discard_lines(
        &self,
        path: String,
        old_start: u32,
        new_start: u32,
        hunk: git_core::diff::HunkOptions,
        line_indices: Vec<u32>,
        expected_lines: Vec<String>,
    ) -> Result<(), ErrorDto> {
        self.request(|tx| Command::DiscardLines {
            path,
            old_start,
            new_start,
            hunk,
            line_indices,
            expected_lines,
            reply: tx,
        })
    }

    pub fn discard_file(&self, path: String) -> Result<(), ErrorDto> {
        self.request(|tx| Command::DiscardFile { path, reply: tx })
    }

    /// Discards exactly `paths` (the list the user confirmed), never a fresh status read.
    pub fn discard_all_unstaged(&self, paths: Vec<String>) -> Result<(), ErrorDto> {
        self.request(|tx| Command::DiscardAllUnstaged { paths, reply: tx })
    }

    pub fn ignore_path(&self, pattern: String) -> Result<(), ErrorDto> {
        self.request(|tx| Command::IgnorePath { pattern, reply: tx })
    }

    /// Adds `patterns` to `.gitignore` in one write, one undo step (#528).
    pub fn ignore_paths(&self, patterns: Vec<String>) -> Result<(), ErrorDto> {
        self.request(|tx| Command::IgnorePaths {
            patterns,
            reply: tx,
        })
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
    ) -> Result<git_core::commit::CommitOutcome, ErrorDto> {
        self.request(|tx| Command::Commit {
            message,
            no_verify,
            trailers,
            operation_id: self.cancelled.allocate("commit"),
            events,
            reply: tx,
        })
    }
    /// Sets the git identity commits are made with (#689): in the repository's own config, or
    /// with `global` in the user's global git config.
    pub fn set_identity(&self, name: String, email: String, global: bool) -> Result<(), ErrorDto> {
        self.request(|reply| Command::SetIdentity {
            name,
            email,
            global,
            reply,
        })
    }
}

impl ReadHandle {
    pub fn get_status(&self) -> Result<Vec<StatusEntry>, ErrorDto> {
        self.read(|tx| Command::GetStatus { reply: tx })
    }

    /// The repository's change stamp (#650); see `git_core::change_stamp`.
    pub fn get_change_stamp(&self) -> Result<git_core::change_stamp::ChangeStamp, ErrorDto> {
        self.read(|tx| Command::GetChangeStamp { reply: tx })
    }

    /// The files of the untracked folder `folder` (`dir/`), listed as one entry in the status
    /// (#845).
    pub fn list_untracked_folder(&self, folder: String) -> Result<Vec<StatusEntry>, ErrorDto> {
        self.read(|tx| Command::ListUntrackedFolder { folder, reply: tx })
    }

    /// The status and the change stamp from one scan of the work tree (#841), which also
    /// refreshes the index's stat data; see `git_core::change_stamp::status_with_stamp`.
    pub fn get_status_with_stamp(
        &self,
    ) -> Result<git_core::change_stamp::StatusWithStamp, ErrorDto> {
        self.read(|tx| Command::GetStatusWithStamp { reply: tx })
    }

    /// One page of the graph of `selection` (#685), from `cursor` on (`None`: the first commit).
    pub fn get_commit_graph(
        &self,
        limit: usize,
        selection: GraphSelection,
        cursor: Option<String>,
    ) -> Result<git_core::graph::GraphPage, ErrorDto> {
        self.read(|tx| Command::GetCommitGraph {
            limit,
            selection,
            cursor,
            reply: tx,
        })
    }

    /// Where `revision`'s commit sits in the graph of `selection` (#441).
    pub fn locate_commit(
        &self,
        revision: String,
        selection: GraphSelection,
    ) -> Result<Option<git_core::locate::CommitLocation>, ErrorDto> {
        self.read(|tx| Command::LocateCommit {
            revision,
            selection,
            reply: tx,
        })
    }

    /// The commits merge `merge_id` brought in, at most `limit` (#617).
    pub fn get_merged_commits(
        &self,
        merge_id: String,
        limit: usize,
    ) -> Result<Vec<GraphCommit>, ErrorDto> {
        self.read(|tx| Command::GetMergedCommits {
            merge_id,
            limit,
            reply: tx,
        })
    }

    /// `old_path`: a staged rename's old path (see `status::get_working_diff`).
    pub fn get_working_diff(
        &self,
        path: String,
        staged: bool,
        opts: git_core::diff::DiffOptions,
        old_path: Option<String>,
    ) -> Result<Vec<DiffHunk>, ErrorDto> {
        self.read(|tx| Command::GetWorkingDiff {
            path,
            staged,
            opts,
            old_path,
            reply: tx,
        })
    }

    pub fn get_commit_diff(
        &self,
        commit_id: String,
        path: String,
        opts: git_core::diff::DiffOptions,
    ) -> Result<Vec<DiffHunk>, ErrorDto> {
        self.read(|tx| Command::GetCommitDiff {
            commit_id,
            path,
            opts,
            reply: tx,
        })
    }

    pub fn get_commit_files(&self, commit_id: String) -> Result<Vec<String>, ErrorDto> {
        self.read(|tx| Command::GetCommitFiles {
            commit_id,
            reply: tx,
        })
    }

    /// Line counts of `commit_ids` (#620); see `status::get_commit_stats`.
    pub fn get_commit_stats(
        &self,
        commit_ids: Vec<String>,
        with_files: bool,
    ) -> Result<Vec<CommitStatsEntry>, ErrorDto> {
        self.read(|tx| Command::GetCommitStats {
            commit_ids,
            with_files,
            reply: tx,
        })
    }

    pub fn get_commit_message(&self, commit_id: String) -> Result<String, ErrorDto> {
        self.read(|tx| Command::GetCommitMessage {
            commit_id,
            reply: tx,
        })
    }

    /// Author, committer and containing refs of `commit_id` (#641); see
    /// `status::get_commit_details`.
    pub fn get_commit_details(
        &self,
        commit_id: String,
    ) -> Result<git_core::commit_details::CommitDetails, ErrorDto> {
        self.read(|tx| Command::GetCommitDetails {
            commit_id,
            reply: tx,
        })
    }

    pub fn get_commit_signature(
        &self,
        commit_id: String,
    ) -> Result<git_core::signing::CommitSignature, ErrorDto> {
        self.read(|tx| Command::GetCommitSignature {
            commit_id,
            reply: tx,
        })
    }

    pub fn get_blame(&self, commit_id: String, path: String) -> Result<Vec<BlameLine>, ErrorDto> {
        self.read(|tx| Command::GetBlame {
            commit_id,
            path,
            reply: tx,
        })
    }

    pub fn get_file_history(
        &self,
        path: String,
        limit: usize,
    ) -> Result<git_core::history::FileHistoryPage, ErrorDto> {
        self.read(|tx| Command::GetFileHistory {
            path,
            limit,
            reply: tx,
        })
    }

    pub fn get_line_history(
        &self,
        revision: String,
        path: String,
        start_line: usize,
        end_line: usize,
        limit: usize,
    ) -> Result<git_core::line_history::LineHistory, ErrorDto> {
        self.read(|tx| Command::GetLineHistory {
            revision,
            path,
            start_line,
            end_line,
            limit,
            reply: tx,
        })
    }

    pub fn get_commit_rename_diff(
        &self,
        commit_id: String,
        old_path: String,
        new_path: String,
        opts: git_core::diff::DiffOptions,
    ) -> Result<Vec<DiffHunk>, ErrorDto> {
        self.read(|tx| Command::GetCommitRenameDiff {
            commit_id,
            old_path,
            new_path,
            opts,
            reply: tx,
        })
    }
}

pub(super) fn set_identity(
    repo: &git2::Repository,
    name: String,
    email: String,
    global: bool,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let result = git_core::commit::identity_config(repo, global)
        .and_then(|mut config| git_core::commit::set_identity(&mut config, &name, &email))
        .map_err(ErrorDto::from_error);
    let _ = reply.send(result);
}
