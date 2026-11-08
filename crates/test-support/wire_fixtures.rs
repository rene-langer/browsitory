//! Domain values for the wire snapshot tests (#306). Not part of the `test-support` library,
//! which can't depend on `git-core`: both transports' snapshot tests include this file with
//! `#[path]` (`crates/tauri-app/src/commands/wire_snapshots.rs` and
//! `crates/vscode-sidecar/src/dispatch/wire_snapshots.rs`), so the two build their DTOs from the
//! same values and the cross-transport test can compare their bytes.
//!
//! Each function returns `(case, value)` pairs. Together they cover every variant of every enum
//! that reaches the wire and both `None` and `Some` of every optional field. An exhaustive
//! `match` next to each enum list stops compiling when a variant is added, so a new variant
//! can't go unsnapshotted.

use git_core::diff::DiffLineOrigin;
use git_core::merge::FastForward;
use git_core::pick::{SequenceKind, SequenceProgress};
use git_core::rebase::{RebaseDivergence, RebaseHealth};
use git_core::remote::{TransferErrorKind, TransferOperation, TransferPhase};
use git_core::repo::{OperationKind, TerminalRebaseKind};
use git_core::status::StatusKind;
use git_core::tools::ConfigSource;
use repo_service::worker::{HookEvent, HookSource, TransferEvent};

pub type Cases<T> = Vec<(String, T)>;

fn case<T>(name: impl Into<String>, value: T) -> (String, T) {
    (name.into(), value)
}

/// `Some(text)` as an owned string.
fn some(text: &str) -> Option<String> {
    Some(text.to_string())
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// Every variant of a fieldless enum, plus a `match` that fails to compile when one is added.
macro_rules! every {
    ($ty:ident { $($variant:ident),* $(,)? }) => {{
        #[allow(dead_code)]
        fn exhaustive(value: $ty) {
            match value {
                $($ty::$variant => {})*
            }
        }
        vec![$($ty::$variant),*]
    }};
}

/// Fails unless `values` (references) hold a value matching each pattern. Its `match` stops
/// compiling when the enum gains a variant the patterns miss, so a new data-carrying variant
/// needs a pattern here and then a case, or the snapshot test fails.
macro_rules! assert_every_variant {
    ($values:expr, $($pat:pat),+ $(,)?) => {{
        let names = [$(stringify!($pat)),+];
        let mut hit = vec![false; names.len()];
        for value in $values {
            match value {
                $($pat => {})+
            }
            // A pattern can be `None`, which clippy would rather see as `is_none()`.
            #[allow(clippy::redundant_pattern_matching)]
            let matched = [$(matches!(value, $pat)),+];
            for (slot, matched) in hit.iter_mut().zip(matched) {
                *slot |= matched;
            }
        }
        for (name, hit) in names.iter().zip(&hit) {
            assert!(*hit, "no wire snapshot case for {name}");
        }
    }};
}

pub fn status_kinds() -> Vec<StatusKind> {
    every!(StatusKind {
        New,
        Modified,
        Deleted,
        Renamed,
        TypeChange,
        Conflicted
    })
}

pub fn diff_line_origins() -> Vec<DiffLineOrigin> {
    every!(DiffLineOrigin {
        Add,
        Remove,
        Context
    })
}

pub fn transfer_operations() -> Vec<TransferOperation> {
    every!(TransferOperation {
        Fetch,
        FetchAll,
        Pull,
        PushBranch,
        ForcePushBranch,
        PushTags,
        DeleteRemoteBranch,
        DeleteRemoteTag,
        Clone,
        BackgroundFetch
    })
}

pub fn transfer_phases() -> Vec<TransferPhase> {
    every!(TransferPhase {
        Receiving,
        Updating,
        RunningHook
    })
}

pub fn transfer_error_kinds() -> Vec<TransferErrorKind> {
    every!(TransferErrorKind {
        NonFastForward,
        RejectedRemoteRef,
        StaleLease,
        MissingCredential,
        CredentialStoreFailure,
        SshAgentFailure,
        TransferFailed,
        Cancelled,
        HookFailed
    })
}

pub fn hook_sources() -> Vec<HookSource> {
    every!(HookSource {
        Commit,
        Checkout,
        Merge,
        Pick
    })
}

pub fn operation_kinds() -> Vec<OperationKind> {
    every!(OperationKind {
        Merge,
        CherryPick,
        Revert
    })
}

pub fn sequence_kinds() -> Vec<SequenceKind> {
    every!(SequenceKind { CherryPick, Revert })
}

pub fn terminal_rebase_kinds() -> Vec<TerminalRebaseKind> {
    every!(TerminalRebaseKind { Rebase, Am })
}

pub fn rebase_divergences() -> Vec<RebaseDivergence> {
    every!(RebaseDivergence {
        HeadMoved,
        BranchMoved,
        BranchMissing,
        OtherOperation
    })
}

pub fn config_sources() -> Vec<ConfigSource> {
    every!(ConfigSource {
        System,
        Xdg,
        Global,
        Local,
        Worktree,
        Other
    })
}

pub fn status_entries() -> Cases<git_core::status::StatusEntry> {
    let mut cases: Cases<_> = status_kinds()
        .into_iter()
        .map(|kind| {
            case(
                format!("{kind:?}"),
                git_core::status::StatusEntry {
                    path: "src/lib.rs".to_string(),
                    old_path: None,
                    staged: false,
                    kind,
                },
            )
        })
        .collect();
    cases.push(case(
        "staged-rename",
        git_core::status::StatusEntry {
            path: "src/new.rs".to_string(),
            old_path: some("src/old.rs"),
            staged: true,
            kind: StatusKind::Renamed,
        },
    ));
    cases
}

fn graph_commit(summary: &str) -> git_core::graph::GraphCommit {
    git_core::graph::GraphCommit {
        id: "0123456789abcdef0123456789abcdef01234567".to_string(),
        short_id: "0123456".to_string(),
        summary: summary.to_string(),
        author_name: "Ada Lovelace".to_string(),
        author_email: "ada@example.com".to_string(),
        timestamp: 1_700_000_000,
        parent_ids: strings(&["89abcdef0123456789abcdef0123456789abcdef"]),
        branch_refs: strings(&["main"]),
        remote_branch_refs: strings(&["origin/main"]),
        tags: strings(&["v1.0.0"]),
    }
}

pub fn graph_commits() -> Cases<git_core::graph::GraphCommit> {
    vec![
        case("plain", graph_commit("Add the graph")),
        // Escaping: quotes, backslash, a control character, non-ASCII and a non-BMP character.
        case(
            "escapes",
            graph_commit("Say \"hi\" \\ tab\there, caf\u{e9} \u{1f600}\u{1}"),
        ),
        case(
            "root-commit",
            git_core::graph::GraphCommit {
                parent_ids: Vec::new(),
                branch_refs: Vec::new(),
                remote_branch_refs: Vec::new(),
                tags: Vec::new(),
                timestamp: -1,
                ..graph_commit("Initial commit")
            },
        ),
    ]
}

pub fn diff_hunks() -> Cases<git_core::diff::DiffHunk> {
    vec![
        case(
            "every-origin",
            git_core::diff::DiffHunk {
                old_start: 1,
                old_lines: 2,
                new_start: 1,
                new_lines: 2,
                lines: diff_line_origins()
                    .into_iter()
                    .map(|origin| git_core::diff::DiffLine {
                        origin,
                        content: format!("{origin:?} line\n"),
                    })
                    .collect(),
            },
        ),
        case(
            "empty",
            git_core::diff::DiffHunk {
                old_start: 0,
                old_lines: 0,
                new_start: 0,
                new_lines: 0,
                lines: Vec::new(),
            },
        ),
    ]
}

pub fn file_history_entries() -> Cases<git_core::history::FileHistoryEntry> {
    vec![
        case(
            "no-old-path",
            git_core::history::FileHistoryEntry {
                commit: graph_commit("Edit"),
                path: "a.txt".to_string(),
                old_path: None,
            },
        ),
        case(
            "renamed",
            git_core::history::FileHistoryEntry {
                commit: graph_commit("Rename"),
                path: "b.txt".to_string(),
                old_path: some("a.txt"),
            },
        ),
    ]
}

pub fn search_pages() -> Cases<git_core::search::SearchPage> {
    vec![
        case(
            "more",
            git_core::search::SearchPage {
                commits: vec![graph_commit("Match")],
                next_cursor: some("cursor-1"),
                scanned: 500,
            },
        ),
        case(
            "exhausted",
            git_core::search::SearchPage {
                commits: Vec::new(),
                next_cursor: None,
                scanned: 0,
            },
        ),
    ]
}

pub fn changed_files() -> Cases<git_core::diff::ChangedFile> {
    vec![
        case(
            "plain",
            git_core::diff::ChangedFile {
                path: "a.txt".to_string(),
                old_path: None,
            },
        ),
        case(
            "renamed",
            git_core::diff::ChangedFile {
                path: "b.txt".to_string(),
                old_path: some("a.txt"),
            },
        ),
    ]
}

pub fn compare_ranges() -> Cases<git_core::diff::CompareRange> {
    vec![case(
        "plain",
        git_core::diff::CompareRange {
            from_id: "1111111111111111111111111111111111111111".to_string(),
            to_id: "2222222222222222222222222222222222222222".to_string(),
        },
    )]
}

pub fn open_repo_entries() -> Cases<config::OpenRepoEntry> {
    vec![
        case(
            "loose",
            config::OpenRepoEntry {
                path: "/repos/app".into(),
                workspace_id: None,
            },
        ),
        case(
            "in-workspace",
            config::OpenRepoEntry {
                path: "/repos/lib".into(),
                workspace_id: some("ws-1"),
            },
        ),
    ]
}

pub fn workspaces() -> Cases<config::Workspace> {
    vec![
        case(
            "members",
            config::Workspace {
                id: "ws-1".to_string(),
                name: "Work".to_string(),
                root_path: "/repos".into(),
                member_paths: vec!["/repos/app".into(), "/repos/lib".into()],
            },
        ),
        case(
            "empty",
            config::Workspace {
                id: "ws-2".to_string(),
                name: "Empty".to_string(),
                root_path: "/empty".into(),
                member_paths: Vec::new(),
            },
        ),
    ]
}

pub fn diff_settings() -> Cases<config::DiffSettings> {
    let mut cases = vec![case(
        "plain",
        config::DiffSettings {
            ignore_whitespace: true,
            context_lines: 5,
            syntax_highlight: false,
            word_wrap: config::WordWrap::Auto,
        },
    )];
    use config::WordWrap;
    // `plain` covers `Auto`.
    for word_wrap in every!(WordWrap { Auto, On, Off }) {
        if word_wrap == WordWrap::Auto {
            continue;
        }
        cases.push(case(
            format!("wrap-{word_wrap:?}"),
            config::DiffSettings {
                word_wrap,
                ..config::DiffSettings::default()
            },
        ));
    }
    cases
}

pub fn external_tool_settings() -> Cases<config::ExternalToolSettings> {
    vec![
        case("unset", config::ExternalToolSettings::default()),
        case(
            "set",
            config::ExternalToolSettings {
                editor: some("code -w"),
                terminal: some("wt"),
            },
        ),
    ]
}

pub fn commit_message_settings() -> Cases<config::CommitMessageSettings> {
    vec![case(
        "plain",
        config::CommitMessageSettings {
            summary_limit: 50,
            body_ruler: 72,
            show_ruler: true,
        },
    )]
}

pub fn fetch_settings() -> Cases<config::FetchSettings> {
    vec![
        case(
            "off",
            config::FetchSettings {
                auto_fetch_minutes: 0,
            },
        ),
        case(
            "every-10",
            config::FetchSettings {
                auto_fetch_minutes: 10,
            },
        ),
    ]
}

fn git_tool(source: ConfigSource) -> git_core::tools::GitToolInfo {
    git_core::tools::GitToolInfo {
        name: "meld".to_string(),
        command: some("meld \"$LOCAL\" \"$REMOTE\""),
        source,
        via_merge_tool: true,
        problem: some("not found on PATH"),
    }
}

pub fn git_tools() -> Cases<git_core::tools::GitTools> {
    let mut cases: Cases<_> = config_sources()
        .into_iter()
        .map(|source| {
            case(
                format!("diff-tool-{source:?}"),
                git_core::tools::GitTools {
                    diff_tool: Some(git_tool(source)),
                    merge_tool: None,
                },
            )
        })
        .collect();
    cases.push(case(
        "merge-tool-minimal",
        git_core::tools::GitTools {
            diff_tool: None,
            merge_tool: Some(git_core::tools::GitToolInfo {
                name: "vimdiff".to_string(),
                command: None,
                source: ConfigSource::Global,
                via_merge_tool: false,
                problem: None,
            }),
        },
    ));
    cases.push(case(
        "none",
        git_core::tools::GitTools {
            diff_tool: None,
            merge_tool: None,
        },
    ));
    cases
}

pub fn mark_resolved_outcomes() -> Cases<git_core::merge::MarkResolvedOutcome> {
    use git_core::merge::MarkResolvedOutcome;
    every!(MarkResolvedOutcome {
        Resolved,
        MarkersRemain
    })
    .into_iter()
    .map(|outcome| case(format!("{outcome:?}"), outcome))
    .collect()
}

pub fn file_sides() -> Cases<git_core::diff::FileSides> {
    use git_core::diff::FileSide;
    let side_cases = || {
        let cases = vec![
            ("Missing", FileSide::Missing),
            ("TooLarge", FileSide::TooLarge { size: 99_000_000 }),
            (
                "Loaded-png",
                FileSide::Loaded {
                    bytes: b"\x89PNG\r\n\x1a\n\0\0".to_vec(),
                },
            ),
            (
                "Loaded-not-an-image",
                FileSide::Loaded {
                    bytes: b"hello".to_vec(),
                },
            ),
        ];
        assert_every_variant!(
            cases.iter().map(|(_, side)| side),
            FileSide::Missing,
            FileSide::TooLarge { .. },
            FileSide::Loaded { .. },
        );
        cases
    };
    side_cases()
        .into_iter()
        .map(|(name, side)| {
            case(
                format!("old-{name}"),
                git_core::diff::FileSides {
                    old: side,
                    new: FileSide::Missing,
                },
            )
        })
        .chain(side_cases().into_iter().map(|(name, side)| {
            case(
                format!("new-{name}"),
                git_core::diff::FileSides {
                    old: FileSide::Missing,
                    new: side,
                },
            )
        }))
        .collect()
}

pub fn commit_outcomes() -> Cases<git_core::commit::CommitOutcome> {
    use git_core::commit::CommitOutcome;
    let cases = vec![
        case(
            "Committed",
            CommitOutcome::Committed {
                commit_id: "3333333333333333333333333333333333333333".to_string(),
            },
        ),
        case(
            "HookFailed",
            CommitOutcome::HookFailed {
                hook: "pre-commit".to_string(),
                output: "lint failed\n".to_string(),
            },
        ),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        CommitOutcome::Committed { .. },
        CommitOutcome::HookFailed { .. },
    );
    cases
}

/// Each source with `Started`, and one `Finished`.
pub fn hook_events() -> Cases<HookEvent> {
    let mut cases: Cases<_> = hook_sources()
        .into_iter()
        .map(|source| {
            case(
                format!("Started-{source:?}"),
                HookEvent::Started {
                    operation_id: "op-1".to_string(),
                    hook: "pre-commit".to_string(),
                    source,
                },
            )
        })
        .collect();
    cases.push(case(
        "Finished-Commit",
        HookEvent::Finished {
            operation_id: "op-1".to_string(),
            hook: "pre-commit".to_string(),
            source: HookSource::Commit,
        },
    ));
    assert_every_variant!(
        cases.iter().map(|(_, event)| event),
        HookEvent::Started { .. },
        HookEvent::Finished { .. },
    );
    cases
}

/// The repository path the hook event DTOs carry.
pub const HOOK_REPO_PATH: &str = "/repos/app";

pub fn branches() -> Cases<git_core::branch::BranchInfo> {
    vec![
        case(
            "tracking",
            git_core::branch::BranchInfo {
                name: "main".to_string(),
                is_current: true,
                upstream: some("origin/main"),
                ahead: Some(2),
                behind: Some(0),
            },
        ),
        case(
            "local-only",
            git_core::branch::BranchInfo {
                name: "topic".to_string(),
                is_current: false,
                upstream: None,
                ahead: None,
                behind: None,
            },
        ),
    ]
}

pub fn worktrees() -> Cases<git_core::worktree::WorktreeInfo> {
    vec![
        case(
            "main",
            git_core::worktree::WorktreeInfo {
                name: "app".to_string(),
                path: "/repos/app".into(),
                head: "4444444444444444444444444444444444444444".to_string(),
                is_main: true,
                is_locked: false,
                is_prunable: false,
            },
        ),
        case(
            "unknown-head",
            git_core::worktree::WorktreeInfo {
                name: "gone".to_string(),
                path: "/repos/gone".into(),
                head: String::new(),
                is_main: false,
                is_locked: true,
                is_prunable: true,
            },
        ),
    ]
}

pub fn submodules() -> Cases<git_core::submodule::SubmoduleInfo> {
    vec![
        case(
            "initialized",
            git_core::submodule::SubmoduleInfo {
                path: "vendor/lib".to_string(),
                url: some("https://example.com/lib.git"),
                gitlink_id: some("5555555555555555555555555555555555555555"),
                initialized: true,
                head_id: some("6666666666666666666666666666666666666666"),
                workdir: some("/repos/app/vendor/lib"),
            },
        ),
        case(
            "unknown",
            git_core::submodule::SubmoduleInfo {
                path: "vendor/other".to_string(),
                url: None,
                gitlink_id: None,
                initialized: false,
                head_id: None,
                workdir: None,
            },
        ),
    ]
}

pub fn reflog_entries() -> Cases<git_core::reflog::ReflogEntry> {
    let entry = |summary: Option<String>| git_core::reflog::ReflogEntry {
        reference: "HEAD".to_string(),
        old_id: "7777777777777777777777777777777777777777".to_string(),
        new_id: "8888888888888888888888888888888888888888".to_string(),
        committer_name: "Ada Lovelace".to_string(),
        committer_email: "ada@example.com".to_string(),
        timestamp: 1_700_000_100,
        message: "commit: Add".to_string(),
        summary,
    };
    vec![
        case("with-summary", entry(some("Add"))),
        case("no-summary", entry(None)),
    ]
}

pub fn remotes() -> Cases<(
    git_core::remote::RemoteInfo,
    Option<git_core::remote::RemoteAuthMode>,
)> {
    use git_core::remote::{RemoteAuthMode, RemoteInfo};
    let remote = |push_url: Option<String>, prune: bool| RemoteInfo {
        name: "origin".to_string(),
        fetch_url: "https://example.com/app.git".to_string(),
        push_url,
        prune,
    };
    let cases = vec![
        case("no-auth", (remote(None, false), None)),
        case(
            "HttpsToken",
            (
                remote(some("https://push.example.com/app.git"), true),
                Some(RemoteAuthMode::HttpsToken {
                    username: "ada".to_string(),
                }),
            ),
        ),
        case(
            "SshAgent",
            (remote(None, false), Some(RemoteAuthMode::SshAgent)),
        ),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, (_, mode))| mode),
        None,
        Some(RemoteAuthMode::HttpsToken { .. }),
        Some(RemoteAuthMode::SshAgent),
    );
    cases
}

pub fn upstreams() -> Cases<git_core::remote::UpstreamInfo> {
    vec![
        case(
            "known",
            git_core::remote::UpstreamInfo {
                local_branch: "main".to_string(),
                remote_name: "origin".to_string(),
                remote_branch: "main".to_string(),
                ahead: Some(1),
                behind: Some(3),
                push_branch: some("main"),
            },
        ),
        case(
            "unknown",
            git_core::remote::UpstreamInfo {
                local_branch: "topic".to_string(),
                remote_name: "origin".to_string(),
                remote_branch: "topic".to_string(),
                ahead: None,
                behind: None,
                push_branch: None,
            },
        ),
    ]
}

pub fn tags() -> Cases<git_core::remote::TagInfo> {
    vec![
        case(
            "annotated",
            git_core::remote::TagInfo {
                name: "v1.0.0".to_string(),
                target_id: "9999999999999999999999999999999999999999".to_string(),
                commit_id: some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                annotated: true,
                message: some("Release 1.0.0\n"),
                tagger_name: some("Ada Lovelace"),
                timestamp: Some(1_700_000_200),
            },
        ),
        case(
            "lightweight",
            git_core::remote::TagInfo {
                name: "tree-tag".to_string(),
                target_id: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                commit_id: None,
                annotated: false,
                message: None,
                tagger_name: None,
                timestamp: None,
            },
        ),
    ]
}

/// Every operation `Started` and as `Progress`, every phase as `Progress` (with remote text
/// that must not reach the wire), `RemoteFailed` and `Completed` with each error kind, and
/// `Completed` without one and with hook output.
pub fn transfer_events() -> Cases<TransferEvent> {
    let mut cases: Cases<_> = transfer_operations()
        .into_iter()
        .map(|operation| {
            case(
                format!("Started-{operation:?}"),
                TransferEvent::Started {
                    operation_id: "op-2".to_string(),
                    operation,
                },
            )
        })
        .collect();
    let progress = |operation: TransferOperation, phase: TransferPhase| {
        case(
            format!("Progress-{operation:?}-{phase:?}"),
            TransferEvent::Progress(git_core::remote::TransferProgress {
                operation_id: "op-2".to_string(),
                operation,
                phase,
                current: 3,
                total: 10,
                received_bytes: 4096,
                message: some("remote: secret sideband text"),
            }),
        )
    };
    cases.extend(
        transfer_phases()
            .into_iter()
            .map(|phase| progress(TransferOperation::Fetch, phase)),
    );
    cases.extend(
        transfer_operations()
            .into_iter()
            .filter(|operation| *operation != TransferOperation::Fetch)
            .map(|operation| progress(operation, TransferPhase::Receiving)),
    );
    cases.extend(transfer_error_kinds().into_iter().map(|error| {
        case(
            format!("RemoteFailed-{error:?}"),
            TransferEvent::RemoteFailed {
                operation_id: "op-2".to_string(),
                operation: TransferOperation::FetchAll,
                remote_name: "backup".to_string(),
                error,
            },
        )
    }));
    cases.push(case(
        "Completed-ok",
        TransferEvent::Completed {
            operation_id: "op-2".to_string(),
            operation: TransferOperation::Pull,
            error: None,
            hook_output: None,
        },
    ));
    cases.extend(transfer_error_kinds().into_iter().map(|kind| {
        case(
            format!("Completed-{kind:?}"),
            TransferEvent::Completed {
                operation_id: "op-2".to_string(),
                operation: TransferOperation::PushBranch,
                error: Some(kind),
                hook_output: (kind == TransferErrorKind::HookFailed).then(|| "rejected\n".into()),
            },
        )
    }));
    assert_every_variant!(
        cases.iter().map(|(_, event)| event),
        TransferEvent::Started { .. },
        TransferEvent::Progress(_),
        TransferEvent::RemoteFailed { .. },
        TransferEvent::Completed { .. },
    );
    cases
}

fn sequence_progress(kind: SequenceKind) -> SequenceProgress {
    SequenceProgress {
        kind,
        current_step: 2,
        total_steps: 3,
        current_commit: "cccccccccccccccccccccccccccccccccccccccc".to_string(),
    }
}

pub fn sequence_progresses() -> Cases<SequenceProgress> {
    sequence_kinds()
        .into_iter()
        .map(|kind| case(format!("{kind:?}"), sequence_progress(kind)))
        .collect()
}

fn rebase_info(health: RebaseHealth) -> git_core::repo::RebaseInfo {
    git_core::repo::RebaseInfo {
        branch: "topic".to_string(),
        onto: "dddddddddddddddddddddddddddddddddddddddd".to_string(),
        current_step: 1,
        total_steps: 4,
        health,
    }
}

pub fn rebase_infos() -> Cases<git_core::repo::RebaseInfo> {
    let mut cases = vec![case("Paused", rebase_info(RebaseHealth::Paused))];
    cases.extend(rebase_divergences().into_iter().map(|reason| {
        case(
            format!("Diverged-{reason:?}"),
            rebase_info(RebaseHealth::Diverged(reason)),
        )
    }));
    assert_every_variant!(
        cases.iter().map(|(_, info)| &info.health),
        RebaseHealth::Paused,
        RebaseHealth::Diverged(_),
    );
    cases
}

fn terminal_sequence(kind: SequenceKind) -> git_core::pick::TerminalSequence {
    git_core::pick::TerminalSequence {
        kind,
        paused_on_conflict: true,
        started_at: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_string(),
        started_at_summary: "Start here".to_string(),
        rewinds: true,
        commits_to_remove: 2,
        // Not on the wire.
        damage: Some(git_core::pick::SequencerDamage::MissingStart),
    }
}

pub fn terminal_sequences() -> Cases<git_core::pick::TerminalSequence> {
    let mut cases: Cases<_> = sequence_kinds()
        .into_iter()
        .map(|kind| case(format!("{kind:?}"), terminal_sequence(kind)))
        .collect();
    cases.push(case(
        "undamaged",
        git_core::pick::TerminalSequence {
            paused_on_conflict: false,
            rewinds: false,
            commits_to_remove: 0,
            damage: None,
            ..terminal_sequence(SequenceKind::CherryPick)
        },
    ));
    cases
}

fn terminal_rebase(kind: TerminalRebaseKind) -> git_core::repo::TerminalRebase {
    git_core::repo::TerminalRebase {
        kind,
        branch: "topic".to_string(),
        onto: "ffffffffffffffffffffffffffffffffffffffff".to_string(),
        current_step: 1,
        total_steps: 2,
    }
}

pub fn terminal_rebases() -> Cases<git_core::repo::TerminalRebase> {
    let mut cases: Cases<_> = terminal_rebase_kinds()
        .into_iter()
        .map(|kind| case(format!("{kind:?}"), terminal_rebase(kind)))
        .collect();
    cases.push(case(
        "unknown",
        git_core::repo::TerminalRebase {
            branch: String::new(),
            onto: String::new(),
            current_step: 0,
            total_steps: 0,
            ..terminal_rebase(TerminalRebaseKind::Am)
        },
    ));
    cases
}

fn empty_head() -> git_core::repo::HeadInfo {
    git_core::repo::HeadInfo {
        head_id: None,
        detached_at: None,
        head_pushed: false,
        operation: None,
        operation_leftover: false,
        squash_message: None,
        commit_hooks: Vec::new(),
        sequence: None,
        rebase: None,
        bisect: None,
        terminal_sequence: None,
        terminal_rebase: None,
    }
}

/// Every field empty, every field set, and each operation kind. The nested DTOs' own variants
/// are covered by their own cases.
pub fn head_infos() -> Cases<git_core::repo::HeadInfo> {
    let mut cases = vec![
        case("empty", empty_head()),
        case(
            "everything",
            git_core::repo::HeadInfo {
                head_id: some("1212121212121212121212121212121212121212"),
                detached_at: some("v1.0.0"),
                head_pushed: true,
                operation: Some(OperationKind::Merge),
                operation_leftover: true,
                squash_message: some("Squashed commit of the following:\n"),
                commit_hooks: strings(&["pre-commit", "commit-msg"]),
                sequence: Some(sequence_progress(SequenceKind::Revert)),
                rebase: Some(rebase_info(RebaseHealth::Paused)),
                bisect: Some(git_core::repo::BisectInfo {
                    started_from: "main".to_string(),
                }),
                terminal_sequence: Some(terminal_sequence(SequenceKind::CherryPick)),
                terminal_rebase: Some(terminal_rebase(TerminalRebaseKind::Rebase)),
            },
        ),
    ];
    cases.extend(operation_kinds().into_iter().map(|kind| {
        case(
            format!("operation-{kind:?}"),
            git_core::repo::HeadInfo {
                operation: Some(kind),
                ..empty_head()
            },
        )
    }));
    cases
}

pub fn rebase_aborts() -> Cases<git_core::rebase::RebaseAbort> {
    vec![
        case("restored", git_core::rebase::RebaseAbort { restored: true }),
        case(
            "not-restored",
            git_core::rebase::RebaseAbort { restored: false },
        ),
    ]
}

pub fn sequence_outcomes() -> Cases<git_core::pick::SequenceOutcome> {
    use git_core::pick::SequenceOutcome;
    let progress = || sequence_progress(SequenceKind::CherryPick);
    let cases = vec![
        case(
            "Completed",
            SequenceOutcome::Completed {
                applied: 2,
                skipped: strings(&["1313131313131313131313131313131313131313"]),
            },
        ),
        case(
            "Conflicted",
            SequenceOutcome::Conflicted {
                files: strings(&["a.txt"]),
                progress: progress(),
            },
        ),
        case(
            "HookFailed",
            SequenceOutcome::HookFailed {
                hook: "prepare-commit-msg".to_string(),
                output: "no ticket\n".to_string(),
                progress: progress(),
            },
        ),
        case(
            "Stopped",
            SequenceOutcome::Stopped {
                kept: "Kept commit".to_string(),
                progress: progress(),
            },
        ),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        SequenceOutcome::Completed { .. },
        SequenceOutcome::Conflicted { .. },
        SequenceOutcome::HookFailed { .. },
        SequenceOutcome::Stopped { .. },
    );
    cases
}

pub fn sequence_aborts() -> Cases<git_core::pick::SequenceAbort> {
    vec![
        case("rewound", git_core::pick::SequenceAbort { rewound: true }),
        case(
            "not-rewound",
            git_core::pick::SequenceAbort { rewound: false },
        ),
    ]
}

pub fn pick_outcomes() -> Cases<git_core::pick::PickOutcome> {
    use git_core::pick::PickOutcome;
    let cases = vec![
        case(
            "Committed",
            PickOutcome::Committed {
                commit_id: "1414141414141414141414141414141414141414".to_string(),
            },
        ),
        case(
            "Conflicted",
            PickOutcome::Conflicted {
                files: strings(&["a.txt", "b.txt"]),
            },
        ),
        case(
            "HookFailed",
            PickOutcome::HookFailed {
                hook: "prepare-commit-msg".to_string(),
                output: "no ticket\n".to_string(),
            },
        ),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        PickOutcome::Committed { .. },
        PickOutcome::Conflicted { .. },
        PickOutcome::HookFailed { .. },
    );
    cases
}

pub fn undo_states() -> Cases<repo_service::worker::UndoState> {
    vec![
        case(
            "both",
            repo_service::worker::UndoState {
                undo: some("Commit"),
                redo: some("Discard"),
            },
        ),
        case(
            "none",
            repo_service::worker::UndoState {
                undo: None,
                redo: None,
            },
        ),
    ]
}

pub fn commit_signatures() -> Cases<git_core::signing::CommitSignature> {
    use git_core::signing::{CommitSignature, SignatureFormat, SignatureStatus};
    let statuses = every!(SignatureStatus {
        Unsigned,
        Good,
        GoodUntrusted,
        Bad,
        UnknownKey,
        Expired,
        ExpiredKey,
        RevokedKey,
        Error
    });
    let formats = every!(SignatureFormat { OpenPgp, Ssh });
    let mut cases: Cases<_> = statuses
        .into_iter()
        .map(|status| {
            case(
                format!("status-{status:?}"),
                CommitSignature {
                    status,
                    format: None,
                    signer: None,
                    key: None,
                    detail: None,
                },
            )
        })
        .collect();
    cases.extend(formats.into_iter().map(|format| {
        case(
            format!("format-{format:?}"),
            CommitSignature {
                status: SignatureStatus::Good,
                format: Some(format),
                signer: some("Ada Lovelace <ada@example.com>"),
                key: some("ABCDEF0123456789"),
                detail: some("gpg: Good signature\n"),
            },
        )
    }));
    cases
}

pub fn undo_outcomes() -> Cases<repo_service::worker::UndoOutcome> {
    use repo_service::worker::UndoOutcome;
    let cases = vec![
        case(
            "Done",
            UndoOutcome::Done {
                label: "Commit".to_string(),
            },
        ),
        case("Nothing", UndoOutcome::Nothing),
        case(
            "RepositoryChanged",
            UndoOutcome::RepositoryChanged {
                label: "Commit".to_string(),
            },
        ),
        case(
            "BranchHeld",
            UndoOutcome::BranchHeld {
                label: "Switch".to_string(),
                message: "main is checked out in another worktree".to_string(),
            },
        ),
        case("OperationInProgress", UndoOutcome::OperationInProgress),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        UndoOutcome::Done { .. },
        UndoOutcome::Nothing,
        UndoOutcome::RepositoryChanged { .. },
        UndoOutcome::BranchHeld { .. },
        UndoOutcome::OperationInProgress,
    );
    cases
}

pub fn pull_outcomes() -> Cases<git_core::remote::PullOutcome> {
    use git_core::remote::PullOutcome;
    let cases = vec![
        case("UpToDate", PullOutcome::UpToDate),
        case(
            "FastForwarded",
            PullOutcome::FastForwarded {
                upstream_ref: "refs/remotes/origin/main".to_string(),
            },
        ),
        case(
            "Diverged",
            PullOutcome::Diverged {
                upstream_ref: "refs/remotes/origin/main".to_string(),
            },
        ),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        PullOutcome::UpToDate,
        PullOutcome::FastForwarded { .. },
        PullOutcome::Diverged { .. },
    );
    cases
}

pub fn stash_entries() -> Cases<git_core::stash::StashEntry> {
    vec![case(
        "plain",
        git_core::stash::StashEntry {
            index: 0,
            message: "WIP on main: Add".to_string(),
            commit_id: "1515151515151515151515151515151515151515".to_string(),
        },
    )]
}

pub fn stash_apply_outcomes() -> Cases<git_core::stash::StashApplyOutcome> {
    use git_core::stash::StashApplyOutcome;
    let cases = vec![
        case("Applied", StashApplyOutcome::Applied),
        case(
            "Conflicted",
            StashApplyOutcome::Conflicted {
                files: strings(&["a.txt"]),
            },
        ),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        StashApplyOutcome::Applied,
        StashApplyOutcome::Conflicted { .. },
    );
    cases
}

pub fn stash_files() -> Cases<git_core::stash::StashFiles> {
    vec![
        case(
            "with-untracked",
            git_core::stash::StashFiles {
                tracked: strings(&["a.txt"]),
                untracked: strings(&["new.txt"]),
                untracked_commit_id: some("1616161616161616161616161616161616161616"),
            },
        ),
        case(
            "tracked-only",
            git_core::stash::StashFiles {
                tracked: strings(&["a.txt"]),
                untracked: Vec::new(),
                untracked_commit_id: None,
            },
        ),
    ]
}

pub fn blame_lines() -> Cases<git_core::blame::BlameLine> {
    vec![case(
        "plain",
        git_core::blame::BlameLine {
            line_number: 1,
            content: "fn main() {}\n".to_string(),
            commit_id: "1717171717171717171717171717171717171717".to_string(),
            short_id: "1717171".to_string(),
            author_name: "Ada Lovelace".to_string(),
            timestamp: 1_700_000_300,
        },
    )]
}

pub fn merge_outcomes() -> Cases<git_core::merge::MergeOutcome> {
    use git_core::merge::MergeOutcome;
    let cases = vec![
        case("UpToDate", MergeOutcome::UpToDate),
        case("FastForwarded", MergeOutcome::FastForwarded),
        case("Merged", MergeOutcome::Merged),
        case("Squashed", MergeOutcome::Squashed),
        case(
            "Conflicted",
            MergeOutcome::Conflicted {
                files: strings(&["a.txt"]),
            },
        ),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        MergeOutcome::UpToDate,
        MergeOutcome::FastForwarded,
        MergeOutcome::Merged,
        MergeOutcome::Squashed,
        MergeOutcome::Conflicted { .. },
    );
    cases
}

pub fn merge_options() -> Cases<git_core::merge::MergeOptions> {
    every!(FastForward { Allow, Never, Only })
        .into_iter()
        .enumerate()
        .map(|(index, fast_forward)| {
            let squash = index % 2 == 1;
            case(
                format!("{fast_forward:?}-squash-{squash}"),
                git_core::merge::MergeOptions {
                    fast_forward,
                    squash,
                },
            )
        })
        .collect()
}

pub fn conflict_segments() -> Cases<git_core::merge::ConflictSegment> {
    use git_core::merge::ConflictSegment;
    let cases = vec![
        case(
            "Clean",
            ConflictSegment::Clean {
                content: "same\n".to_string(),
            },
        ),
        case(
            "Conflict",
            ConflictSegment::Conflict {
                ours: "ours\n".to_string(),
                theirs: "theirs\n".to_string(),
            },
        ),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, segment)| segment),
        ConflictSegment::Clean { .. },
        ConflictSegment::Conflict { .. },
    );
    cases
}

pub fn rebase_plan_commits() -> Cases<git_core::rebase::RebasePlanCommit> {
    vec![case(
        "plain",
        git_core::rebase::RebasePlanCommit {
            id: "1818181818181818181818181818181818181818".to_string(),
            short_id: "1818181".to_string(),
            summary: "Add".to_string(),
            author_name: "Ada Lovelace".to_string(),
            timestamp: 1_700_000_400,
        },
    )]
}

pub fn rebase_step_results() -> Cases<git_core::rebase::RebaseStepResult> {
    use git_core::rebase::RebaseStepResult;
    let cases = vec![
        case(
            "Conflicted",
            RebaseStepResult::Conflicted {
                files: strings(&["a.txt"]),
            },
        ),
        case("PausedForEdit", RebaseStepResult::PausedForEdit),
        case("Advanced", RebaseStepResult::Advanced),
        case("Done", RebaseStepResult::Done),
    ];
    assert_every_variant!(
        cases.iter().map(|(_, result)| result),
        RebaseStepResult::Conflicted { .. },
        RebaseStepResult::PausedForEdit,
        RebaseStepResult::Advanced,
        RebaseStepResult::Done,
    );
    cases
}

pub fn forge_repositories() -> Cases<git_core::forge::ForgeRepository> {
    use git_core::forge::ForgeProvider;
    every!(ForgeProvider { GitHub, Bitbucket })
        .into_iter()
        .map(|provider| {
            case(
                format!("{provider:?}"),
                git_core::forge::ForgeRepository {
                    provider,
                    host: "example.com".to_string(),
                    owner: "ada".to_string(),
                    name: "app".to_string(),
                    remote_name: "origin".to_string(),
                },
            )
        })
        .collect()
}

fn pull_request() -> repo_service::pull_requests::PullRequest {
    repo_service::pull_requests::PullRequest {
        id: "PR_1".to_string(),
        number: 42,
        title: "Add the graph".to_string(),
        url: "https://example.com/ada/app/pull/42".to_string(),
        author: "ada".to_string(),
        source_branch: "topic".to_string(),
        target_branch: "main".to_string(),
        state: "open".to_string(),
    }
}

pub fn pull_requests() -> Cases<repo_service::pull_requests::PullRequest> {
    vec![case("plain", pull_request())]
}

pub fn pull_request_lists() -> Cases<repo_service::pull_requests::PullRequestList> {
    vec![
        case(
            "truncated",
            repo_service::pull_requests::PullRequestList {
                pull_requests: vec![pull_request()],
                truncated: true,
            },
        ),
        case(
            "empty",
            repo_service::pull_requests::PullRequestList {
                pull_requests: Vec::new(),
                truncated: false,
            },
        ),
    ]
}

pub fn recent_authors() -> Cases<git_core::commit::SignatureInput> {
    vec![case(
        "plain",
        git_core::commit::SignatureInput {
            name: "Ada Lovelace".to_string(),
            email: "ada@example.com".to_string(),
        },
    )]
}
