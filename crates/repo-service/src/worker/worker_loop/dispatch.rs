//! The worker's `match` over `Command`: each arm hands its command to the module for its area.

use git_core::merge::MergeOutcome;

use super::WorkerLoop;
use crate::credentials::CredentialStore;
use crate::pull_requests::ForgeApi;
use crate::worker::state_lock::StateGuard;
use crate::worker::{
    branch, compare, fetch_all, forge, history, merge, patch, rebase, reflog, remote, search,
    stash, status, submodule, tag, tools, tree, undo, worktree, Command,
};

impl<S, A> WorkerLoop<S, A>
where
    S: CredentialStore + Send + Sync + 'static,
    A: ForgeApi + Send + 'static,
{
    /// Runs `command`, after `before` gated it. `guard` is the state lock `acquire` took.
    pub(super) fn dispatch(&mut self, command: Command, guard: Option<&StateGuard>) {
        match command {
            Command::Ping { reply } => {
                let _ = reply.send(());
            }
            #[cfg(test)]
            Command::PanicNow => {
                panic!("intentional panic for worker-crash-recovery test")
            }
            Command::GetStatus { reply } => status::get_status(&self.repo, reply),
            Command::GetCommitGraph {
                limit,
                selection,
                reply,
            } => status::get_commit_graph(&self.repo, limit, selection, reply),
            Command::LocateCommit {
                revision,
                selection,
                reply,
            } => status::locate_commit(&self.repo, revision, selection, reply),
            Command::GetMergedCommits {
                merge_id,
                limit,
                reply,
            } => status::get_merged_commits(&self.repo, merge_id, limit, reply),
            Command::GetWorkingDiff {
                path,
                staged,
                opts,
                old_path,
                reply,
            } => status::get_working_diff(&self.repo, path, staged, opts, old_path, reply),
            Command::GetCommitDiff {
                commit_id,
                path,
                opts,
                reply,
            } => status::get_commit_diff(&self.repo, commit_id, path, opts, reply),
            Command::GetCommitFiles { commit_id, reply } => {
                status::get_commit_files(&self.repo, commit_id, reply)
            }
            Command::GetCommitStats {
                commit_ids,
                with_files,
                reply,
            } => status::get_commit_stats(&self.repo, commit_ids, with_files, reply),
            Command::GetCommitMessage { commit_id, reply } => {
                status::get_commit_message(&self.repo, commit_id, reply)
            }
            Command::GetCommitSignature { commit_id, reply } => {
                status::get_commit_signature(&self.repo, commit_id, reply)
            }
            Command::GetCommitDetails { commit_id, reply } => {
                status::get_commit_details(&self.repo, commit_id, reply)
            }
            Command::GetBlame {
                commit_id,
                path,
                reply,
            } => status::get_blame(&self.repo, commit_id, path, reply),
            Command::GetFileHistory { path, limit, reply } => {
                status::get_file_history(&self.repo, path, limit, reply)
            }
            Command::GetLineHistory {
                revision,
                path,
                start_line,
                end_line,
                limit,
                reply,
            } => status::get_line_history(
                &self.repo, revision, path, start_line, end_line, limit, reply,
            ),
            Command::SearchCommits {
                query,
                selection,
                cursor,
                generation,
                reply,
            } => search::search_commits(
                &self.repo,
                &self.search_generation,
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
            } => status::get_commit_rename_diff(
                &self.repo, commit_id, old_path, new_path, opts, reply,
            ),
            Command::GetRangeFiles {
                from_id,
                to_id,
                reply,
            } => compare::get_range_files(&self.repo, from_id, to_id, reply),
            Command::GetRangeDiff {
                from_id,
                to_id,
                path,
                old_path,
                opts,
                reply,
            } => compare::get_range_diff(&self.repo, from_id, to_id, path, old_path, opts, reply),
            Command::GetWorkingTreeCompareFiles { commit_id, reply } => {
                compare::get_working_tree_compare_files(&self.repo, commit_id, reply)
            }
            Command::GetWorkingTreeCompareDiff {
                commit_id,
                path,
                old_path,
                opts,
                reply,
            } => compare::get_working_tree_compare_diff(
                &self.repo, commit_id, path, old_path, opts, reply,
            ),
            Command::GetBranchCompareRange { ref_name, reply } => {
                compare::get_branch_compare_range(&self.repo, ref_name, reply)
            }
            Command::StageFile { path, reply } => status::stage_file(&self.repo, path, reply),
            Command::UnstageFile { path, reply } => status::unstage_file(&self.repo, path, reply),
            Command::StageHunk {
                path,
                old_start,
                new_start,
                hunk,
                reply,
            } => status::stage_hunk(&self.repo, path, old_start, new_start, hunk, reply),
            Command::UnstageHunk {
                path,
                old_start,
                new_start,
                hunk,
                reply,
            } => status::unstage_hunk(&self.repo, path, old_start, new_start, hunk, reply),
            Command::DiscardHunk {
                path,
                old_start,
                new_start,
                hunk,
                reply,
            } => status::discard_hunk(&self.repo, path, old_start, new_start, hunk, reply),
            Command::StageLines {
                path,
                old_start,
                new_start,
                hunk,
                line_indices,
                expected_lines,
                reply,
            } => status::stage_lines(
                &self.repo,
                path,
                old_start,
                new_start,
                hunk,
                line_indices,
                expected_lines,
                reply,
            ),
            Command::UnstageLines {
                path,
                old_start,
                new_start,
                hunk,
                line_indices,
                expected_lines,
                reply,
            } => status::unstage_lines(
                &self.repo,
                path,
                old_start,
                new_start,
                hunk,
                line_indices,
                expected_lines,
                reply,
            ),
            Command::DiscardLines {
                path,
                old_start,
                new_start,
                hunk,
                line_indices,
                expected_lines,
                reply,
            } => status::discard_lines(
                &self.repo,
                path,
                old_start,
                new_start,
                hunk,
                line_indices,
                expected_lines,
                reply,
            ),
            Command::DiscardFile { path, reply } => status::discard_file(&self.repo, path, reply),
            Command::DiscardAllUnstaged { paths, reply } => {
                status::discard_all_unstaged(&self.repo, paths, reply)
            }
            Command::ApplyCommitChange { change, reply } => {
                patch::apply_commit_change(&self.repo, change, reply)
            }
            Command::GetPatch { source, reply } => patch::get_patch(&self.repo, source, reply),
            Command::ApplyPatch { content, reply } => {
                patch::apply_patch(&self.repo, content, reply)
            }
            Command::IgnorePath { pattern, reply } => {
                status::ignore_path(&self.repo, pattern, reply)
            }
            Command::CreateRootFile { name, reply } => {
                tools::create_root_file(&self.repo, name, reply)
            }
            Command::Commit {
                message,
                no_verify,
                trailers,
                operation_id,
                events,
                reply,
            } => {
                let set = match &self.clean_merge {
                    Some(merge) if merge.matches(&mut self.repo) => merge.hook_set(),
                    _ => git_core::commit::CommitHookSet::Commit,
                };
                let committed = status::commit(
                    &mut self.repo,
                    message,
                    no_verify,
                    trailers,
                    set,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
                // A failed or cancelled commit keeps it: "Commit without hooks" still
                // concludes the clean merge (`git merge --no-verify`).
                if committed {
                    self.clean_merge = None;
                }
            }
            Command::ListBranches { reply } => branch::list(&self.repo, reply),
            Command::CreateBranch {
                name,
                start_point,
                checkout,
                reply,
            } => branch::create(&self.repo, name, start_point, checkout, reply),
            Command::GetSwitchConflicts { name, reply } => {
                branch::switch_conflicts(&self.repo, name, reply)
            }
            Command::StashAndSwitch {
                name,
                reapply,
                operation_id,
                events,
                reply,
            } => branch::stash_and_switch(
                &mut self.repo,
                name,
                reapply,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::SetIdentity {
                name,
                email,
                global,
                reply,
            } => status::set_identity(&self.repo, name, email, global, reply),
            Command::GetPullDefaults { reply } => remote::pull_defaults(&self.repo, reply),
            Command::SwitchBranch {
                name,
                operation_id,
                events,
                reply,
            } => branch::switch(
                &self.repo,
                name,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::DeleteBranch { name, force, reply } => {
                branch::delete(&self.repo, name, force, reply)
            }
            Command::RenameBranch {
                old_name,
                new_name,
                reply,
            } => branch::rename(&self.repo, old_name, new_name, reply),
            Command::ListWorktrees { reply } => worktree::list(&self.repo, reply),
            Command::CreateWorktree {
                name,
                path,
                branch,
                start_point,
                reply,
            } => worktree::create(
                &self.repo_path,
                &mut self.repo,
                name,
                path,
                branch,
                start_point,
                reply,
            ),
            Command::RemoveWorktree { name, reply } => {
                worktree::remove(&self.repo_path, &mut self.repo, name, reply)
            }
            Command::PruneWorktrees { reply } => {
                worktree::prune(&self.repo_path, &mut self.repo, reply)
            }
            Command::ListSubmodules { reply } => submodule::list(&self.repo, reply),
            Command::InitSubmodule { path, reply } => submodule::init(&self.repo, path, reply),
            Command::UpdateSubmodule {
                path,
                recursive,
                reply,
            } => submodule::update(&self.repo, path, recursive, reply),
            Command::ListReflogRefs { reply } => reflog::list_refs(&self.repo, reply),
            Command::GetReflog { reference, reply } => reflog::get(&self.repo, reference, reply),
            Command::RestoreReflogEntry {
                reference,
                new_id,
                reply,
            } => reflog::restore(&self.repo, reference, new_id, reply),
            Command::ListRemotes { reply } => remote::list(&self.repo, reply),
            Command::ListRemoteBranches { remote_name, reply } => {
                remote::list_branches(&self.repo, remote_name, reply)
            }
            Command::GetCurrentUpstream { reply } => remote::current_upstream(&self.repo, reply),
            Command::AddRemote {
                name,
                fetch_url,
                push_url,
                reply,
            } => remote::add(&self.repo, name, fetch_url, push_url, reply),
            Command::RenameRemote {
                old_name,
                new_name,
                reply,
            } => remote::rename(&self.repo, old_name, new_name, reply),
            Command::UpdateRemoteUrls {
                name,
                fetch_url,
                push_url,
                reply,
            } => remote::update_urls(&self.repo, name, fetch_url, push_url, reply),
            Command::GetRemoteUpstreams { name, reply } => {
                remote::upstreams(&self.repo, name, reply)
            }
            Command::GetRemoteAuthMode { name, reply } => {
                remote::auth_mode(&self.repo, name, reply)
            }
            Command::SaveHttpsCredential {
                remote_name,
                username,
                token,
                reply,
            } => remote::save_https(
                &self.repo,
                &self.credential_service,
                remote_name,
                username,
                token,
                reply,
            ),
            Command::ForgetHttpsCredential { remote_name, reply } => {
                remote::forget_https(&self.repo, &self.credential_service, remote_name, reply)
            }
            Command::SetRemoteAuthMode {
                remote_name,
                mode,
                reply,
            } => remote::set_auth_mode(&self.repo, remote_name, mode, reply),
            Command::RemoveRemote {
                name,
                clear_upstreams,
                reply,
            } => remote::remove(&self.repo, name, clear_upstreams, reply),
            Command::SetCurrentUpstream {
                remote_name,
                remote_branch,
                reply,
            } => remote::set_current_upstream(&self.repo, remote_name, remote_branch, reply),
            Command::ClearCurrentUpstream { reply } => {
                remote::clear_current_upstream(&self.repo, reply)
            }
            Command::ListTags { reply } => tag::list(&self.repo, reply),
            Command::CreateTag {
                name,
                message,
                target,
                reply,
            } => tag::create(&self.repo, name, message, target, reply),
            Command::DeleteTag { name, reply } => tag::delete(&self.repo, name, reply),
            Command::ListStashes { reply } => stash::list(&mut self.repo, reply),
            Command::SaveStash { options, reply } => stash::save(&mut self.repo, options, reply),
            Command::ApplyStash {
                index,
                commit_id,
                restore_index,
                reply,
            } => stash::apply(&mut self.repo, index, &commit_id, restore_index, reply),
            Command::PopStash {
                index,
                commit_id,
                restore_index,
                reply,
            } => stash::pop(&mut self.repo, index, &commit_id, restore_index, reply),
            Command::GetStashFiles { commit_id, reply } => {
                stash::files(&mut self.repo, commit_id, reply)
            }
            Command::DropStash {
                index,
                commit_id,
                reply,
            } => stash::drop(&mut self.repo, index, &commit_id, reply),
            Command::StartMerge {
                branch_name,
                options,
                operation_id,
                events,
                reply,
            } => {
                let result = merge::start(
                    &self.repo,
                    branch_name,
                    options,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
                match result {
                    Ok(MergeOutcome::Merged) => {
                        self.clean_merge = merge::CleanMerge::capture(&mut self.repo)
                    }
                    Ok(_) => self.clean_merge = None,
                    // Refused (e.g. "already merging"): nothing changed, so a staged
                    // clean merge is still the one remembered.
                    Err(_) => {}
                }
            }
            Command::GetMergeDefaults { reply } => merge::defaults(&self.repo, reply),
            Command::GetConflictHunks { path, reply } => {
                merge::conflict_hunks(&self.repo, path, reply)
            }
            Command::ResolveConflict {
                path,
                resolved_content,
                reply,
            } => merge::resolve(&self.repo, path, resolved_content, reply),
            Command::AbortMerge { reply } => {
                self.clean_merge = None;
                // A stale UI's Abort during a sequence must not strand sequence.json.
                if self.operations.sequence_store.load(&self.repo).is_some() {
                    history::abort_sequence_for_merge(
                        &self.repo,
                        &self.operations.sequence_store,
                        reply,
                    )
                } else if let Some(refusal) = history::abort_merge_refusal(&self.repo) {
                    // A terminal sequence: only AbortSequence ends it (#253).
                    let _ = reply.send(Err(refusal.into()));
                } else {
                    merge::abort(&self.repo, reply)
                }
            }
            Command::GetHeadInfo { reply } => {
                let clean = self
                    .clean_merge
                    .as_ref()
                    .is_some_and(|m| m.matches(&mut self.repo));
                history::head_info(&self.repo, &self.operations.probe(), clean, reply)
            }
            Command::AmendCommit {
                message,
                no_verify,
                author,
                trailers,
                operation_id,
                events,
                reply,
            } => history::amend(
                &self.repo,
                message,
                no_verify,
                author,
                trailers,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::CherryPick {
                commit_id,
                mainline,
                operation_id,
                events,
                reply,
            } => history::pick_one(
                &self.repo,
                git_core::pick::SequenceKind::CherryPick,
                commit_id,
                mainline,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::RevertCommit {
                commit_id,
                mainline,
                operation_id,
                events,
                reply,
            } => history::pick_one(
                &self.repo,
                git_core::pick::SequenceKind::Revert,
                commit_id,
                mainline,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::CherryPickMany {
                commit_ids,
                operation_id,
                events,
                reply,
            } => {
                let kind = git_core::pick::SequenceKind::CherryPick;
                let result = history::pick_many(
                    &self.repo,
                    &self.operations.sequence_store,
                    kind,
                    commit_ids,
                    remote::TransferOp::new(operation_id),
                    events,
                );
                undo::relabel_sequence(&mut self.undo_recorder, kind, &result);
                let _ = reply.send(result);
            }
            Command::RevertMany {
                commit_ids,
                operation_id,
                events,
                reply,
            } => {
                let kind = git_core::pick::SequenceKind::Revert;
                let result = history::pick_many(
                    &self.repo,
                    &self.operations.sequence_store,
                    kind,
                    commit_ids,
                    remote::TransferOp::new(operation_id),
                    events,
                );
                undo::relabel_sequence(&mut self.undo_recorder, kind, &result);
                let _ = reply.send(result);
            }
            Command::ContinueSequence {
                no_verify,
                operation_id,
                events,
                reply,
            } => {
                let (kind, result) = history::continue_sequence(
                    &self.repo,
                    &self.operations.sequence_store,
                    no_verify,
                    remote::TransferOp::new(operation_id),
                    events,
                );
                if let Some(kind) = kind {
                    undo::relabel_sequence(&mut self.undo_recorder, kind, &result);
                }
                let _ = reply.send(result);
            }
            Command::AbortSequence { confirmed, reply } => history::abort_sequence(
                &self.repo,
                &self.operations.sequence_store,
                confirmed,
                reply,
            ),
            Command::ResetToCommit {
                commit_id,
                mode,
                reply,
            } => history::reset(&self.repo, commit_id, mode, reply),
            Command::CheckoutCommit {
                commit_id,
                operation_id,
                events,
                reply,
            } => history::checkout(
                &self.repo,
                commit_id,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::ForcePushCurrentBranch {
                remote_name,
                no_verify,
                operation_id,
                events,
                reply,
            } => remote::force_push_branch(
                &self.repo,
                &self.credential_service,
                remote_name,
                no_verify,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::GetMergeMessage { reply } => merge::message(&self.repo, reply),
            Command::ResolveAddDeleteConflict {
                path,
                choice,
                reply,
            } => merge::resolve_add_delete(&self.repo, path, choice, reply),
            Command::MarkResolved { path, force, reply } => {
                merge::mark_resolved(&self.repo, path, force, reply)
            }
            Command::ResolveWholeFile { path, side, reply } => {
                merge::resolve_whole_file(&self.repo, path, side, reply)
            }
            Command::PrepareDiffTool {
                path,
                source,
                reply,
            } => tools::prepare_diff_tool(&self.repo, path, source, reply),
            Command::PrepareMergeTool { path, reply } => {
                tools::prepare_merge_tool(&self.repo, path, reply)
            }
            Command::GetGitTools { reply } => tools::git_tools(&self.repo, reply),
            Command::GetFileSides {
                path,
                source,
                reply,
            } => tools::file_sides(&self.repo, path, source, reply),
            Command::ListRecentAuthors { reply } => history::recent_authors(&self.repo, reply),
            Command::CheckMergeTarget { path, reply } => {
                tools::check_merge_target(&self.repo, path, reply)
            }
            Command::ReadFileVersion {
                path,
                version,
                reply,
            } => tools::read_file_version(&self.repo, path, version, reply),
            Command::GetCommitTree {
                commit_id,
                dir,
                filter,
                reply,
            } => tree::get_commit_tree(&self.repo, commit_id, dir, filter, reply),
            Command::GetFileAtCommit {
                commit_id,
                path,
                reply,
            } => tree::get_file_at_commit(&self.repo, commit_id, path, reply),
            Command::SaveFileVersion {
                commit_id,
                path,
                destination,
                reply,
            } => tree::save_file_version(&self.repo, commit_id, path, destination, reply),
            Command::RestoreFileVersion {
                commit_id,
                path,
                reply,
            } => tree::restore_file_version(&self.repo, commit_id, path, reply),
            Command::CommitsSince { onto, reply } => rebase::commits_since(&self.repo, onto, reply),
            Command::StartRebase {
                onto,
                plan,
                edit,
                operation_id,
                events,
                reply,
            } => {
                let result = rebase::start(
                    &self.repo,
                    &onto,
                    plan,
                    self.operations.rebase_slot(),
                    remote::TransferOp::new(operation_id),
                    events,
                );
                let label = undo::start_rebase_label(&onto, edit.as_ref());
                undo::relabel_rebase_start(&mut self.undo_recorder, &self.repo, &label, &result);
                let _ = reply.send(result);
            }
            Command::RebaseContinue {
                operation_id,
                events,
                reply,
            } => rebase::continue_rebase(
                &self.repo,
                self.operations.rebase_slot(),
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::AbortRebase { reply } => {
                rebase::abort(&self.repo, self.operations.rebase_slot(), reply)
            }
            Command::FetchRemote {
                remote_name,
                prune,
                operation_id,
                events,
                reply,
            } => remote::fetch(
                &self.repo,
                &self.credential_service,
                &self.background_memory,
                remote_name,
                prune,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::PullCurrentUpstream {
                operation_id,
                events,
                reply,
            } => remote::pull(
                &self.repo,
                &self.credential_service,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::PushCurrentBranch {
                remote_name,
                no_verify,
                operation_id,
                events,
                reply,
            } => remote::push_current(
                &self.repo,
                &self.credential_service,
                remote_name,
                no_verify,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::PushTags {
                remote_name,
                names,
                no_verify,
                operation_id,
                events,
                reply,
            } => remote::push_tags(
                &self.repo,
                &self.credential_service,
                remote_name,
                names,
                no_verify,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::FetchAllRemotes {
                prune,
                background,
                operation_id,
                events,
                reply,
            } => fetch_all::run(
                &self.repo,
                &self.credential_service,
                &self.background_memory,
                background.map_or(
                    fetch_all::Mode::Manual { prune },
                    fetch_all::Mode::Background,
                ),
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::PushBranch {
                remote_name,
                local_branch,
                remote_branch,
                set_upstream,
                no_verify,
                operation_id,
                events,
                reply,
            } => remote::push_branch(
                &self.repo,
                &self.credential_service,
                remote::PushTarget {
                    remote_name,
                    local_branch,
                    remote_branch,
                    set_upstream,
                    no_verify,
                },
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::DeleteRemoteBranch {
                remote_name,
                branch_name,
                no_verify,
                operation_id,
                events,
                reply,
            } => remote::delete_remote_branch(
                &self.repo,
                &self.credential_service,
                remote_name,
                branch_name,
                no_verify,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::DeleteRemoteTag {
                remote_name,
                tag_name,
                no_verify,
                operation_id,
                events,
                reply,
            } => remote::delete_remote_tag(
                &self.repo,
                &self.credential_service,
                remote_name,
                tag_name,
                no_verify,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::DetectForgeRepository { reply } => forge::detect(&self.repo, reply),
            Command::SaveForgeToken {
                provider,
                account,
                token,
                reply,
            } => forge::save_token(&self.credential_service, provider, account, token, reply),
            Command::ForgetForgeToken {
                provider,
                account,
                reply,
            } => forge::forget_token(&self.credential_service, provider, account, reply),
            Command::ListPullRequests {
                remote_name,
                account,
                reply,
            } => forge::list_pull_requests(
                &self.repo,
                &self.credential_service,
                &self.pull_request_service,
                remote_name,
                account,
                reply,
            ),
            Command::CreatePullRequest {
                remote_name,
                account,
                create,
                reply,
            } => forge::create_pull_request(
                &self.repo,
                &self.credential_service,
                &self.pull_request_service,
                remote_name,
                account,
                create,
                reply,
            ),
            Command::GetUndoState { reply } => undo::get_state(&mut self.undo_recorder, reply),
            Command::Undo { reply } => {
                let _ = reply.send(
                    self.undo_recorder
                        .undo(&self.repo, guard, || self.operations.marker(&self.repo)),
                );
            }
            Command::Redo { reply } => {
                let _ = reply.send(
                    self.undo_recorder
                        .redo(&self.repo, guard, || self.operations.marker(&self.repo)),
                );
            }
        }
    }
}
