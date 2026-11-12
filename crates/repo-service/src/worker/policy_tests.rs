//! #485: every worker command's policy, pinned in one table, and the generic actions that read it.
use std::sync::mpsc::{channel, TryRecvError};

use git_core::reset::ResetMode;

use super::gate::{gate_during_operation, GateScope};
use super::policy::{self, reply_of, BusyReply, CommandPolicy, Gate, Lane};
use super::undo::{hold_reply, reply_busy, HeldReplies, DISCARD_BUSY, UNDO_BUSY};
use super::Command;

const HUNK: git_core::diff::HunkOptions = git_core::diff::HunkOptions {
    context_lines: 3,
    algorithm: git_core::diff::DiffAlgorithm::Default,
};
const ONTO: &str = "707e824e1af1b266aaa5c4faf7517e586b174183";

/// `undo(<scope>, <label>) lock busy=<reply> hold gate[=…] abort read`, each part only when it
/// applies; `-` for a command with no policy at all.
fn summary(policy: &CommandPolicy) -> String {
    let mut parts = Vec::new();
    if let Some(plan) = &policy.undo {
        parts.push(format!("undo({:?}, {})", plan.scope, plan.label));
    }
    if policy.locks_state {
        parts.push("lock".to_string());
    }
    if let Some(busy) = policy.busy {
        parts.push(format!("busy={busy:?}"));
    }
    if policy.holds_reply {
        parts.push("hold".to_string());
    }
    match &policy.gate {
        Gate::Open => {}
        Gate::Refused => parts.push("gate".to_string()),
        Gate::Branch(name) => parts.push(format!("gate=branch {name}")),
        Gate::StashSave => parts.push("gate=stash-save".to_string()),
    }
    if policy.aborts {
        parts.push("abort".to_string());
    }
    if policy.lane == Lane::Read {
        parts.push("read".to_string());
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(" ")
    }
}

/// The variant's name and `Command::$variant { fields…, reply }` with a fresh reply channel.
macro_rules! cmd {
    ($variant:ident { $($field:ident: $value:expr),* $(,)? }) => {
        (stringify!($variant), Command::$variant { $($field: $value,)* reply: channel().0 })
    };
    ($variant:ident) => {
        (stringify!($variant), Command::$variant { reply: channel().0 })
    };
}

/// `vec![…]` of `(cmd!(…), expected)` rows, plus a match with one arm per row's variant and no
/// catch-all, so a `Command` variant without a row doesn't compile (#518). A variant with several
/// rows repeats its arm. `PanicNow` is a unit variant `cmd!` can't build; its arm is here and its
/// row is added by `table`.
macro_rules! rows {
    ($((cmd!($variant:ident $($fields:tt)?), $expected:expr $(,)?)),* $(,)?) => {{
        #[allow(dead_code, unreachable_patterns)]
        fn every_variant_has_a_row(command: &Command) {
            match command {
                Command::PanicNow => {}
                $(Command::$variant { .. } => {})*
            }
        }
        vec![$((cmd!($variant $($fields)?), $expected)),*]
    }};
}

/// One sample of every `Command` variant (two or three where the policy depends on a field),
/// with its policy. A new command gets a row here: `rows!` doesn't compile without one, nor does
/// `policy::of` without its policy.
pub(super) fn table() -> Vec<((&'static str, Command), &'static str)> {
    let s = |text: &str| text.to_string();
    let op = || "op-1".into();
    let diff = git_core::diff::DiffOptions::default;
    let graph = git_core::graph::GraphSelection::default;
    let mut rows = rows![
        (cmd!(Ping), "-"),
        (cmd!(GetStatus), "read"),
        (cmd!(GetChangeStamp), "read"),
        (cmd!(GetStatusWithStamp), "read"),
        (
            cmd!(GetCommitGraph {
                limit: 1,
                selection: graph(),
                cursor: None
            }),
            "read",
        ),
        (
            cmd!(LocateCommit {
                revision: s("HEAD"),
                selection: graph()
            }),
            "read",
        ),
        (
            cmd!(GetMergedCommits {
                merge_id: s(ONTO),
                limit: 1
            }),
            "read",
        ),
        (
            cmd!(GetWorkingDiff {
                path: s("a.txt"),
                staged: false,
                opts: diff(),
                old_path: None
            }),
            "read",
        ),
        (
            cmd!(GetCommitDiff {
                commit_id: s(ONTO),
                path: s("a.txt"),
                opts: diff()
            }),
            "read",
        ),
        (cmd!(GetCommitFiles { commit_id: s(ONTO) }), "read"),
        (
            cmd!(GetCommitStats {
                commit_ids: vec![s(ONTO)],
                with_files: false
            }),
            "read",
        ),
        (cmd!(GetCommitMessage { commit_id: s(ONTO) }), "read"),
        (cmd!(GetCommitSignature { commit_id: s(ONTO) }), "read"),
        (cmd!(GetCommitDetails { commit_id: s(ONTO) }), "read"),
        (
            cmd!(GetBlame {
                commit_id: s(ONTO),
                path: s("a.txt")
            }),
            "read",
        ),
        (
            cmd!(GetFileHistory {
                path: s("a.txt"),
                limit: 1
            }),
            "read",
        ),
        (
            cmd!(GetLineHistory {
                revision: s("HEAD"),
                path: s("a.txt"),
                start_line: 1,
                end_line: 1,
                limit: 1
            }),
            "read",
        ),
        (
            cmd!(SearchCommits {
                query: s("fix"),
                selection: graph(),
                cursor: None,
                generation: 1
            }),
            "read",
        ),
        (
            cmd!(GetCommitRenameDiff {
                commit_id: s(ONTO),
                old_path: s("a.txt"),
                new_path: s("b.txt"),
                opts: diff()
            }),
            "read",
        ),
        (
            cmd!(GetRangeFiles {
                from_id: s(ONTO),
                to_id: s(ONTO)
            }),
            "read",
        ),
        (
            cmd!(GetRangeDiff {
                from_id: s(ONTO),
                to_id: s(ONTO),
                path: s("a.txt"),
                old_path: None,
                opts: diff()
            }),
            "read",
        ),
        (
            cmd!(GetWorkingTreeCompareFiles { commit_id: s(ONTO) }),
            "read"
        ),
        (
            cmd!(GetWorkingTreeCompareDiff {
                commit_id: s(ONTO),
                path: s("a.txt"),
                old_path: None,
                opts: diff()
            }),
            "read",
        ),
        (
            cmd!(GetBranchCompareRange {
                ref_name: s("topic")
            }),
            "read",
        ),
        (
            cmd!(StageFile { path: s("a.txt") }),
            "undo(RefsAndIndex, Stage a.txt) lock hold",
        ),
        (
            cmd!(UnstageFile { path: s("a.txt") }),
            "undo(RefsAndIndex, Unstage a.txt) lock hold",
        ),
        (
            cmd!(StagePaths {
                paths: vec![s("a.txt"), s("b.txt"), s("c.txt")]
            }),
            "undo(RefsAndIndex, Stage 3 files) lock hold",
        ),
        (
            cmd!(UnstagePaths {
                paths: vec![s("a.txt")]
            }),
            "undo(RefsAndIndex, Unstage a.txt) lock hold",
        ),
        (
            cmd!(StageHunk {
                path: s("a.txt"),
                old_start: 1,
                new_start: 1,
                hunk: HUNK
            }),
            "undo(RefsAndIndex, Stage hunk in a.txt) lock hold",
        ),
        (
            cmd!(UnstageHunk {
                path: s("a.txt"),
                old_start: 1,
                new_start: 1,
                hunk: HUNK
            }),
            "undo(RefsAndIndex, Unstage hunk in a.txt) lock hold",
        ),
        (
            cmd!(DiscardHunk {
                path: s("a.txt"),
                old_start: 1,
                new_start: 1,
                hunk: HUNK
            }),
            "undo(Full, Discard hunk in a.txt) lock busy=Discard hold",
        ),
        (
            cmd!(StageLines {
                path: s("a.txt"),
                old_start: 1,
                new_start: 1,
                hunk: HUNK,
                line_indices: vec![],
                expected_lines: vec![]
            }),
            "undo(RefsAndIndex, Stage lines in a.txt) lock hold",
        ),
        (
            cmd!(UnstageLines {
                path: s("a.txt"),
                old_start: 1,
                new_start: 1,
                hunk: HUNK,
                line_indices: vec![],
                expected_lines: vec![]
            }),
            "undo(RefsAndIndex, Unstage lines in a.txt) lock hold",
        ),
        (
            cmd!(DiscardLines {
                path: s("a.txt"),
                old_start: 1,
                new_start: 1,
                hunk: HUNK,
                line_indices: vec![],
                expected_lines: vec![]
            }),
            "undo(Full, Discard lines in a.txt) lock busy=Discard hold",
        ),
        (
            cmd!(DiscardFile { path: s("a.txt") }),
            "undo(Full, Discard changes to a.txt) lock busy=Discard hold",
        ),
        (
            cmd!(DiscardAllUnstaged { paths: vec![] }),
            "undo(Full, Discard all unstaged changes) lock busy=Discard hold",
        ),
        (
            cmd!(ApplyCommitChange {
                change: super::CommitChange {
                    commit_id: s(ONTO),
                    path: s("a.txt"),
                    old_path: None,
                    hunk: None,
                    reverse: true,
                    stage: false,
                }
            }),
            "undo(Full, Revert a.txt from 707e824) lock busy=Discard hold",
        ),
        (
            cmd!(ApplyCommitChange {
                change: super::CommitChange {
                    commit_id: s(ONTO),
                    path: s("a.txt"),
                    old_path: None,
                    hunk: Some(git_core::patch::CommitHunk {
                        old_start: 1,
                        new_start: 1,
                        options: HUNK,
                    }),
                    reverse: false,
                    stage: false,
                }
            }),
            "undo(Full, Apply hunk in a.txt from 707e824) lock busy=Discard hold",
        ),
        (
            cmd!(GetPatch {
                source: super::PatchSource::Staged
            }),
            "read",
        ),
        (
            cmd!(ApplyPatch {
                content: vec![],
                how: git_core::patch::PatchApply::WorkTree,
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Apply patch) lock busy=Discard hold",
        ),
        (
            cmd!(ApplyPatch {
                content: vec![],
                how: git_core::patch::PatchApply::Stage,
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Apply and stage patch) lock busy=Discard hold",
        ),
        (
            cmd!(ApplyPatch {
                content: vec![],
                how: git_core::patch::PatchApply::Commits,
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Apply patch as commits) lock busy=Discard hold gate",
        ),
        (cmd!(PreviewPatch { content: vec![] }), "read"),
        (
            cmd!(IgnorePath {
                pattern: s("*.log")
            }),
            "undo(Full, Ignore *.log) lock hold",
        ),
        (
            cmd!(IgnorePaths {
                patterns: vec![s("*.log"), s("/out/")]
            }),
            "undo(Full, Ignore 2 patterns) lock hold",
        ),
        (
            cmd!(CreateRootFile {
                name: s(".gitignore")
            }),
            "-",
        ),
        (
            cmd!(Commit {
                message: s("Fix it\n\nBody"),
                no_verify: false,
                trailers: vec![],
                operation_id: op(),
                events: channel().0
            }),
            "undo(RefsAndIndex, Commit “Fix it”) lock hold gate",
        ),
        (cmd!(ListBranches), "read"),
        (
            cmd!(CreateBranch {
                name: s("topic"),
                start_point: s("HEAD"),
                checkout: true
            }),
            "undo(RefsAndIndex, Create branch topic) lock hold gate",
        ),
        (
            cmd!(CreateBranch {
                name: s("topic"),
                start_point: s("HEAD"),
                checkout: false
            }),
            "undo(RefsAndIndex, Create branch topic) lock hold gate",
        ),
        (cmd!(GetSwitchConflicts { name: s("topic") }), "read"),
        (
            cmd!(StashAndSwitch {
                name: s("topic"),
                reapply: true,
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Switch to topic with local changes) lock hold gate",
        ),
        (
            cmd!(SetIdentity {
                name: s("Ada"),
                email: s("ada@example.com"),
                global: false
            }),
            "-",
        ),
        (cmd!(GetPullDefaults), "read"),
        (
            cmd!(SwitchBranch {
                name: s("topic"),
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Switch to topic) lock hold gate",
        ),
        (
            cmd!(DeleteBranch {
                name: s("topic"),
                force: false
            }),
            "undo(RefsAndIndex, Delete branch topic) lock hold gate=branch topic",
        ),
        (
            cmd!(RenameBranch {
                old_name: s("topic"),
                new_name: s("renamed")
            }),
            "undo(RefsAndIndex, Rename branch topic to renamed) lock hold gate=branch topic",
        ),
        (cmd!(ListWorktrees), "read"),
        (
            cmd!(CreateWorktree {
                name: s("wt"),
                path: "wt".into(),
                branch: s("topic"),
                start_point: None
            }),
            "lock gate=branch topic",
        ),
        (cmd!(RemoveWorktree { name: s("wt") }), "-"),
        (cmd!(PruneWorktrees), "-"),
        (cmd!(ListSubmodules), "read"),
        (cmd!(InitSubmodule { path: s("sub") }), "-"),
        (
            cmd!(UpdateSubmodule {
                path: s("sub"),
                recursive: false
            }),
            "-",
        ),
        (cmd!(ListReflogRefs), "read"),
        (
            cmd!(GetReflog {
                reference: s("HEAD")
            }),
            "read",
        ),
        (
            cmd!(RestoreReflogEntry {
                reference: s("refs/heads/main"),
                new_id: s(ONTO)
            }),
            "undo(Full, Restore refs/heads/main to 707e824) lock hold gate",
        ),
        (cmd!(ListRemotes), "read"),
        (
            cmd!(ListRemoteBranches {
                remote_name: s("origin")
            }),
            "read",
        ),
        (cmd!(GetCurrentUpstream), "read"),
        (cmd!(GetRemoteUpstreams { name: s("origin") }), "read"),
        (cmd!(GetRemoteAuthMode { name: s("origin") }), "read"),
        (
            cmd!(SaveHttpsCredential {
                remote_name: s("origin"),
                username: s("me"),
                token: s("token")
            }),
            "-",
        ),
        (
            cmd!(ForgetHttpsCredential {
                remote_name: s("origin")
            }),
            "-",
        ),
        (
            cmd!(SetRemoteAuthMode {
                remote_name: s("origin"),
                mode: git_core::remote::RemoteAuthMode::SshAgent
            }),
            "-",
        ),
        (
            cmd!(AddRemote {
                name: s("origin"),
                fetch_url: s("u"),
                push_url: None
            }),
            "-",
        ),
        (
            cmd!(RenameRemote {
                old_name: s("origin"),
                new_name: s("up")
            }),
            "-",
        ),
        (
            cmd!(UpdateRemoteUrls {
                name: s("origin"),
                fetch_url: s("u"),
                push_url: None
            }),
            "-",
        ),
        (
            cmd!(RemoveRemote {
                name: s("origin"),
                clear_upstreams: true
            }),
            "-",
        ),
        (
            cmd!(SetCurrentUpstream {
                remote_name: s("origin"),
                remote_branch: s("main")
            }),
            "-",
        ),
        (cmd!(ClearCurrentUpstream), "-"),
        (cmd!(ListTags), "read"),
        (
            cmd!(CreateTag {
                name: s("v1"),
                message: None,
                target: None
            }),
            "undo(RefsAndIndex, Create tag v1) lock hold",
        ),
        (
            cmd!(DeleteTag { name: s("v1") }),
            "undo(RefsAndIndex, Delete tag v1) lock hold",
        ),
        (cmd!(ListStashes), "read"),
        (
            cmd!(SaveStash {
                options: git_core::stash::StashOptions::default()
            }),
            "lock gate=stash-save",
        ),
        (
            cmd!(ApplyStash {
                index: 0,
                commit_id: s(ONTO),
                restore_index: false
            }),
            "gate",
        ),
        (
            cmd!(PopStash {
                index: 0,
                commit_id: s(ONTO),
                restore_index: false
            }),
            "gate",
        ),
        (cmd!(GetStashFiles { commit_id: s(ONTO) }), "read"),
        (
            cmd!(DropStash {
                index: 0,
                commit_id: s(ONTO)
            }),
            "-",
        ),
        (
            cmd!(StartMerge {
                branch_name: s("refs/heads/topic"),
                options: git_core::merge::MergeOptions::default(),
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Merge topic) lock hold gate",
        ),
        (
            cmd!(StartMerge {
                branch_name: s("refs/remotes/origin/topic"),
                options: git_core::merge::MergeOptions {
                    squash: true,
                    ..Default::default()
                },
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Squash merge origin/topic) lock hold gate",
        ),
        (cmd!(GetMergeDefaults), "read"),
        (cmd!(GetConflictHunks { path: s("a.txt") }), "read"),
        (
            cmd!(ResolveConflict {
                path: s("a.txt"),
                resolved_content: s("a\n")
            }),
            "-",
        ),
        (cmd!(AbortMerge), "lock hold abort"),
        (cmd!(GetMergeMessage), "read"),
        (
            cmd!(ResolveAddDeleteConflict {
                path: s("a.txt"),
                choice: git_core::merge::FileConflictChoice::Ours
            }),
            "-",
        ),
        (
            cmd!(MarkResolved {
                path: s("a.txt"),
                force: false
            }),
            "-",
        ),
        (
            cmd!(ResolveWholeFile {
                path: s("a.txt"),
                side: git_core::merge::ConflictSide::Ours
            }),
            "-",
        ),
        (
            cmd!(PrepareDiffTool {
                path: s("a.txt"),
                source: git_core::tools::DiffSource::Unstaged
            }),
            "read",
        ),
        (cmd!(PrepareMergeTool { path: s("a.txt") }), "-"),
        (cmd!(GetGitTools), "read"),
        (
            cmd!(GetFileSides {
                path: s("a.txt"),
                source: git_core::tools::DiffSource::Unstaged
            }),
            "read",
        ),
        (cmd!(ListRecentAuthors), "read"),
        (cmd!(CheckMergeTarget { path: s("a.txt") }), "read"),
        (
            cmd!(ReadFileVersion {
                path: s("a.txt"),
                version: git_core::tools::FileVersion::Head
            }),
            "read",
        ),
        (
            cmd!(GetCommitTree {
                commit_id: s(ONTO),
                dir: s(""),
                filter: None
            }),
            "read",
        ),
        (
            cmd!(GetFileAtCommit {
                commit_id: s(ONTO),
                path: s("a.txt")
            }),
            "read",
        ),
        (
            cmd!(SaveFileVersion {
                commit_id: s(ONTO),
                path: s("a.txt"),
                destination: std::path::PathBuf::from("/elsewhere/a.txt")
            }),
            "read",
        ),
        (
            cmd!(RestoreFileVersion {
                commit_id: s(ONTO),
                path: s("a.txt")
            }),
            "undo(Full, Restore a.txt from 707e824) lock busy=Discard hold",
        ),
        (cmd!(CommitsSince { onto: s(ONTO) }), "read"),
        (
            cmd!(StartRebase {
                onto: s(ONTO),
                plan: vec![],
                edit: None,
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Rebase onto 707e824) lock hold gate",
        ),
        (
            cmd!(StartRebase {
                onto: s(ONTO),
                plan: vec![],
                edit: Some(super::HistoryEdit {
                    kind: super::HistoryEditKind::Squash,
                    commit_id: s("1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d"),
                }),
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Squash 1a2b3c4 into its parent) lock hold gate",
        ),
        (
            cmd!(RebaseContinue {
                operation_id: op(),
                events: channel().0
            }),
            "lock hold",
        ),
        (cmd!(AbortRebase), "lock hold abort"),
        (
            cmd!(FetchRemote {
                remote_name: s("origin"),
                prune: None,
                operation_id: op(),
                events: channel().0
            }),
            "-",
        ),
        (
            cmd!(PullCurrentUpstream {
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Pull) lock hold gate",
        ),
        (cmd!(GetHeadInfo), "read"),
        (
            cmd!(AmendCommit {
                message: s(" "),
                no_verify: false,
                author: None,
                trailers: vec![],
                operation_id: op(),
                events: channel().0
            }),
            "undo(RefsAndIndex, Amend last commit) lock hold gate",
        ),
        (
            cmd!(CherryPick {
                commit_id: s(ONTO),
                mainline: None,
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Cherry-pick 707e824) lock hold gate",
        ),
        (
            cmd!(RevertCommit {
                commit_id: s(ONTO),
                mainline: None,
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Revert 707e824) lock hold gate",
        ),
        (
            cmd!(CherryPickMany {
                commit_ids: vec![s(ONTO), s(ONTO)],
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Cherry-pick 2 commits) lock hold gate",
        ),
        (
            cmd!(RevertMany {
                commit_ids: vec![s(ONTO)],
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Revert 1 commit) lock hold gate",
        ),
        (
            cmd!(ContinueSequence {
                no_verify: false,
                operation_id: op(),
                events: channel().0
            }),
            "lock hold",
        ),
        (cmd!(AbortSequence { confirmed: None }), "lock hold abort"),
        (
            cmd!(ResetToCommit {
                commit_id: s(ONTO),
                mode: ResetMode::Hard
            }),
            "undo(Full, Reset to 707e824 (hard)) lock busy=Discard hold gate",
        ),
        (
            cmd!(ResetToCommit {
                commit_id: s(ONTO),
                mode: ResetMode::Mixed
            }),
            "undo(Full, Reset to 707e824 (mixed)) lock hold gate",
        ),
        (
            cmd!(ResetToCommit {
                commit_id: s(ONTO),
                mode: ResetMode::Soft
            }),
            "undo(Full, Reset to 707e824 (soft)) lock hold gate",
        ),
        (
            cmd!(CheckoutCommit {
                commit_id: s(ONTO),
                operation_id: op(),
                events: channel().0
            }),
            "undo(Full, Check out 707e824) lock hold gate",
        ),
        (
            cmd!(ForcePushCurrentBranch {
                remote_name: s("origin"),
                no_verify: false,
                operation_id: op(),
                events: channel().0
            }),
            "-",
        ),
        (
            cmd!(PushCurrentBranch {
                remote_name: s("origin"),
                no_verify: false,
                operation_id: op(),
                events: channel().0
            }),
            "-",
        ),
        (
            cmd!(PushTags {
                remote_name: s("origin"),
                names: vec![],
                no_verify: false,
                operation_id: op(),
                events: channel().0
            }),
            "-",
        ),
        (
            cmd!(FetchAllRemotes {
                prune: None,
                background: None,
                operation_id: op(),
                events: channel().0
            }),
            "-",
        ),
        (
            cmd!(PushBranch {
                remote_name: s("origin"),
                local_branch: s("topic"),
                remote_branch: s("topic"),
                set_upstream: false,
                no_verify: false,
                operation_id: op(),
                events: channel().0
            }),
            "-",
        ),
        (
            cmd!(DeleteRemoteBranch {
                remote_name: s("origin"),
                branch_name: s("topic"),
                no_verify: false,
                operation_id: op(),
                events: channel().0
            }),
            "-",
        ),
        (
            cmd!(DeleteRemoteTag {
                remote_name: s("origin"),
                tag_name: s("v1"),
                no_verify: false,
                operation_id: op(),
                events: channel().0
            }),
            "-",
        ),
        (cmd!(DetectForgeRepository), "read"),
        (
            cmd!(SaveForgeToken {
                provider: git_core::forge::ForgeProvider::GitHub,
                account: s("me"),
                token: s("token")
            }),
            "-",
        ),
        (
            cmd!(ForgetForgeToken {
                provider: git_core::forge::ForgeProvider::GitHub,
                account: s("me")
            }),
            "-",
        ),
        (
            cmd!(ListPullRequests {
                remote_name: s("origin"),
                account: s("me")
            }),
            "-",
        ),
        (
            cmd!(CreatePullRequest {
                remote_name: s("origin"),
                account: s("me"),
                create: crate::pull_requests::CreatePullRequest {
                    title: s("t"),
                    description: None,
                    source_branch: s("topic"),
                    target_branch: s("main"),
                }
            }),
            "-",
        ),
        (cmd!(GetUndoState), "read"),
        (cmd!(Undo), "lock busy=Undo"),
        (cmd!(Redo), "lock busy=Undo"),
    ];
    rows.push((("PanicNow", Command::PanicNow), "-"));
    rows
}

/// The whole policy, one row per command: what it records (and under which label), whether it
/// takes the state lock, is refused while that lock stays busy, has its reply held, is refused
/// during a stored rebase or sequence, and aborts a paused operation.
#[test]
fn every_command_has_the_expected_policy() {
    let mismatches: Vec<String> = table()
        .iter()
        .filter_map(|((name, command), expected)| {
            let actual = summary(&policy::of(command));
            (actual != *expected).then(|| format!("{name}: expected `{expected}`, got `{actual}`"))
        })
        .collect();
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

/// The pairings the worker relies on: a recorded or aborting command holds its reply (so the
/// recorder's "after" snapshot comes first), whatever holds its reply or refuses while busy holds
/// the state lock, and every command that can be held or refused has a reply to do it with.
#[test]
fn policies_pair_up() {
    for ((name, mut command), _) in table() {
        let policy = policy::of(&command);
        if policy.undo.is_some() || policy.aborts {
            assert!(policy.holds_reply, "{name} doesn't hold its reply");
        }
        if policy.holds_reply || policy.busy.is_some() {
            assert!(policy.locks_state, "{name} doesn't take the state lock");
        }
        let can_fail = policy.holds_reply || policy.busy.is_some() || policy.gate != Gate::Open;
        if can_fail {
            assert!(reply_of(&mut command).is_some(), "{name} has no reply");
        }
        let replyless = matches!(command, Command::Ping { .. } | Command::PanicNow);
        assert_eq!(reply_of(&mut command).is_none(), replyless, "{name}");
    }
}

/// #734: a read-lane command runs while the worker waits for a transfer, outside `step`, so it
/// must need nothing `step` does: no lock, no undo entry, no held reply, no gate, no abort.
#[test]
fn the_read_lane_only_holds_free_commands() {
    for ((name, command), _) in table() {
        let policy = policy::of(&command);
        if policy.lane == Lane::Read {
            let free = CommandPolicy {
                lane: Lane::Read,
                ..policy::of(&Command::Ping { reply: channel().0 })
            };
            assert_eq!(policy, free, "{name}");
        }
    }
}

/// Assert that `rx` gets nothing until `held` is released, then the reply.
fn assert_held_until_release<T: std::fmt::Debug>(
    rx: std::sync::mpsc::Receiver<Result<T, crate::wire::ErrorDto>>,
    held: HeldReplies,
) {
    assert_eq!(rx.try_recv().map(|_| ()), Err(TryRecvError::Empty));
    held.release();
    assert!(rx.try_recv().is_ok(), "the reply was not forwarded");
}

#[test]
fn a_held_reply_is_forwarded_on_release() {
    // A `()` reply.
    let (reply, rx) = channel();
    let mut command = Command::StageFile {
        path: "a.txt".into(),
        reply,
    };
    let mut held = HeldReplies::default();
    hold_reply(&mut command, &mut held);
    let Command::StageFile { reply, .. } = &command else {
        unreachable!()
    };
    reply.send(Ok(())).unwrap();
    drop(command);
    assert_held_until_release(rx, held);

    // An abort, whose reply carries a value.
    let (reply, rx) = channel();
    let mut command = Command::AbortSequence {
        confirmed: None,
        reply,
    };
    let mut held = HeldReplies::default();
    hold_reply(&mut command, &mut held);
    if let Some(reply) = reply_of(&mut command) {
        reply.refuse("failed".into());
    }
    drop(command);
    assert_held_until_release(rx, held);
}

#[test]
fn a_busy_refusal_replies_with_its_text() {
    let check = |mut command: Command,
                 rx: std::sync::mpsc::Receiver<Result<(), crate::wire::ErrorDto>>,
                 text: &str| {
        let busy = policy::of(&command).busy.expect("refuses when busy");
        reply_busy(&mut command, busy);
        assert_eq!(
            rx.try_recv().unwrap(),
            Err(crate::wire::ErrorDto::from(text))
        );
    };
    let (reply, rx) = channel();
    check(
        Command::DiscardFile {
            path: "a.txt".into(),
            reply,
        },
        rx,
        DISCARD_BUSY,
    );
    let (reply, rx) = channel();
    check(
        Command::ResetToCommit {
            commit_id: ONTO.into(),
            mode: ResetMode::Hard,
            reply,
        },
        rx,
        DISCARD_BUSY,
    );
    let (reply, rx) = channel();
    let mut command = Command::Undo { reply };
    reply_busy(&mut command, BusyReply::Undo);
    assert_eq!(
        rx.try_recv().unwrap().map(|_| ()),
        Err(crate::wire::ErrorDto::from(UNDO_BUSY))
    );
}

/// What a stored operation with `scope` does to `command`: `Some(text)` when it refused it (and
/// replaced it with a no-op), `None` when it let it through unchanged.
fn gate(
    build: impl FnOnce(std::sync::mpsc::Sender<Result<(), crate::wire::ErrorDto>>) -> Command,
    scope: &GateScope,
) -> Option<crate::wire::ErrorDto> {
    let (reply, rx) = channel();
    let command = build(reply);
    let policy = policy::of(&command);
    let variant = std::mem::discriminant(&command);
    let gated = gate_during_operation(command, &policy.gate, scope);
    match rx.try_recv() {
        Ok(result) => {
            assert!(
                matches!(gated, Command::Ping { .. }),
                "a refused command still runs"
            );
            Some(result.unwrap_err())
        }
        Err(_) => {
            assert!(
                std::mem::discriminant(&gated) == variant,
                "a passed command is unchanged"
            );
            None
        }
    }
}

#[test]
fn a_stored_operation_refuses_by_gate_and_scope() {
    let in_progress = Some(crate::wire::ErrorDto::from_error(
        git_core::pick::PickError::OperationInProgress,
    ));
    let rebase = GateScope {
        branch: Some("topic".into()),
        refuse_stash_save: true,
    };
    let detached = GateScope {
        branch: None,
        refuse_stash_save: false,
    };
    let delete = |name: &'static str| {
        move |reply| Command::DeleteBranch {
            name: name.into(),
            force: false,
            reply,
        }
    };
    let rename = |old_name: &'static str| {
        move |reply| Command::RenameBranch {
            old_name: old_name.into(),
            new_name: "renamed".into(),
            reply,
        }
    };
    let worktree = |branch: &'static str| {
        move |reply| Command::CreateWorktree {
            name: "wt".into(),
            path: "wt".into(),
            branch: branch.into(),
            start_point: None,
            reply,
        }
    };
    let stash = |reply| Command::SaveStash {
        options: git_core::stash::StashOptions::default(),
        reply,
    };
    let reset = |reply| Command::ResetToCommit {
        commit_id: ONTO.into(),
        mode: ResetMode::Mixed,
        reply,
    };
    let stage = |reply| Command::StageFile {
        path: "a.txt".into(),
        reply,
    };

    // Always refused, always allowed.
    assert_eq!(gate(reset, &rebase), in_progress);
    assert_eq!(gate(reset, &detached), in_progress);
    assert_eq!(gate(stage, &rebase), None);
    // The protected branch only.
    assert_eq!(gate(delete("topic"), &rebase), in_progress);
    assert_eq!(gate(delete("other"), &rebase), None);
    assert_eq!(gate(delete("topic"), &detached), None);
    assert_eq!(gate(rename("topic"), &rebase), in_progress);
    assert_eq!(gate(rename("other"), &rebase), None);
    assert_eq!(gate(worktree("topic"), &rebase), in_progress);
    assert_eq!(gate(worktree("other"), &rebase), None);
    // Stash save only while the scope says so.
    assert_eq!(gate(stash, &rebase), in_progress);
    assert_eq!(gate(stash, &detached), None);
}
