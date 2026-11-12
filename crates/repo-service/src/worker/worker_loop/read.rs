//! The read lane (#734): the commands `policy` places on `Lane::Read`, which only read the
//! repository, and the host a transfer runs on, which serves them while the worker waits for the
//! transfer's helper.
//!
//! Idle, a read runs through `WorkerLoop::step` like any command. While the worker waits for a
//! helper (`transfer_thread::run`), its own `Repository` is idle: `Host::serve_read` takes the
//! queued commands, runs the reads at once on that handle and keeps the rest, in order, for after
//! the transfer (`WorkerLoop::deferred`). Such a read skips the state lock, the undo bookkeeping
//! and the operation gate: it changes nothing they cover, and the next main-lane command settles
//! undo as usual. Reads never overlap the worker's own writes (one thread); they only overlap a
//! helper's network phase, whose writes (a new pack, then remote-tracking refs) never leave a
//! ref naming a missing object.
use std::collections::VecDeque;
use std::sync::mpsc::Receiver;

use git2::Repository;

use super::Operations;
use crate::worker::policy::{self, Lane};
use crate::worker::transfer_thread::TransferHost;
use crate::worker::undo::UndoRecorder;
use crate::worker::{
    branch, compare, forge, history, merge, patch, rebase, reflog, remote, search, stash, status,
    submodule, tag, tools, tree, undo, worktree, Command, SearchGeneration,
};

/// The worker state the reads use besides the repository: disjoint from what a transfer
/// borrows, so a transfer's host can lend it out.
pub(super) struct ReadState<'a> {
    pub(super) search_generation: &'a SearchGeneration,
    // The walk orders and ref badges of the graph, search and "Go to" (#835).
    pub(super) walk_cache: &'a mut git_core::graph::WalkCache,
    pub(super) undo_recorder: &'a mut UndoRecorder,
    pub(super) operations: &'a Operations,
    pub(super) clean_merge: &'a Option<merge::CleanMerge>,
    // The branches' ahead/behind counts (#112).
    pub(super) ahead_behind: &'a mut git_core::ahead_behind::AheadBehindCache,
    // The co-author suggestions for the current HEAD (#140).
    pub(super) recent_authors: &'a mut git_core::commit::RecentAuthorsCache,
}

/// What a transfer borrows from the worker loop: the repository, and the queue it serves reads
/// from while it waits (see the module docs).
pub(in crate::worker) struct Host<'a> {
    pub(super) repo: &'a mut Repository,
    pub(super) queue: &'a Receiver<Command>,
    pub(super) deferred: &'a mut VecDeque<Command>,
    pub(super) state: ReadState<'a>,
}

impl TransferHost for Host<'_> {
    fn repo(&mut self) -> &mut Repository {
        self.repo
    }

    fn serve_read(&mut self) -> bool {
        while let Ok(command) = self.queue.try_recv() {
            if policy::of(&command).lane != Lane::Read {
                self.deferred.push_back(command);
                continue;
            }
            if let Some(command) = dispatch(self.repo, &mut self.state, command) {
                // `policy` and `dispatch` disagree; run it after the transfer instead.
                debug_assert!(false, "a read-lane command `dispatch` doesn't handle");
                self.deferred.push_back(command);
                continue;
            }
            return true;
        }
        false
    }
}

/// Runs `command` if it is one of the read lane's (`None`), or hands it back.
pub(super) fn dispatch(
    repo: &mut Repository,
    state: &mut ReadState<'_>,
    command: Command,
) -> Option<Command> {
    match command {
        Command::GetStatus { reply } => status::get_status(repo, reply),
        Command::GetChangeStamp { reply } => status::get_change_stamp(repo, reply),
        Command::GetStatusWithStamp { reply } => status::get_status_with_stamp(repo, reply),
        Command::ListUntrackedFolder { folder, reply } => {
            status::list_untracked_folder(repo, &folder, reply)
        }
        Command::GetCommitGraph {
            limit,
            selection,
            cursor,
            reply,
        } => status::get_commit_graph(repo, state.walk_cache, limit, selection, cursor, reply),
        Command::LocateCommit {
            revision,
            selection,
            reply,
        } => status::locate_commit(repo, state.walk_cache, revision, selection, reply),
        Command::GetMergedCommits {
            merge_id,
            limit,
            reply,
        } => status::get_merged_commits(repo, state.walk_cache, merge_id, limit, reply),
        Command::GetWorkingDiff {
            path,
            staged,
            opts,
            old_path,
            reply,
        } => status::get_working_diff(repo, path, staged, opts, old_path, reply),
        Command::GetCommitDiff {
            commit_id,
            path,
            opts,
            reply,
        } => status::get_commit_diff(repo, commit_id, path, opts, reply),
        Command::GetCommitFiles { commit_id, reply } => {
            status::get_commit_files(repo, commit_id, reply)
        }
        Command::GetCommitStats {
            commit_ids,
            with_files,
            reply,
        } => status::get_commit_stats(repo, commit_ids, with_files, reply),
        Command::GetCommitMessage { commit_id, reply } => {
            status::get_commit_message(repo, commit_id, reply)
        }
        Command::GetCommitSignature { commit_id, reply } => {
            status::get_commit_signature(repo, commit_id, reply)
        }
        Command::GetCommitDetails { commit_id, reply } => {
            status::get_commit_details(repo, commit_id, reply)
        }
        Command::GetBlame {
            commit_id,
            path,
            reply,
        } => status::get_blame(repo, commit_id, path, reply),
        Command::GetFileHistory { path, limit, reply } => {
            status::get_file_history(repo, path, limit, reply)
        }
        Command::GetLineHistory {
            revision,
            path,
            start_line,
            end_line,
            limit,
            reply,
        } => status::get_line_history(repo, revision, path, start_line, end_line, limit, reply),
        Command::SearchCommits {
            query,
            selection,
            cursor,
            generation,
            reply,
        } => search::search_commits(
            repo,
            state.walk_cache,
            state.search_generation,
            generation,
            query,
            selection,
            cursor,
            reply,
        ),
        Command::GetCommitRenameDiff {
            commit_id,
            old_path,
            new_path,
            opts,
            reply,
        } => status::get_commit_rename_diff(repo, commit_id, old_path, new_path, opts, reply),
        Command::GetRangeFiles {
            from_id,
            to_id,
            reply,
        } => compare::get_range_files(repo, from_id, to_id, reply),
        Command::GetRangeDiff {
            from_id,
            to_id,
            path,
            old_path,
            opts,
            reply,
        } => compare::get_range_diff(repo, from_id, to_id, path, old_path, opts, reply),
        Command::GetWorkingTreeCompareFiles { commit_id, reply } => {
            compare::get_working_tree_compare_files(repo, commit_id, reply)
        }
        Command::GetWorkingTreeCompareDiff {
            commit_id,
            path,
            old_path,
            opts,
            reply,
        } => compare::get_working_tree_compare_diff(repo, commit_id, path, old_path, opts, reply),
        Command::GetBranchCompareRange { ref_name, reply } => {
            compare::get_branch_compare_range(repo, ref_name, reply)
        }
        Command::GetPatch { source, reply } => patch::get_patch(repo, source, reply),
        Command::PreviewPatch { content, reply } => patch::preview_patch(repo, content, reply),
        Command::ListBranches { reply } => branch::list(repo, state.ahead_behind, reply),
        Command::GetSwitchConflicts { name, reply } => branch::switch_conflicts(repo, name, reply),
        Command::GetPullDefaults { reply } => remote::pull_defaults(repo, reply),
        Command::ListWorktrees { reply } => worktree::list(repo, reply),
        Command::ListSubmodules { reply } => submodule::list(repo, reply),
        Command::ListReflogRefs { reply } => reflog::list_refs(repo, reply),
        Command::GetReflog { reference, reply } => reflog::get(repo, reference, reply),
        Command::ListRemotes { reply } => remote::list(repo, reply),
        Command::ListRemoteBranches { remote_name, reply } => {
            remote::list_branches(repo, remote_name, reply)
        }
        Command::GetCurrentUpstream { reply } => {
            remote::current_upstream(repo, state.ahead_behind, reply)
        }
        Command::GetRemoteUpstreams { name, reply } => {
            remote::upstreams(repo, state.ahead_behind, name, reply)
        }
        Command::GetRemoteAuthMode { name, reply } => remote::auth_mode(repo, name, reply),
        Command::ListTags { reply } => tag::list(repo, reply),
        Command::ListStashes { reply } => stash::list(repo, reply),
        Command::GetStashFiles { commit_id, reply } => stash::files(repo, commit_id, reply),
        Command::GetMergeDefaults { reply } => merge::defaults(repo, reply),
        Command::GetConflictHunks { path, reply } => merge::conflict_hunks(repo, path, reply),
        Command::GetHeadInfo { reply } => {
            let clean = state.clean_merge.as_ref().is_some_and(|m| m.matches(repo));
            history::head_info(repo, &state.operations.probe(), clean, reply)
        }
        Command::GetMergeMessage { reply } => merge::message(repo, reply),
        Command::PrepareDiffTool {
            path,
            source,
            reply,
        } => tools::prepare_diff_tool(repo, path, source, reply),
        Command::GetGitTools { reply } => tools::git_tools(repo, reply),
        Command::GetFileSides {
            path,
            source,
            reply,
        } => tools::file_sides(repo, path, source, reply),
        Command::ListRecentAuthors { reply } => {
            history::recent_authors(repo, state.recent_authors, reply)
        }
        Command::CheckMergeTarget { path, reply } => tools::check_merge_target(repo, path, reply),
        Command::ReadFileVersion {
            path,
            version,
            reply,
        } => tools::read_file_version(repo, path, version, reply),
        Command::GetCommitTree {
            commit_id,
            dir,
            filter,
            reply,
        } => tree::get_commit_tree(repo, commit_id, dir, filter, reply),
        Command::GetFileAtCommit {
            commit_id,
            path,
            reply,
        } => tree::get_file_at_commit(repo, commit_id, path, reply),
        Command::SaveFileVersion {
            commit_id,
            path,
            destination,
            reply,
        } => tree::save_file_version(repo, commit_id, path, destination, reply),
        Command::CommitsSince { onto, reply } => rebase::commits_since(repo, onto, reply),
        Command::DetectForgeRepository { reply } => forge::detect(repo, reply),
        Command::GetUndoState { reply } => undo::get_state(state.undo_recorder, reply),
        other => return Some(other),
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::policy_tests::table;
    use crate::worker::rebase_store::RebaseStore;
    use crate::worker::sequence_store::SequenceStore;
    use crate::worker::{gate, merge};

    /// `policy` and `dispatch` agree: every read-lane command runs here, every other comes back.
    #[test]
    fn dispatch_runs_exactly_the_read_lane() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let mut repo = Repository::init(dir.path()).expect("init");
        let mut undo_recorder = UndoRecorder::open(&repo);
        let operations = Operations {
            sequence_store: SequenceStore::open(&repo),
            rebase_store: RebaseStore::open(&repo),
            rebase_memory: gate::RebaseMemory::default(),
        };
        let mut ahead_behind = git_core::ahead_behind::AheadBehindCache::default();
        let mut recent_authors = git_core::commit::RecentAuthorsCache::default();
        let clean_merge: Option<merge::CleanMerge> = None;
        let search_generation = SearchGeneration::default();
        let mut walk_cache = git_core::graph::WalkCache::default();
        let mut state = ReadState {
            search_generation: &search_generation,
            walk_cache: &mut walk_cache,
            undo_recorder: &mut undo_recorder,
            operations: &operations,
            clean_merge: &clean_merge,
            ahead_behind: &mut ahead_behind,
            recent_authors: &mut recent_authors,
        };
        for ((name, command), _) in table() {
            if matches!(command, Command::PanicNow) {
                continue;
            }
            let read = policy::of(&command).lane == Lane::Read;
            let ran = dispatch(&mut repo, &mut state, command).is_none();
            assert_eq!(ran, read, "{name}");
        }
    }
}
