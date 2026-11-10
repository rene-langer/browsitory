//! #306: byte-exact snapshots of the desktop app's wire output, checked in as
//! `crates/tauri-app/tests/wire/dto-snapshots.txt`. Each line is one DTO built from the shared
//! domain values in `crates/test-support/wire_fixtures.rs` through the `From` impls and helpers
//! the commands use (`repo_service::wire`, plus this crate's desktop-only DTOs and event names),
//! serialized the way Tauri sends it. The sidecar's snapshot test compares
//! the cases both transports share (see `crates/vscode-sidecar/src/dispatch/wire_snapshots.rs`).
//! Regenerate with `UPDATE_TRANSPORT_METHODS=1 cargo test -p tauri-app wire_snapshots`.

use serde::Serialize;

use super::clone::{CloneErrorDto, CloneOutcomeDto};
use super::*;
use repo_service::wire::{
    hook_progress_dto, AppearanceSettingsDto, BlameLineDto, BranchInfoDto, ChangedFileDto,
    CommitLocationDto, CommitMessageSettingsDto, CommitOutcomeDto, CommitSignatureDto,
    CompareRangeDto, ConflictSegmentDto, DiffHunkDto, DiffSettingsDto, ExternalToolSettingsDto,
    FetchSettingsDto, FileContentDto, FileHistoryEntryDto, FileSidesDto, ForgeRepositoryDto,
    GitToolsDto, GraphCommitDto, HeadInfoDto, LineHistoryDto, MarkResolvedOutcomeDto,
    MergeOptionsDto, MergeOutcomeDto, OpenRepoEntryDto, PickOutcomeDto, PullOutcomeDto,
    PullRequestDto, PullRequestListDto, PullStrategyDto, RebaseAbortDto, RebaseInfoDto,
    RebasePlanCommitDto, RebaseStepResultDto, ReflogEntryDto, RemoteInfoDto, SearchPageDto,
    SequenceAbortDto, SequenceOutcomeDto, SequenceProgressDto, SignatureInputDto,
    StashApplyOutcomeDto, StashEntryDto, StashFilesDto, StashSwitchOutcomeDto, StatusEntryDto,
    SubmoduleInfoDto, TagInfoDto, TerminalRebaseDto, TerminalSequenceDto, TreeListingDto,
    UndoOutcomeDto, UndoStateDto, UpdateSettingsDto, UpstreamInfoDto, WorkspaceDto,
    WorktreeInfoDto,
};

#[path = "../../../test-support/wire_fixtures.rs"]
#[macro_use]
mod fixtures;

use fixtures::Cases;

/// What Tauri sends for a command's answer (`IpcResponse::body`) and an event's payload
/// (`EmitArgs::new`): `serde_json::to_string` of the value, fields in declaration order.
fn wire<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

/// What Tauri sends for a command's error: `InvokeError::from` makes a `serde_json::Value`
/// first, whose objects sort their keys.
fn wire_error<T: Serialize>(value: &T) -> String {
    serde_json::to_string(&serde_json::to_value(value).unwrap()).unwrap()
}

#[derive(Default)]
struct Snapshots(Vec<(String, String)>);

impl Snapshots {
    /// One line per case, named `<dto>/<case>`.
    fn add<T, D: Serialize>(&mut self, dto: &str, cases: Cases<T>, to_dto: impl Fn(T) -> D) {
        for (case, value) in cases {
            self.0.push((format!("{dto}/{case}"), wire(&to_dto(value))));
        }
    }
}

fn snapshots() -> Vec<(String, String)> {
    let mut s = Snapshots::default();
    s.add(
        "StatusEntry",
        fixtures::status_entries(),
        StatusEntryDto::from,
    );
    s.add(
        "GraphCommit",
        fixtures::graph_commits(),
        GraphCommitDto::from,
    );
    s.add("DiffHunk", fixtures::diff_hunks(), DiffHunkDto::from);
    s.add(
        "FileHistoryEntry",
        fixtures::file_history_entries(),
        FileHistoryEntryDto::from,
    );
    s.add(
        "LineHistory",
        fixtures::line_histories(),
        LineHistoryDto::from,
    );
    s.add("SearchPage", fixtures::search_pages(), SearchPageDto::from);
    s.add(
        "ChangedFile",
        fixtures::changed_files(),
        ChangedFileDto::from,
    );
    s.add(
        "CompareRange",
        fixtures::compare_ranges(),
        CompareRangeDto::from,
    );
    s.add(
        "CommitLocation",
        fixtures::commit_locations(),
        CommitLocationDto::from,
    );
    s.add(
        "CommitStatsEntry",
        fixtures::commit_stats_entries(),
        repo_service::wire::CommitStatsEntryDto::from,
    );
    s.add(
        "CommitDetails",
        fixtures::commit_details(),
        repo_service::wire::CommitDetailsDto::from,
    );
    s.add("PatchContent", fixtures::patch_contents(), |bytes| {
        repo_service::wire::PatchContentDto::from_bytes(&bytes)
    });
    s.add(
        "OpenRepoEntry",
        fixtures::open_repo_entries(),
        OpenRepoEntryDto::from,
    );
    s.add("Workspace", fixtures::workspaces(), WorkspaceDto::from);
    s.add(
        "RepoAlias",
        fixtures::repo_aliases(),
        repo_service::wire::RepoAliasDto::from,
    );
    s.add(
        "DiffSettings",
        fixtures::diff_settings(),
        DiffSettingsDto::from,
    );
    s.add(
        "ExternalToolSettings",
        fixtures::external_tool_settings(),
        ExternalToolSettingsDto::from,
    );
    s.add(
        "CommitMessageSettings",
        fixtures::commit_message_settings(),
        CommitMessageSettingsDto::from,
    );
    s.add(
        "FetchSettings",
        fixtures::fetch_settings(),
        FetchSettingsDto::from,
    );
    s.add(
        "AppearanceSettings",
        fixtures::appearance_settings(),
        AppearanceSettingsDto::from,
    );
    s.add(
        "UpdateSettings",
        fixtures::update_settings(),
        UpdateSettingsDto::from,
    );
    s.add("GitTools", fixtures::git_tools(), GitToolsDto::from);
    s.add(
        "MarkResolvedOutcome",
        fixtures::mark_resolved_outcomes(),
        MarkResolvedOutcomeDto::from,
    );
    s.add("FileSides", fixtures::file_sides(), FileSidesDto::from);
    s.add(
        "TreeListing",
        fixtures::tree_listings(),
        TreeListingDto::from,
    );
    s.add(
        "FileContent",
        fixtures::file_contents(),
        FileContentDto::from,
    );
    s.add(
        "CommitOutcome",
        fixtures::commit_outcomes(),
        CommitOutcomeDto::from,
    );
    s.add("HookProgress", fixtures::hook_events(), |event| {
        hook_progress_dto(fixtures::HOOK_REPO_PATH, event)
    });
    // The Tauri event they are all emitted as.
    s.0.push((
        "HookProgressEvent/all".to_string(),
        wire(&HOOK_PROGRESS_EVENT),
    ));
    s.add("BranchInfo", fixtures::branches(), BranchInfoDto::from);
    s.add("WorktreeInfo", fixtures::worktrees(), WorktreeInfoDto::from);
    s.add(
        "SubmoduleInfo",
        fixtures::submodules(),
        SubmoduleInfoDto::from,
    );
    s.add(
        "ReflogEntry",
        fixtures::reflog_entries(),
        ReflogEntryDto::from,
    );
    s.add("RemoteInfo", fixtures::remotes(), RemoteInfoDto::from);
    s.add("UpstreamInfo", fixtures::upstreams(), UpstreamInfoDto::from);
    s.add("TagInfo", fixtures::tags(), TagInfoDto::from);
    s.add("TransferProgress", fixtures::transfer_events(), |event| {
        transfer_event_payload(event).1
    });
    // The Tauri event each one is emitted as.
    s.add(
        "TransferProgressEvent",
        fixtures::transfer_events(),
        |event| transfer_event_payload(event).0,
    );
    s.add(
        "SequenceProgress",
        fixtures::sequence_progresses(),
        SequenceProgressDto::from,
    );
    s.add("RebaseInfo", fixtures::rebase_infos(), RebaseInfoDto::from);
    s.add(
        "TerminalSequence",
        fixtures::terminal_sequences(),
        TerminalSequenceDto::from,
    );
    s.add(
        "TerminalRebase",
        fixtures::terminal_rebases(),
        TerminalRebaseDto::from,
    );
    s.add("HeadInfo", fixtures::head_infos(), HeadInfoDto::from);
    s.add(
        "RebaseAbort",
        fixtures::rebase_aborts(),
        RebaseAbortDto::from,
    );
    s.add(
        "SequenceOutcome",
        fixtures::sequence_outcomes(),
        SequenceOutcomeDto::from,
    );
    s.add(
        "SequenceAbort",
        fixtures::sequence_aborts(),
        SequenceAbortDto::from,
    );
    s.add(
        "PickOutcome",
        fixtures::pick_outcomes(),
        PickOutcomeDto::from,
    );
    s.add("UndoState", fixtures::undo_states(), UndoStateDto::from);
    s.add(
        "CommitSignature",
        fixtures::commit_signatures(),
        CommitSignatureDto::from,
    );
    s.add(
        "UndoOutcome",
        fixtures::undo_outcomes(),
        UndoOutcomeDto::from,
    );
    s.add(
        "PullOutcome",
        fixtures::pull_outcomes(),
        PullOutcomeDto::from,
    );
    s.add("StashEntry", fixtures::stash_entries(), StashEntryDto::from);
    s.add(
        "StashApplyOutcome",
        fixtures::stash_apply_outcomes(),
        StashApplyOutcomeDto::from,
    );
    s.add(
        "StashSwitchOutcome",
        fixtures::stash_switch_outcomes(),
        StashSwitchOutcomeDto::from,
    );
    s.add(
        "PullStrategy",
        fixtures::pull_strategies(),
        PullStrategyDto::from,
    );
    s.add("StashFiles", fixtures::stash_files(), StashFilesDto::from);
    s.add("BlameLine", fixtures::blame_lines(), BlameLineDto::from);
    s.add(
        "MergeOutcome",
        fixtures::merge_outcomes(),
        MergeOutcomeDto::from,
    );
    s.add(
        "MergeOptions",
        fixtures::merge_options(),
        MergeOptionsDto::from,
    );
    s.add(
        "ConflictSegment",
        fixtures::conflict_segments(),
        ConflictSegmentDto::from,
    );
    s.add(
        "RebasePlanCommit",
        fixtures::rebase_plan_commits(),
        RebasePlanCommitDto::from,
    );
    s.add(
        "RebaseStepResult",
        fixtures::rebase_step_results(),
        RebaseStepResultDto::from,
    );
    s.add(
        "ForgeRepository",
        fixtures::forge_repositories(),
        ForgeRepositoryDto::from,
    );
    s.add(
        "PullRequest",
        fixtures::pull_requests(),
        PullRequestDto::from,
    );
    s.add(
        "PullRequestList",
        fixtures::pull_request_lists(),
        PullRequestListDto::from,
    );
    s.add(
        "SignatureInput",
        fixtures::recent_authors(),
        SignatureInputDto::from,
    );

    // Desktop only.
    s.add(
        "MergeToolOutcome",
        merge_tool_outcomes(),
        MergeToolOutcomeDto::from,
    );
    s.add("ListOpenRepos", fixtures::open_repo_entries(), |entry| {
        // `list_open_repos` answers a tuple, which Tauri sends as a two-element array.
        let active = Some(entry.path.to_string_lossy().into_owned());
        (vec![OpenRepoEntryDto::from(entry)], active)
    });
    s.0.push((
        "ListOpenRepos/none".to_string(),
        wire(&(Vec::<OpenRepoEntryDto>::new(), None::<String>)),
    ));
    s.add("CloneOutcome", clone_outcomes(), CloneOutcomeDto::from);
    for (case, error) in clone_errors() {
        s.0.push((format!("CloneErrorPayload/{case}"), wire_error(&error)));
    }
    // Every other command's error (#496).
    for (case, error) in fixtures::errors() {
        s.0.push((format!("RepoErrorPayload/{case}"), wire_error(&error)));
    }
    s.0
}

fn merge_tool_outcomes() -> Cases<git_core::tools::MergeToolOutcome> {
    use git_core::tools::MergeToolOutcome;
    let cases: Cases<_> = [MergeToolOutcome::Resolved, MergeToolOutcome::Unresolved]
        .into_iter()
        .map(|outcome| (format!("{outcome:?}"), outcome))
        .collect();
    assert_every_variant!(
        cases.iter().map(|(_, outcome)| outcome),
        MergeToolOutcome::Resolved,
        MergeToolOutcome::Unresolved,
    );
    cases
}

fn clone_outcomes() -> Cases<repo_service::clone::ServiceCloneOutcome> {
    let outcome = |submodule_failures: Vec<String>, credential_save_failed| {
        repo_service::clone::ServiceCloneOutcome {
            outcome: git_core::clone::CloneOutcome {
                path: "/repos/cloned".into(),
                submodule_failures,
            },
            credential_save_failed,
        }
    };
    vec![
        ("clean".to_string(), outcome(Vec::new(), false)),
        (
            "partial".to_string(),
            outcome(vec!["vendor/lib".to_string()], true),
        ),
    ]
}

/// Every `CloneError` and `RepoError` mapping, plus the `Io` error `clone_repo` builds itself
/// when its blocking task fails.
fn clone_errors() -> Vec<(String, CloneErrorDto)> {
    use git_core::clone::CloneError;
    use git_core::repo::RepoError;
    let mut clone_errors: Vec<(String, CloneError)> = vec![
        ("InvalidUrl".into(), CloneError::InvalidUrl),
        (
            "CredentialBearingUrl".into(),
            CloneError::CredentialBearingUrl,
        ),
        ("DestNotEmpty".into(), CloneError::DestNotEmpty),
        ("BranchNotFound".into(), CloneError::BranchNotFound),
        ("Cancelled".into(), CloneError::Cancelled),
        (
            "Io".into(),
            CloneError::Io(std::io::Error::other("disk full")),
        ),
        (
            "CaseCollision".into(),
            CloneError::CaseCollision(git_core::CaseCollision {
                link: "Docs".to_string(),
                folder: "docs".to_string(),
            }),
        ),
        (
            "CaseConfig".into(),
            CloneError::CaseConfig(git_core::CaseConfigUnreadable(
                "failed to parse 'maybe' as a boolean".to_string(),
            )),
        ),
        (
            "FileInUse".into(),
            CloneError::HeldOpen(git_core::HeldOpen {
                path: "f.txt".to_string(),
                during: git_core::HeldDuring::Clone,
            }),
        ),
    ];
    clone_errors.extend(fixtures::transfer_error_kinds().into_iter().map(|kind| {
        (
            format!("Transfer-{kind:?}"),
            CloneError::Transfer {
                kind,
                message: "The transfer failed.".to_string(),
            },
        )
    }));
    assert_every_variant!(
        clone_errors.iter().map(|(_, error)| error),
        CloneError::InvalidUrl,
        CloneError::CredentialBearingUrl,
        CloneError::DestNotEmpty,
        CloneError::BranchNotFound,
        CloneError::Cancelled,
        CloneError::Transfer { .. },
        CloneError::Io(_),
        CloneError::CaseCollision(_),
        CloneError::CaseConfig(_),
        CloneError::HeldOpen(_),
    );
    let init_errors: Vec<(String, RepoError)> = vec![
        (
            "Open".into(),
            RepoError::Open(git2::Error::from_str("could not open")),
        ),
        ("AlreadyRepository".into(), RepoError::AlreadyRepository),
        (
            "Init".into(),
            RepoError::Init(git2::Error::from_str("could not create")),
        ),
        (
            "Io".into(),
            RepoError::Io(std::io::Error::other("read-only")),
        ),
    ];
    assert_every_variant!(
        init_errors.iter().map(|(_, error)| error),
        RepoError::Open(_),
        RepoError::AlreadyRepository,
        RepoError::Init(_),
        RepoError::Io(_),
    );
    let mut out: Vec<(String, CloneErrorDto)> = clone_errors
        .into_iter()
        .map(|(case, error)| (format!("clone-{case}"), CloneErrorDto::from(error)))
        .collect();
    out.extend(
        init_errors
            .into_iter()
            .map(|(case, error)| (format!("init-{case}"), CloneErrorDto::from(error))),
    );
    out.push((
        "task-failed".to_string(),
        CloneErrorDto {
            kind: "Io".into(),
            message: "task panicked".to_string(),
        },
    ));
    out
}

#[test]
fn wire_snapshots_are_current() {
    let file = test_support::wire_snapshot_file(
        "# Generated: the desktop app's wire bytes, one `<dto>/<case> <json>` line per case (#306).\n\
         # Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p tauri-app wire_snapshots\n",
        &snapshots(),
    );
    test_support::assert_generated_file(
        &test_support::repo_root().join("crates/tauri-app/tests/wire/dto-snapshots.txt"),
        &file,
    );
}
