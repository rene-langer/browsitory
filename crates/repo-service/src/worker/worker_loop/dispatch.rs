//! The worker's `match` over `Command`: each arm hands its command to the module for its area.

use git_core::merge::MergeOutcome;

use super::read::{self, ReadState};
use super::WorkerLoop;
use crate::credentials::CredentialStore;
use crate::pull_requests::ForgeApi;
use crate::worker::state_lock::StateGuard;
use crate::worker::{
    branch, fetch_all, forge, history, merge, patch, rebase, reflog, remote, stash, status,
    submodule, tag, tools, tree, undo, worktree, Command,
};

impl<S, A> WorkerLoop<S, A>
where
    S: CredentialStore + Send + Sync + 'static,
    A: ForgeApi + Send + 'static,
{
    /// Runs `command`, after `before` gated it. `guard` is the state lock `acquire` took.
    pub(super) fn dispatch(&mut self, command: Command, guard: Option<&StateGuard>) {
        let mut state = ReadState {
            search_generation: &self.search_generation,
            walk_cache: &mut self.walk_cache,
            undo_recorder: &mut self.undo_recorder,
            operations: &self.operations,
            clean_merge: &self.clean_merge,
        };
        let Some(command) = read::dispatch(&mut self.repo, &mut state, command) else {
            return;
        };
        match command {
            Command::GetStatus { .. }
            | Command::GetChangeStamp { .. }
            | Command::GetCommitGraph { .. }
            | Command::LocateCommit { .. }
            | Command::GetMergedCommits { .. }
            | Command::GetWorkingDiff { .. }
            | Command::GetCommitDiff { .. }
            | Command::GetCommitFiles { .. }
            | Command::GetCommitStats { .. }
            | Command::GetCommitMessage { .. }
            | Command::GetCommitSignature { .. }
            | Command::GetCommitDetails { .. }
            | Command::GetBlame { .. }
            | Command::GetFileHistory { .. }
            | Command::GetLineHistory { .. }
            | Command::SearchCommits { .. }
            | Command::GetCommitRenameDiff { .. }
            | Command::GetRangeFiles { .. }
            | Command::GetRangeDiff { .. }
            | Command::GetWorkingTreeCompareFiles { .. }
            | Command::GetWorkingTreeCompareDiff { .. }
            | Command::GetBranchCompareRange { .. }
            | Command::GetPatch { .. }
            | Command::PreviewPatch { .. }
            | Command::ListBranches { .. }
            | Command::GetSwitchConflicts { .. }
            | Command::GetPullDefaults { .. }
            | Command::ListWorktrees { .. }
            | Command::ListSubmodules { .. }
            | Command::ListReflogRefs { .. }
            | Command::GetReflog { .. }
            | Command::ListRemotes { .. }
            | Command::ListRemoteBranches { .. }
            | Command::GetCurrentUpstream { .. }
            | Command::GetRemoteUpstreams { .. }
            | Command::GetRemoteAuthMode { .. }
            | Command::ListTags { .. }
            | Command::ListStashes { .. }
            | Command::GetStashFiles { .. }
            | Command::GetMergeDefaults { .. }
            | Command::GetConflictHunks { .. }
            | Command::GetHeadInfo { .. }
            | Command::GetMergeMessage { .. }
            | Command::PrepareDiffTool { .. }
            | Command::GetGitTools { .. }
            | Command::GetFileSides { .. }
            | Command::ListRecentAuthors { .. }
            | Command::CheckMergeTarget { .. }
            | Command::ReadFileVersion { .. }
            | Command::GetCommitTree { .. }
            | Command::GetFileAtCommit { .. }
            | Command::SaveFileVersion { .. }
            | Command::CommitsSince { .. }
            | Command::DetectForgeRepository { .. }
            | Command::GetUndoState { .. } => {
                unreachable!("`read::dispatch` runs the read lane's commands")
            }
            Command::Ping { reply } => {
                let _ = reply.send(());
            }
            #[cfg(test)]
            Command::PanicNow => {
                panic!("intentional panic for worker-crash-recovery test")
            }
            Command::StageFile { path, reply } => status::stage_file(&self.repo, path, reply),
            Command::UnstageFile { path, reply } => status::unstage_file(&self.repo, path, reply),
            Command::StagePaths { paths, reply } => status::stage_paths(&self.repo, paths, reply),
            Command::UnstagePaths { paths, reply } => {
                status::unstage_paths(&self.repo, paths, reply)
            }
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
            Command::ApplyPatch {
                content,
                how,
                operation_id,
                events,
                reply,
            } => patch::apply_patch(
                &self.repo,
                content,
                how,
                remote::TransferOp::new(operation_id),
                events,
                reply,
            ),
            Command::IgnorePath { pattern, reply } => {
                status::ignore_path(&self.repo, pattern, reply)
            }
            Command::IgnorePaths { patterns, reply } => {
                status::ignore_paths(&self.repo, patterns, reply)
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
            Command::CreateBranch {
                name,
                start_point,
                checkout,
                reply,
            } => branch::create(&self.repo, name, start_point, checkout, reply),
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
            Command::InitSubmodule { path, reply } => submodule::init(&self.repo, path, reply),
            Command::UpdateSubmodule {
                path,
                recursive,
                reply,
            } => submodule::update(&self.repo, path, recursive, reply),
            Command::RestoreReflogEntry {
                reference,
                new_id,
                reply,
            } => reflog::restore(&self.repo, reference, new_id, reply),
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
            Command::CreateTag {
                name,
                message,
                target,
                reply,
            } => tag::create(&self.repo, name, message, target, reply),
            Command::DeleteTag { name, reply } => tag::delete(&self.repo, name, reply),
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
            } => {
                let (mut host, credentials, _) = self.transfer_parts();
                remote::force_push_branch(
                    &mut host,
                    credentials,
                    remote_name,
                    no_verify,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
            }
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
            Command::PrepareMergeTool { path, reply } => {
                tools::prepare_merge_tool(&self.repo, path, reply)
            }
            Command::RestoreFileVersion {
                commit_id,
                path,
                reply,
            } => tree::restore_file_version(&self.repo, commit_id, path, reply),
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
            } => {
                let (mut host, credentials, memory) = self.transfer_parts();
                remote::fetch(
                    &mut host,
                    credentials,
                    memory,
                    remote_name,
                    prune,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
            }
            Command::PullCurrentUpstream {
                operation_id,
                events,
                reply,
            } => {
                let (mut host, credentials, _) = self.transfer_parts();
                remote::pull(
                    &mut host,
                    credentials,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
            }
            Command::PushCurrentBranch {
                remote_name,
                no_verify,
                operation_id,
                events,
                reply,
            } => {
                let (mut host, credentials, _) = self.transfer_parts();
                remote::push_current(
                    &mut host,
                    credentials,
                    remote_name,
                    no_verify,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
            }
            Command::PushTags {
                remote_name,
                names,
                no_verify,
                operation_id,
                events,
                reply,
            } => {
                let (mut host, credentials, _) = self.transfer_parts();
                remote::push_tags(
                    &mut host,
                    credentials,
                    remote_name,
                    names,
                    no_verify,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
            }
            Command::FetchAllRemotes {
                prune,
                background,
                operation_id,
                events,
                reply,
            } => {
                let (mut host, credentials, memory) = self.transfer_parts();
                fetch_all::run(
                    &mut host,
                    credentials,
                    memory,
                    background.map_or(
                        fetch_all::Mode::Manual { prune },
                        fetch_all::Mode::Background,
                    ),
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
            }
            Command::PushBranch {
                remote_name,
                local_branch,
                remote_branch,
                set_upstream,
                no_verify,
                operation_id,
                events,
                reply,
            } => {
                let (mut host, credentials, _) = self.transfer_parts();
                remote::push_branch(
                    &mut host,
                    credentials,
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
                );
            }
            Command::DeleteRemoteBranch {
                remote_name,
                branch_name,
                no_verify,
                operation_id,
                events,
                reply,
            } => {
                let (mut host, credentials, _) = self.transfer_parts();
                remote::delete_remote_branch(
                    &mut host,
                    credentials,
                    remote_name,
                    branch_name,
                    no_verify,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
            }
            Command::DeleteRemoteTag {
                remote_name,
                tag_name,
                no_verify,
                operation_id,
                events,
                reply,
            } => {
                let (mut host, credentials, _) = self.transfer_parts();
                remote::delete_remote_tag(
                    &mut host,
                    credentials,
                    remote_name,
                    tag_name,
                    no_verify,
                    remote::TransferOp::new(operation_id),
                    events,
                    reply,
                );
            }
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
