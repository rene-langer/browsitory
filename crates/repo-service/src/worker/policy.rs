//! Each worker command's policy, stated in one place (#485): the undo entry it records, whether
//! it holds the state lock, refuses while that lock stays busy, holds its reply until the undo
//! recorder's `finish`, is refused while a rebase or a cherry-pick/revert sequence is stored, and
//! whether it aborts a paused operation, and whether it may run while the worker waits for a
//! transfer (its lane, #734).
//!
//! [`of`] names every `Command` variant (no catch-all), so a new command doesn't compile until it
//! is given a policy, nor until `policy_tests.rs` pins that policy in a table row (#518). The
//! worker loop, `undo` and `gate` only read the result; the typed reply each of them answers or
//! holds is reached through [`reply_of`], which also names every variant.
use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;

use git_core::patch::PatchApply;
use git_core::reset::ResetMode;
use git_core::undo::SnapshotScope::{self, Full, RefsAndIndex};

use super::undo::{
    amend_label, count_label, fit, merge_label, quoted, reset_label, several, short,
    start_rebase_label, with_path, HeldReplies, UndoPlan, DISCARD_BUSY, UNDO_BUSY,
};
use super::Command;

/// How the worker treats one command; see the module docs. Built only by [`of`], from the
/// constructors below, so the pairings between the fields can't drift apart: everything recorded
/// or ending a paused operation holds its reply, and everything holding its reply holds the
/// state lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CommandPolicy {
    /// The undo entry the command records (`None`: it records nothing itself, though it may end
    /// a pending entry).
    pub undo: Option<UndoPlan>,
    /// Holds the state lock (#172) from before `settle` until its `finish`: everything recorded,
    /// everything that ends a paused operation, Undo/Redo, and the gated commands that start no
    /// operation (stash save, worktree add, #174), whose gate check and change then can't
    /// interleave with the other window starting a rebase or sequence. Reads never wait.
    pub locks_state: bool,
    /// Refused with this reply, instead of running unrecorded, while another holder keeps the
    /// state lock past `LOCK_WAIT` (#225); such a command waits for the lock even inside a burst
    /// (#197).
    pub busy: Option<BusyReply>,
    /// Its reply is held until the recorder has taken its "after" snapshot (when the recorder
    /// tracks the command at all): everything that can change what the recorder snapshots.
    pub holds_reply: bool,
    /// Whether a stored rebase or sequence refuses it.
    pub gate: Gate,
    /// Aborts a paused operation: its `finish` drops the pending entry instead of recording it.
    pub aborts: bool,
    /// Where the command may run (#734).
    pub lane: Lane,
}

/// Whether a command may run while the worker waits for a fetch, pull or push (#734).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Lane {
    /// Only once nothing else runs: everything that writes the repository or the app's state,
    /// talks to the network, or leads to a write (the merge tool).
    Main,
    /// Also while the worker waits for a transfer's helper (`worker_loop/read.rs`): it only reads
    /// the repository (or writes outside it, to a temp file or a path the user picked).
    Read,
}

/// The reply of a command refused while the state lock stays busy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BusyReply {
    /// `UNDO_BUSY`: Undo and Redo (D7-A).
    Undo,
    /// `DISCARD_BUSY`: commands that destroy work-tree content with no other way back (M4,
    /// option C). They would run unrecorded, so nothing could bring the content back.
    Discard,
}

impl BusyReply {
    pub fn text(self) -> &'static str {
        match self {
            BusyReply::Undo => UNDO_BUSY,
            BusyReply::Discard => DISCARD_BUSY,
        }
    }
}

/// Which commands a stored rebase or cherry-pick/revert sequence refuses (#171); see
/// `gate::gate_during_operation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Gate {
    /// Runs: reads, fetch, push, staging, conflict resolution, other branches' management.
    Open,
    /// Refused: it starts another operation or moves HEAD or the index under the paused one.
    Refused,
    /// Refused when this branch is the one the stored operation protects
    /// (`GateScope::branch`): git would let it go under the operation once it isn't checked
    /// out, and a renamed branch can't be rewound to by Abort.
    Branch(String),
    /// Refused while `GateScope::refuse_stash_save`: a stash would carry the paused step's index
    /// away.
    StashSave,
}

impl CommandPolicy {
    /// No lock, no undo entry, never refused.
    fn free() -> Self {
        CommandPolicy {
            undo: None,
            locks_state: false,
            busy: None,
            holds_reply: false,
            gate: Gate::Open,
            aborts: false,
            lane: Lane::Main,
        }
    }

    /// A read: like `free`, and it may run while the worker waits for a transfer (#734).
    fn read() -> Self {
        CommandPolicy {
            lane: Lane::Read,
            ..Self::free()
        }
    }

    /// Records an undo entry (which also covers everything that starts an operation, and
    /// `Commit`/`AmendCommit`, which end a merge or pick).
    fn recorded(label: String, scope: SnapshotScope) -> Self {
        CommandPolicy {
            undo: Some(UndoPlan { label, scope }),
            locks_state: true,
            holds_reply: true,
            ..Self::free()
        }
    }

    /// Records nothing and ends nothing, but holds the state lock.
    fn locked() -> Self {
        CommandPolicy {
            locks_state: true,
            ..Self::free()
        }
    }

    /// Continues a paused operation, which may end it and so record its pending entry.
    fn continuing() -> Self {
        CommandPolicy {
            locks_state: true,
            holds_reply: true,
            ..Self::free()
        }
    }

    /// Aborts a paused operation, which drops its pending entry.
    fn aborting() -> Self {
        CommandPolicy {
            aborts: true,
            ..Self::continuing()
        }
    }

    fn refused_when_busy(self, busy: BusyReply) -> Self {
        debug_assert!(
            self.locks_state,
            "only a locking command can find the lock busy"
        );
        CommandPolicy {
            busy: Some(busy),
            ..self
        }
    }

    fn gated(self, gate: Gate) -> Self {
        CommandPolicy { gate, ..self }
    }
}

/// The policy of `command`. Every variant is named, so a new command must be placed here.
pub(super) fn of(command: &Command) -> CommandPolicy {
    use BusyReply::Discard;
    use CommandPolicy as P;
    use Gate::{Refused, StashSave};
    match command {
        Command::Ping { .. } => P::free(),
        #[cfg(test)]
        Command::PanicNow => P::free(),

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
        | Command::GetPatch { .. }
        | Command::PreviewPatch { .. }
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
        | Command::ListBranches { .. }
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
        | Command::GetGitTools { .. }
        | Command::GetFileSides { .. }
        | Command::ListRecentAuthors { .. }
        | Command::CheckMergeTarget { .. }
        | Command::ReadFileVersion { .. }
        | Command::GetCommitTree { .. }
        | Command::GetFileAtCommit { .. }
        // Writes only outside the repository, to a path the user picked.
        | Command::SaveFileVersion { .. }
        | Command::CommitsSince { .. }
        | Command::GetHeadInfo { .. }
        | Command::DetectForgeRepository { .. }
        | Command::GetSwitchConflicts { .. }
        | Command::GetPullDefaults { .. }
        | Command::GetUndoState { .. } => P::read(),

        // Reads that stay on the main lane: the merge tool leads to a work tree write and a
        // `MarkResolved`, and listing pull requests is an HTTP request on the worker, which would
        // hold up the transfer's polling.
        Command::PrepareMergeTool { .. } | Command::ListPullRequests { .. } => P::free(),

        // Changes the undo stack doesn't cover: worktrees and submodules, remotes and their
        // settings, credentials, stashes other than save/apply/pop, conflict resolution (the
        // paused operation's entry covers it), transfers and forge calls. Branch, tag and reflog
        // changes are recorded below.
        Command::RemoveWorktree { .. }
        | Command::PruneWorktrees { .. }
        | Command::InitSubmodule { .. }
        | Command::UpdateSubmodule { .. }
        | Command::SaveHttpsCredential { .. }
        | Command::ForgetHttpsCredential { .. }
        | Command::SetRemoteAuthMode { .. }
        // Git config, like the remotes' settings (#689).
        | Command::SetIdentity { .. }
        | Command::AddRemote { .. }
        | Command::RenameRemote { .. }
        | Command::UpdateRemoteUrls { .. }
        | Command::RemoveRemote { .. }
        | Command::SetCurrentUpstream { .. }
        | Command::ClearCurrentUpstream { .. }
        | Command::DropStash { .. }
        | Command::ResolveConflict { .. }
        | Command::ResolveAddDeleteConflict { .. }
        | Command::MarkResolved { .. }
        | Command::ResolveWholeFile { .. }
        | Command::FetchRemote { .. }
        | Command::FetchAllRemotes { .. }
        | Command::ForcePushCurrentBranch { .. }
        | Command::PushCurrentBranch { .. }
        | Command::PushTags { .. }
        | Command::PushBranch { .. }
        | Command::DeleteRemoteBranch { .. }
        | Command::DeleteRemoteTag { .. }
        | Command::SaveForgeToken { .. }
        | Command::ForgetForgeToken { .. }
        | Command::CreatePullRequest { .. }
        // An empty new file the user then edits; there is nothing in it to undo (#624).
        | Command::CreateRootFile { .. } => P::free(),

        // Staging: recorded, and allowed during a paused operation.
        Command::StageFile { path, .. } => P::recorded(with_path("Stage ", path, ""), RefsAndIndex),
        Command::UnstageFile { path, .. } => {
            P::recorded(with_path("Unstage ", path, ""), RefsAndIndex)
        }
        Command::StageHunk { path, .. } => {
            P::recorded(with_path("Stage hunk in ", path, ""), RefsAndIndex)
        }
        Command::UnstageHunk { path, .. } => {
            P::recorded(with_path("Unstage hunk in ", path, ""), RefsAndIndex)
        }
        Command::StageLines { path, .. } => {
            P::recorded(with_path("Stage lines in ", path, ""), RefsAndIndex)
        }
        Command::UnstageLines { path, .. } => {
            P::recorded(with_path("Unstage lines in ", path, ""), RefsAndIndex)
        }
        Command::IgnorePath { pattern, .. } => P::recorded(with_path("Ignore ", pattern, ""), Full),
        // Several paths in one command, one undo step (#528).
        Command::StagePaths { paths, .. } => {
            P::recorded(several("Stage ", paths, "files"), RefsAndIndex)
        }
        Command::UnstagePaths { paths, .. } => {
            P::recorded(several("Unstage ", paths, "files"), RefsAndIndex)
        }
        Command::IgnorePaths { patterns, .. } => {
            P::recorded(several("Ignore ", patterns, "patterns"), Full)
        }

        // Discards: recorded, and refused rather than run unrecorded.
        Command::DiscardHunk { path, .. } => {
            P::recorded(with_path("Discard hunk in ", path, ""), Full).refused_when_busy(Discard)
        }
        Command::DiscardLines { path, .. } => {
            P::recorded(with_path("Discard lines in ", path, ""), Full).refused_when_busy(Discard)
        }
        Command::DiscardFile { path, .. } => {
            P::recorded(with_path("Discard changes to ", path, ""), Full).refused_when_busy(Discard)
        }
        Command::DiscardAllUnstaged { .. } => {
            P::recorded("Discard all unstaged changes".to_string(), Full).refused_when_busy(Discard)
        }
        // Overwrites the work tree file, so it is a discard too (#446).
        Command::RestoreFileVersion {
            commit_id, path, ..
        } => P::recorded(
            with_path("Restore ", path, &format!(" from {}", short(commit_id))),
            Full,
        )
        .refused_when_busy(Discard),

        // A commit's file or hunk, or a patch file, applied to the working tree (#448): recorded,
        // and like a discard refused rather than run unrecorded (it may merge into local edits).
        Command::ApplyCommitChange { change, .. } => {
            P::recorded(commit_change_label(change), Full).refused_when_busy(Discard)
        }
        // As commits (#658) it moves HEAD, so like a commit it can't run under a stored rebase or
        // sequence.
        Command::ApplyPatch { how, .. } => match how {
            PatchApply::WorkTree => {
                P::recorded("Apply patch".to_string(), Full).refused_when_busy(Discard)
            }
            PatchApply::Stage => {
                P::recorded("Apply and stage patch".to_string(), Full).refused_when_busy(Discard)
            }
            PatchApply::Commits => P::recorded("Apply patch as commits".to_string(), Full)
                .refused_when_busy(Discard)
                .gated(Refused),
        },

        // Commits end a merge or pick, so they can't run under a stored rebase or sequence.
        Command::Commit { message, .. } => {
            P::recorded(quoted("Commit ", message), RefsAndIndex).gated(Refused)
        }
        Command::AmendCommit { message, .. } => {
            P::recorded(amend_label(message), RefsAndIndex).gated(Refused)
        }

        // Branches and tags. Creating a branch checks it out.
        Command::CreateBranch { name, .. } => {
            P::recorded(fit(format!("Create branch {name}")), RefsAndIndex).gated(Refused)
        }
        Command::SwitchBranch { name, .. } => {
            P::recorded(fit(format!("Switch to {name}")), Full).gated(Refused)
        }
        // One step (#653): Undo goes back to the old branch with the changes in the work tree.
        // The stash entry the switch left (when not re-applied) stays in the stash list: the
        // undo stack doesn't cover stashes. A re-apply that conflicts isn't recorded at all
        // (the index then has conflicts), like a stash pop.
        Command::StashAndSwitch { name, .. } => P::recorded(
            fit(format!("Switch to {name} with local changes")),
            Full,
        )
        .gated(Refused),
        Command::DeleteBranch { name, .. } => {
            P::recorded(fit(format!("Delete branch {name}")), RefsAndIndex)
                .gated(Gate::Branch(name.clone()))
        }
        Command::RenameBranch {
            old_name, new_name, ..
        } => P::recorded(
            fit(format!("Rename branch {old_name} to {new_name}")),
            RefsAndIndex,
        )
        .gated(Gate::Branch(old_name.clone())),
        Command::CreateTag { name, .. } => {
            P::recorded(fit(format!("Create tag {name}")), RefsAndIndex)
        }
        Command::DeleteTag { name, .. } => {
            P::recorded(fit(format!("Delete tag {name}")), RefsAndIndex)
        }
        Command::CreateWorktree { branch, .. } => P::locked().gated(Gate::Branch(branch.clone())),

        // Moving HEAD.
        Command::CheckoutCommit { commit_id, .. } => {
            P::recorded(format!("Check out {}", short(commit_id)), Full).gated(Refused)
        }
        Command::ResetToCommit {
            commit_id, mode, ..
        } => {
            let policy = P::recorded(reset_label(commit_id, *mode), Full).gated(Refused);
            match mode {
                ResetMode::Hard => policy.refused_when_busy(Discard),
                // A reset that keeps the work tree runs unrecorded instead.
                ResetMode::Soft | ResetMode::Mixed => policy,
            }
        }
        Command::RestoreReflogEntry {
            reference, new_id, ..
        } => P::recorded(
            fit(format!("Restore {reference} to {}", short(new_id))),
            Full,
        )
        .gated(Refused),
        Command::PullCurrentUpstream { .. } => P::recorded("Pull".to_string(), Full).gated(Refused),

        // Stashes. Save records nothing but takes the lock for its gate check.
        Command::SaveStash { .. } => P::locked().gated(StashSave),
        Command::ApplyStash { .. } | Command::PopStash { .. } => P::free().gated(Refused),

        // Operations that can pause.
        Command::StartMerge {
            branch_name,
            options,
            ..
        } => P::recorded(merge_label(branch_name, options.squash), Full).gated(Refused),
        Command::AbortMerge { .. } => P::aborting(),
        Command::StartRebase { onto, edit, .. } => {
            P::recorded(start_rebase_label(onto, edit.as_ref()), Full).gated(Refused)
        }
        Command::RebaseContinue { .. } => P::continuing(),
        Command::AbortRebase { .. } => P::aborting(),
        Command::CherryPick { commit_id, .. } => {
            P::recorded(format!("Cherry-pick {}", short(commit_id)), Full).gated(Refused)
        }
        Command::RevertCommit { commit_id, .. } => {
            P::recorded(format!("Revert {}", short(commit_id)), Full).gated(Refused)
        }
        Command::CherryPickMany { commit_ids, .. } => {
            P::recorded(count_label("Cherry-pick", commit_ids.len()), Full).gated(Refused)
        }
        Command::RevertMany { commit_ids, .. } => {
            P::recorded(count_label("Revert", commit_ids.len()), Full).gated(Refused)
        }
        Command::ContinueSequence { .. } => P::continuing(),
        Command::AbortSequence { .. } => P::aborting(),

        // The undo stack itself.
        Command::Undo { .. } | Command::Redo { .. } => {
            P::locked().refused_when_busy(BusyReply::Undo)
        }
    }
}

/// "Revert hunk in a.txt from 1234567", "Apply a.txt from 1234567".
fn commit_change_label(change: &super::CommitChange) -> String {
    let verb = if change.reverse { "Revert" } else { "Apply" };
    let prefix = if change.hunk.is_some() {
        format!("{verb} hunk in ")
    } else {
        format!("{verb} ")
    };
    with_path(
        &prefix,
        &change.path,
        &format!(" from {}", short(&change.commit_id)),
    )
}

/// A command's reply channel, whatever its value type, for the policy's generic actions.
pub(super) trait Reply {
    /// Swaps the reply for a proxy and keeps the original in `held` (see [`HeldReplies`]).
    fn hold(&mut self, held: &mut HeldReplies);
    /// Replies with an error.
    fn refuse(&self, error: ErrorDto);
}

impl<T: 'static> Reply for Sender<Result<T, ErrorDto>> {
    fn hold(&mut self, held: &mut HeldReplies) {
        held.hold(self);
    }

    fn refuse(&self, error: ErrorDto) {
        let _ = self.send(Err(error));
    }
}

/// `command`'s reply; `None` only for `Ping` and the test-only `PanicNow`, which can't fail.
pub(super) fn reply_of(command: &mut Command) -> Option<&mut dyn Reply> {
    macro_rules! replies {
        ($($variant:ident),* $(,)?) => {
            match command {
                $(Command::$variant { reply, .. } => Some(reply as &mut dyn Reply),)*
                Command::Ping { .. } => None,
                #[cfg(test)]
                Command::PanicNow => None,
            }
        };
    }
    replies!(
        GetStatus,
        GetChangeStamp,
        GetStatusWithStamp,
        ListUntrackedFolder,
        GetCommitGraph,
        LocateCommit,
        GetMergedCommits,
        GetWorkingDiff,
        GetCommitDiff,
        GetCommitFiles,
        GetCommitStats,
        GetCommitMessage,
        GetCommitSignature,
        GetCommitDetails,
        GetBlame,
        GetFileHistory,
        GetLineHistory,
        SearchCommits,
        GetCommitRenameDiff,
        GetRangeFiles,
        GetRangeDiff,
        GetWorkingTreeCompareFiles,
        GetWorkingTreeCompareDiff,
        GetBranchCompareRange,
        StageFile,
        UnstageFile,
        StagePaths,
        UnstagePaths,
        StageHunk,
        UnstageHunk,
        DiscardHunk,
        StageLines,
        UnstageLines,
        DiscardLines,
        DiscardFile,
        DiscardAllUnstaged,
        ApplyCommitChange,
        GetPatch,
        ApplyPatch,
        PreviewPatch,
        IgnorePath,
        IgnorePaths,
        CreateRootFile,
        Commit,
        ListBranches,
        CreateBranch,
        GetSwitchConflicts,
        StashAndSwitch,
        SetIdentity,
        GetPullDefaults,
        SwitchBranch,
        DeleteBranch,
        RenameBranch,
        ListWorktrees,
        CreateWorktree,
        RemoveWorktree,
        PruneWorktrees,
        ListSubmodules,
        InitSubmodule,
        UpdateSubmodule,
        ListReflogRefs,
        GetReflog,
        RestoreReflogEntry,
        ListRemotes,
        ListRemoteBranches,
        GetCurrentUpstream,
        GetRemoteUpstreams,
        GetRemoteAuthMode,
        SaveHttpsCredential,
        ForgetHttpsCredential,
        SetRemoteAuthMode,
        AddRemote,
        RenameRemote,
        UpdateRemoteUrls,
        RemoveRemote,
        SetCurrentUpstream,
        ClearCurrentUpstream,
        ListTags,
        CreateTag,
        DeleteTag,
        ListStashes,
        SaveStash,
        ApplyStash,
        PopStash,
        GetStashFiles,
        DropStash,
        StartMerge,
        GetMergeDefaults,
        GetConflictHunks,
        ResolveConflict,
        AbortMerge,
        GetMergeMessage,
        ResolveAddDeleteConflict,
        MarkResolved,
        ResolveWholeFile,
        PrepareDiffTool,
        PrepareMergeTool,
        GetGitTools,
        GetFileSides,
        ListRecentAuthors,
        CheckMergeTarget,
        ReadFileVersion,
        GetCommitTree,
        GetFileAtCommit,
        SaveFileVersion,
        RestoreFileVersion,
        CommitsSince,
        StartRebase,
        RebaseContinue,
        AbortRebase,
        FetchRemote,
        PullCurrentUpstream,
        GetHeadInfo,
        AmendCommit,
        CherryPick,
        RevertCommit,
        CherryPickMany,
        RevertMany,
        ContinueSequence,
        AbortSequence,
        ResetToCommit,
        CheckoutCommit,
        ForcePushCurrentBranch,
        PushCurrentBranch,
        PushTags,
        FetchAllRemotes,
        PushBranch,
        DeleteRemoteBranch,
        DeleteRemoteTag,
        DetectForgeRepository,
        SaveForgeToken,
        ForgetForgeToken,
        ListPullRequests,
        CreatePullRequest,
        GetUndoState,
        Undo,
        Redo,
    )
}
