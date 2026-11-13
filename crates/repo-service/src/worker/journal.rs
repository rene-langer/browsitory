//! The worker's side of the operation log (#887): what each command is called in the log
//! ([`describe`], every variant named, so a new command must be placed), and the [`Tracker`]
//! that notes one command's entry from its start to its outcome.
//!
//! Most commands answer their reply once they are done, so the tracker swaps the reply for a
//! proxy, reads what the command answered when its step ends (`WorkerLoop::step`) and passes it
//! on. A fetch or push answers its operation id at once and reports its outcome in its last
//! transfer event; those commands are `Outcome::Transfer`, their reply is left alone, and the
//! code that sends that event notes the outcome here too ([`note_transfer`]), on the worker's
//! thread, where the tracker reads it when the step ends. A pull answers when it is done, but
//! notes its transfer error the same way.
//!
//! The programs a command runs (hooks, signing programs) are collected with
//! `git_core::process::record_runs` around the whole step.

use std::any::Any;
use std::cell::RefCell;
use std::sync::Arc;

use git_core::process::{ProgramRunStatus, RunLog};
use git_core::remote::TransferErrorKind;

use crate::oplog::{
    Finish, Journal, OperationOutcome, OperationParam, RemoteFailure, Rerun, Start,
};
use crate::wire::{ErrorDto, ErrorKind};

use super::policy::reply_of;
use super::Command;

/// Where a command's outcome comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Outcome {
    /// Its reply, sent when it is done.
    Reply,
    /// Its last transfer event (see the module docs).
    Transfer,
}

/// A command as the log shows it.
pub(super) struct Described {
    pub start: Start,
    pub outcome: Outcome,
}

fn entry(name: &str) -> Start {
    Start {
        name: name.to_string(),
        ..Start::default()
    }
}

fn on(name: &str, target: impl Into<String>) -> Start {
    Start {
        target: Some(target.into()),
        ..entry(name)
    }
}

impl Start {
    fn param(mut self, name: &str, value: impl Into<String>) -> Self {
        self.params.push(OperationParam {
            name: name.to_string(),
            value: value.into(),
        });
        self
    }

    fn flag(self, name: &str, value: bool) -> Self {
        self.param(name, if value { "yes" } else { "no" })
    }

    fn rerun(mut self, rerun: Rerun) -> Self {
        self.rerun = Some(rerun);
        self
    }

    fn reply(self) -> Option<Described> {
        Some(Described {
            start: self,
            outcome: Outcome::Reply,
        })
    }

    fn transfer(self) -> Option<Described> {
        Some(Described {
            start: self,
            outcome: Outcome::Transfer,
        })
    }
}

fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or("").trim().to_string()
}

fn short(oid: &str) -> String {
    oid.chars().take(7).collect()
}

/// One item as it is, several as a count.
fn several(items: &[String], noun: &str) -> String {
    match items {
        [item] => item.clone(),
        _ => format!("{} {noun}", items.len()),
    }
}

fn commits(ids: &[String]) -> String {
    match ids {
        [id] => short(id),
        _ => format!("{} commits", ids.len()),
    }
}

fn prune(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "as configured",
    }
}

fn lines(old_start: u32, new_start: u32) -> String {
    format!("-{old_start} +{new_start}")
}

/// How the log names `command`; `None` for the commands it leaves out: reads, and the worker's
/// own liveness probe.
pub(super) fn describe(command: &Command) -> Option<Described> {
    match command {
        Command::Ping { .. } => None,
        #[cfg(test)]
        Command::PanicNow => None,

        // Reads.
        Command::GetStatus { .. }
        | Command::GetChangeStamp { .. }
        | Command::GetStatusWithStamp { .. }
        | Command::ListUntrackedFolder { .. }
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
        | Command::GetMergeMessage { .. }
        | Command::PrepareDiffTool { .. }
        | Command::PrepareMergeTool { .. }
        | Command::GetGitTools { .. }
        | Command::GetFileSides { .. }
        | Command::ListRecentAuthors { .. }
        | Command::CheckMergeTarget { .. }
        | Command::ReadFileVersion { .. }
        | Command::GetCommitTree { .. }
        | Command::GetFileAtCommit { .. }
        | Command::CommitsSince { .. }
        | Command::GetEditRest { .. }
        | Command::GetHeadInfo { .. }
        | Command::DetectForgeRepository { .. }
        | Command::ListPullRequests { .. }
        | Command::GetUndoState { .. } => None,

        // Staging and discarding.
        Command::StageFile { path, .. } => on("Stage", path).reply(),
        Command::UnstageFile { path, .. } => on("Unstage", path).reply(),
        Command::StagePaths { paths, .. } => on("Stage", several(paths, "files")).reply(),
        Command::UnstagePaths { paths, .. } => on("Unstage", several(paths, "files")).reply(),
        Command::StageHunk {
            path,
            old_start,
            new_start,
            ..
        } => on("Stage hunk", path)
            .param("Lines", lines(*old_start, *new_start))
            .reply(),
        Command::UnstageHunk {
            path,
            old_start,
            new_start,
            ..
        } => on("Unstage hunk", path)
            .param("Lines", lines(*old_start, *new_start))
            .reply(),
        Command::DiscardHunk {
            path,
            old_start,
            new_start,
            ..
        } => on("Discard hunk", path)
            .param("Lines", lines(*old_start, *new_start))
            .reply(),
        Command::StageLines { path, .. } => on("Stage lines", path).reply(),
        Command::UnstageLines { path, .. } => on("Unstage lines", path).reply(),
        Command::DiscardLines { path, .. } => on("Discard lines", path).reply(),
        Command::DiscardFile { path, .. } => on("Discard changes", path).reply(),
        Command::DiscardAllUnstaged { paths, .. } => {
            on("Discard all unstaged changes", several(paths, "files")).reply()
        }
        Command::ApplyCommitChange { change, .. } => on(
            if change.reverse {
                "Revert change"
            } else {
                "Apply change"
            },
            change.path.clone(),
        )
        .param("Commit", short(&change.commit_id))
        .flag("Hunk only", change.hunk.is_some())
        .flag("Stage", change.stage)
        .reply(),
        Command::ApplyPatch { how, .. } => entry("Apply patch")
            .param("How", format!("{how:?}"))
            .reply(),
        Command::IgnorePath { pattern, .. } => on("Ignore", pattern).reply(),
        Command::IgnorePaths { patterns, .. } => {
            on("Ignore", several(patterns, "patterns")).reply()
        }
        Command::CreateRootFile { name, .. } => on("Create file", name).reply(),

        // Commits.
        Command::Commit {
            message, no_verify, ..
        } => on("Commit", first_line(message))
            .flag("Skip hooks", *no_verify)
            .reply(),
        Command::AmendCommit {
            message,
            no_verify,
            author,
            ..
        } => on("Amend commit", first_line(message))
            .flag("Skip hooks", *no_verify)
            .flag("New author", author.is_some())
            .reply(),
        Command::SetIdentity {
            name,
            email,
            global,
            ..
        } => on("Set identity", format!("{name} <{email}>"))
            .flag("All repositories", *global)
            .reply(),

        // Branches, worktrees and submodules.
        Command::CreateBranch {
            name,
            start_point,
            checkout,
            ..
        } => on("Create branch", name)
            .param("Start point", start_point.clone())
            .flag("Switch to it", *checkout)
            .reply(),
        Command::StashAndSwitch { name, reapply, .. } => {
            on("Switch branch with local changes", name)
                .flag("Reapply the changes", *reapply)
                .reply()
        }
        Command::SwitchBranch { name, .. } => on("Switch branch", name).reply(),
        Command::DeleteBranch { name, force, .. } => {
            on("Delete branch", name).flag("Force", *force).reply()
        }
        Command::RenameBranch {
            old_name, new_name, ..
        } => on("Rename branch", old_name)
            .param("New name", new_name.clone())
            .reply(),
        Command::CreateWorktree {
            path,
            branch,
            start_point,
            ..
        } => {
            let start = on("Add worktree", path.to_string_lossy()).param("Branch", branch.clone());
            match start_point {
                Some(point) => start.param("Start point", point.clone()),
                None => start,
            }
            .reply()
        }
        Command::RemoveWorktree { name, .. } => on("Remove worktree", name).reply(),
        Command::PruneWorktrees { .. } => entry("Prune worktrees").reply(),
        Command::InitSubmodule { path, .. } => on("Initialize submodule", path).reply(),
        Command::UpdateSubmodule {
            path, recursive, ..
        } => on("Update submodule", path)
            .flag("Recursive", *recursive)
            .reply(),
        Command::RestoreReflogEntry {
            reference, new_id, ..
        } => on("Restore from reflog", reference)
            .param("Commit", short(new_id))
            .reply(),

        // Remotes and credentials. The tokens are never noted.
        Command::SaveHttpsCredential { remote_name, .. } => {
            on("Save HTTPS credential", remote_name).reply()
        }
        Command::ForgetHttpsCredential { remote_name, .. } => {
            on("Forget HTTPS credential", remote_name).reply()
        }
        Command::SetRemoteAuthMode {
            remote_name, mode, ..
        } => on("Set remote authentication", remote_name)
            .param("Mode", format!("{mode:?}"))
            .reply(),
        Command::AddRemote {
            name,
            fetch_url,
            push_url,
            ..
        } => {
            let start = on("Add remote", name).param("Fetch URL", fetch_url.clone());
            match push_url {
                Some(url) => start.param("Push URL", url.clone()),
                None => start,
            }
            .reply()
        }
        Command::RenameRemote {
            old_name, new_name, ..
        } => on("Rename remote", old_name)
            .param("New name", new_name.clone())
            .reply(),
        Command::UpdateRemoteUrls {
            name,
            fetch_url,
            push_url,
            ..
        } => {
            let start = on("Edit remote", name).param("Fetch URL", fetch_url.clone());
            match push_url {
                Some(url) => start.param("Push URL", url.clone()),
                None => start,
            }
            .reply()
        }
        Command::RemoveRemote {
            name,
            clear_upstreams,
            ..
        } => on("Remove remote", name)
            .flag("Clear upstreams", *clear_upstreams)
            .reply(),
        Command::SetCurrentUpstream {
            remote_name,
            remote_branch,
            ..
        } => on("Set upstream", format!("{remote_name}/{remote_branch}")).reply(),
        Command::ClearCurrentUpstream { .. } => entry("Clear upstream").reply(),

        // Tags and stashes.
        Command::CreateTag {
            name,
            message,
            target,
            ..
        } => on("Create tag", name)
            .param("Commit", target.as_deref().map_or("HEAD".into(), short))
            .flag("Annotated", message.is_some())
            .reply(),
        Command::DeleteTag { name, .. } => on("Delete tag", name).reply(),
        Command::SaveStash { options, .. } => {
            let start = entry("Stash changes")
                .flag("Keep staged changes", options.keep_index)
                .flag("Include untracked files", options.include_untracked);
            match options.message.as_deref().map(str::trim) {
                Some(message) if !message.is_empty() => Start {
                    target: Some(first_line(message)),
                    ..start
                },
                _ => start,
            }
            .reply()
        }
        Command::ApplyStash { index, .. } => {
            on("Apply stash", format!("stash@{{{index}}}")).reply()
        }
        Command::PopStash { index, .. } => on("Pop stash", format!("stash@{{{index}}}")).reply(),
        Command::DropStash { index, .. } => on("Drop stash", format!("stash@{{{index}}}")).reply(),

        // Merges and conflicts.
        Command::StartMerge {
            branch_name,
            options,
            ..
        } => on("Merge", branch_name)
            .param("Fast-forward", format!("{:?}", options.fast_forward))
            .flag("Squash", options.squash)
            .reply(),
        Command::AbortMerge { .. } => entry("Abort merge").reply(),
        Command::ResolveConflict { path, .. } => on("Resolve conflict", path).reply(),
        Command::ResolveAddDeleteConflict { path, choice, .. } => on("Resolve conflict", path)
            .param("Choice", format!("{choice:?}"))
            .reply(),
        Command::MarkResolved { path, force, .. } => on("Mark resolved", path)
            .flag("Despite conflict markers", *force)
            .reply(),
        Command::ResolveWholeFile { path, side, .. } => on("Resolve conflict", path)
            .param("Side", format!("{side:?}"))
            .reply(),
        Command::SaveFileVersion {
            commit_id, path, ..
        } => on("Save file version", path)
            .param("Commit", short(commit_id))
            .reply(),
        Command::RestoreFileVersion {
            commit_id, path, ..
        } => on("Restore file version", path)
            .param("Commit", short(commit_id))
            .reply(),

        // Rebases and history edits.
        Command::StartRebase { onto, plan, .. } => on("Rebase", onto)
            .param("Commits", plan.len().to_string())
            .reply(),
        Command::RebaseContinue { .. } => entry("Continue rebase").reply(),
        Command::RebaseCommitPart { message, .. } => {
            on("Commit part of a split", first_line(message)).reply()
        }
        Command::FoldStaged { commit_id, .. } => {
            on("Fold staged changes into", short(commit_id)).reply()
        }
        Command::AbortRebase { .. } => entry("Abort rebase").reply(),

        // Cherry-picks, reverts, resets and checkouts.
        Command::CherryPick { commit_id, .. } => on("Cherry-pick", short(commit_id)).reply(),
        Command::RevertCommit { commit_id, .. } => on("Revert", short(commit_id)).reply(),
        Command::CherryPickMany { commit_ids, .. } => {
            on("Cherry-pick", commits(commit_ids)).reply()
        }
        Command::RevertMany { commit_ids, .. } => on("Revert", commits(commit_ids)).reply(),
        Command::ContinueSequence { no_verify, .. } => entry("Continue cherry-pick or revert")
            .flag("Skip hooks", *no_verify)
            .reply(),
        Command::AbortSequence { .. } => entry("Abort cherry-pick or revert").reply(),
        Command::ResetToCommit {
            commit_id, mode, ..
        } => on("Reset", short(commit_id))
            .param("Mode", format!("{mode:?}"))
            .reply(),
        Command::CheckoutCommit { commit_id, .. } => on("Check out", short(commit_id)).reply(),

        // Fetches, pulls and pushes.
        Command::FetchRemote {
            remote_name,
            prune: p,
            ..
        } => on("Fetch", remote_name)
            .param("Prune", prune(*p))
            .rerun(Rerun::Fetch {
                remote_name: remote_name.clone(),
                prune: *p,
            })
            .transfer(),
        Command::FetchAllRemotes {
            prune: p,
            background,
            ..
        } => entry(if background.is_some() {
            "Fetch all (automatic)"
        } else {
            "Fetch all"
        })
        .param("Prune", prune(*p))
        .rerun(Rerun::FetchAll { prune: *p })
        .transfer(),
        Command::PullCurrentUpstream { .. } => entry("Pull").rerun(Rerun::Pull).reply(),
        Command::PushCurrentBranch {
            remote_name,
            no_verify,
            ..
        } => on("Push", remote_name)
            .flag("Skip hooks", *no_verify)
            .rerun(Rerun::PushCurrentBranch {
                remote_name: remote_name.clone(),
                no_verify: *no_verify,
            })
            .transfer(),
        // Not offered again: a forced push overwrites the remote branch.
        Command::ForcePushCurrentBranch {
            remote_name,
            no_verify,
            ..
        } => on("Force push", remote_name)
            .flag("Skip hooks", *no_verify)
            .transfer(),
        Command::PushTags {
            remote_name,
            names,
            no_verify,
            ..
        } => on("Push tags", remote_name)
            .param("Tags", names.join(", "))
            .flag("Skip hooks", *no_verify)
            .rerun(Rerun::PushTags {
                remote_name: remote_name.clone(),
                names: names.clone(),
                no_verify: *no_verify,
            })
            .transfer(),
        Command::PushBranch {
            remote_name,
            local_branch,
            remote_branch,
            set_upstream,
            no_verify,
            ..
        } => on("Push", remote_name)
            .param("Branch", format!("{local_branch} → {remote_branch}"))
            .flag("Set upstream", *set_upstream)
            .flag("Skip hooks", *no_verify)
            .rerun(Rerun::PushBranch {
                remote_name: remote_name.clone(),
                local_branch: local_branch.clone(),
                remote_branch: remote_branch.clone(),
                set_upstream: *set_upstream,
                no_verify: *no_verify,
            })
            .transfer(),
        Command::DeleteRemoteBranch {
            remote_name,
            branch_name,
            ..
        } => on(
            "Delete remote branch",
            format!("{remote_name}/{branch_name}"),
        )
        .transfer(),
        Command::DeleteRemoteTag {
            remote_name,
            tag_name,
            ..
        } => on("Delete remote tag", tag_name)
            .param("Remote", remote_name.clone())
            .transfer(),

        // Forge accounts and pull requests.
        Command::SaveForgeToken {
            provider, account, ..
        } => on("Save forge token", account.clone())
            .param("Provider", format!("{provider:?}"))
            .reply(),
        Command::ForgetForgeToken {
            provider, account, ..
        } => on("Forget forge token", account.clone())
            .param("Provider", format!("{provider:?}"))
            .reply(),
        Command::CreatePullRequest { remote_name, .. } => {
            on("Create pull request", remote_name).reply()
        }

        // The undo stack.
        Command::Undo { .. } => entry("Undo").reply(),
        Command::Redo { .. } => entry("Redo").reply(),
    }
}

/// A fetch's, pull's or push's outcome, noted where its last transfer event is sent.
#[derive(Debug, Default)]
pub(super) struct TransferNote {
    pub error: Option<TransferErrorKind>,
    /// What went wrong, in the transport's words (redacted when kept).
    pub message: Option<String>,
    pub remote_failures: Vec<RemoteFailure>,
    /// Refused before it started: its reply's error.
    pub refused: Option<ErrorDto>,
}

thread_local! {
    static TRANSFER: RefCell<Option<TransferNote>> = const { RefCell::new(None) };
}

/// Notes a fetch's, pull's or push's outcome for the tracker (see the module docs).
pub(super) fn note_transfer(note: TransferNote) {
    TRANSFER.with(|slot| *slot.borrow_mut() = Some(note));
}

/// Notes a transfer refused with `error` before it started (its reply carries the error and no
/// transfer event follows).
pub(super) fn note_refused(error: &ErrorDto) {
    note_transfer(TransferNote {
        refused: Some(error.clone()),
        ..TransferNote::default()
    });
}

fn take_transfer_note() -> Option<TransferNote> {
    TRANSFER.with(|slot| slot.borrow_mut().take())
}

/// Reads a proxied reply's answer, passes it on, and says what it means (see
/// `policy::Reply::observe`); `None` when nothing was answered.
pub(super) type Observed = Box<dyn FnOnce() -> Option<Seen>>;

/// What a command answered, as far as the log cares.
pub(super) struct Seen {
    outcome: OperationOutcome,
    message: Option<String>,
    error_kind: Option<ErrorKind>,
}

impl Seen {
    fn succeeded() -> Self {
        Self {
            outcome: OperationOutcome::Succeeded,
            message: None,
            error_kind: None,
        }
    }

    fn ended(outcome: OperationOutcome, message: String) -> Self {
        Self {
            outcome,
            message: Some(message),
            error_kind: None,
        }
    }
}

/// Reads what an answer says about the outcome; `T` is the command's reply type.
pub(super) fn seen<T: 'static>(value: &Result<T, ErrorDto>) -> Seen {
    match value {
        Ok(value) => seen_ok(value),
        Err(error) => Seen {
            outcome: if error.kind == ErrorKind::Cancelled {
                OperationOutcome::Cancelled
            } else {
                OperationOutcome::Failed
            },
            message: Some(error.message.clone()),
            error_kind: Some(error.kind),
        },
    }
}

fn conflicts(files: &[String]) -> String {
    match files.len() {
        1 => "Stopped on conflicts in 1 file.".to_string(),
        count => format!("Stopped on conflicts in {count} files."),
    }
}

fn hook_failed(hook: &str) -> String {
    format!("The {hook} hook failed.")
}

/// The answers that report a failure or a stop as an outcome, not an error.
fn seen_ok(value: &dyn Any) -> Seen {
    use git_core::commit::CommitOutcome;
    use git_core::pick::{PickOutcome, SequenceOutcome};
    use OperationOutcome::{Cancelled, Failed, Stopped};
    if let Some(outcome) = value.downcast_ref::<CommitOutcome>() {
        if let CommitOutcome::HookFailed { hook, .. } = outcome {
            return Seen::ended(Failed, hook_failed(hook));
        }
    } else if let Some(outcome) = value.downcast_ref::<PickOutcome>() {
        match outcome {
            PickOutcome::HookFailed { hook, .. } => return Seen::ended(Failed, hook_failed(hook)),
            PickOutcome::Conflicted { files } => return Seen::ended(Stopped, conflicts(files)),
            PickOutcome::Committed { .. } => {}
        }
    } else if let Some(outcome) = value.downcast_ref::<SequenceOutcome>() {
        match outcome {
            SequenceOutcome::HookFailed { hook, .. } => {
                return Seen::ended(Failed, hook_failed(hook))
            }
            SequenceOutcome::Conflicted { files, .. } => {
                return Seen::ended(Stopped, conflicts(files))
            }
            SequenceOutcome::Stopped { .. } => {
                return Seen::ended(Cancelled, "Cancelled after a step was committed.".into())
            }
            SequenceOutcome::Completed { .. } => {}
        }
    } else if let Some(outcome) = value.downcast_ref::<git_core::patch::PatchApplyOutcome>() {
        if let git_core::patch::PatchApplyOutcome::HookFailed { hook, .. } = outcome {
            return Seen::ended(Failed, hook_failed(hook));
        }
    } else if let Some(git_core::merge::MergeOutcome::Conflicted { files }) =
        value.downcast_ref::<git_core::merge::MergeOutcome>()
    {
        return Seen::ended(Stopped, conflicts(files));
    } else if let Some(git_core::rebase::RebaseStepResult::Conflicted { files }) =
        value.downcast_ref::<git_core::rebase::RebaseStepResult>()
    {
        return Seen::ended(Stopped, conflicts(files));
    } else if let Some(git_core::stash::StashApplyOutcome::Conflicted { files }) =
        value.downcast_ref::<git_core::stash::StashApplyOutcome>()
    {
        return Seen::ended(Stopped, conflicts(files));
    } else if let Some(git_core::branch::StashSwitchOutcome::Conflicted { files }) =
        value.downcast_ref::<git_core::branch::StashSwitchOutcome>()
    {
        return Seen::ended(Stopped, conflicts(files));
    } else if let Some(outcome) = value.downcast_ref::<git_core::remote::PullOutcome>() {
        use git_core::remote::PullOutcome;
        match outcome {
            PullOutcome::Diverged { .. } => {
                return Seen::ended(Stopped, "The branch and its upstream have diverged.".into())
            }
            PullOutcome::NoFastForward { .. } => {
                return Seen::ended(Stopped, "The pull needs a merge commit.".into())
            }
            PullOutcome::UpToDate | PullOutcome::FastForwarded { .. } => {}
        }
    } else if let Some(outcome) = value.downcast_ref::<super::UndoOutcome>() {
        use super::UndoOutcome;
        match outcome {
            UndoOutcome::RepositoryChanged { .. } => {
                return Seen::ended(Failed, "The repository changed since.".into())
            }
            UndoOutcome::BranchHeld { message, .. } => return Seen::ended(Failed, message.clone()),
            UndoOutcome::OperationInProgress => {
                return Seen::ended(Failed, "An operation is in progress.".into())
            }
            UndoOutcome::Done { .. } | UndoOutcome::Nothing => {}
        }
    }
    Seen::succeeded()
}

/// Notes one command in the log from its start to its outcome. Dropped without `end` (the
/// worker panicked), it ends the entry as failed.
pub(super) struct Tracker {
    journal: Arc<Journal>,
    id: u64,
    outcome: Outcome,
    seen: Option<Observed>,
    runs: RunLog,
    ended: bool,
}

impl Tracker {
    /// Starts `command`'s entry in `journal`, when the log names it.
    pub(super) fn start(journal: &Arc<Journal>, command: &mut Command) -> Option<Self> {
        let described = describe(command)?;
        let seen = match described.outcome {
            Outcome::Reply => reply_of(command).map(|reply| reply.observe()),
            Outcome::Transfer => None,
        };
        // A note left by a command that ended without its tracker (it can't happen; but it
        // must never be read as this command's).
        take_transfer_note();
        let id = journal.begin(described.start);
        Some(Self {
            journal: Arc::clone(journal),
            id,
            outcome: described.outcome,
            seen,
            runs: RunLog::new(),
            ended: false,
        })
    }

    /// The log the step's program runs go to.
    pub(super) fn runs(&self) -> &RunLog {
        &self.runs
    }

    /// Ends the entry with what the command answered (or noted), and passes the reply on.
    pub(super) fn end(mut self) {
        self.ended = true;
        let seen = self.seen.take().and_then(|seen| seen());
        let note = take_transfer_note();
        let runs = self.runs.take();
        let mut finish = match (self.outcome, seen) {
            (Outcome::Reply, Some(seen)) => Finish {
                outcome: seen.outcome,
                message: seen.message,
                error_kind: seen.error_kind,
                ..Finish::succeeded()
            },
            (Outcome::Reply, None) => worker_stopped(),
            (Outcome::Transfer, _) => Finish::succeeded(),
        };
        if let Some(note) = note {
            apply_note(&mut finish, note, self.outcome);
        } else if self.outcome == Outcome::Transfer {
            finish = Finish {
                message: Some("The operation reported no outcome.".into()),
                ..failed()
            };
        }
        // A command whose hook was cancelled fails with the hook's error.
        if finish.outcome == OperationOutcome::Failed
            && runs
                .last()
                .is_some_and(|run| run.status == ProgramRunStatus::Cancelled)
        {
            finish.outcome = OperationOutcome::Cancelled;
        }
        finish.runs = runs;
        self.journal.finish(self.id, finish);
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        if !self.ended {
            self.journal.finish(self.id, worker_stopped());
        }
    }
}

fn failed() -> Finish {
    Finish {
        outcome: OperationOutcome::Failed,
        ..Finish::succeeded()
    }
}

fn worker_stopped() -> Finish {
    let error = ErrorDto::worker_stopped_before_replying();
    Finish {
        message: Some(error.message),
        error_kind: Some(error.kind),
        ..failed()
    }
}

/// Adds a transfer's note to what its reply said (a pull) or stands for it (a fetch or push).
fn apply_note(finish: &mut Finish, note: TransferNote, outcome: Outcome) {
    if let Some(error) = note.refused {
        finish.outcome = OperationOutcome::Failed;
        finish.message = Some(error.message);
        finish.error_kind = Some(error.kind);
        return;
    }
    finish.remote_failures = note.remote_failures;
    let Some(kind) = note.error else {
        return;
    };
    finish.transfer_error = Some(kind);
    finish.outcome = if kind == TransferErrorKind::Cancelled {
        OperationOutcome::Cancelled
    } else {
        OperationOutcome::Failed
    };
    // The transport's own words say more than a pull's generic reply.
    if note.message.is_some() || outcome == Outcome::Transfer {
        finish.message = note.message;
    }
}
