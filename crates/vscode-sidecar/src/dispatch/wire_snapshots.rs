//! #306: byte-exact snapshots of the sidecar's wire output, checked in as
//! `crates/vscode-sidecar/tests/wire/dto-snapshots.txt`. Each line is one DTO built from the
//! shared domain values in `crates/test-support/wire_fixtures.rs` through the same `From` impls
//! and helpers the handlers call (`repo_service::wire`, plus `OpenReposDto` in `dispatch/repos.rs`),
//! serialized the way the sidecar sends it. A second test compares the cases both
//! transports share with the desktop app's snapshots
//! (`crates/tauri-app/src/commands/wire_snapshots.rs`).
//! Regenerate with `UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar wire_snapshots`
//! (after the desktop app's, for the comparison).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use super::*;
use repo_service::wire::{
    transfer_progress_dto, AppearanceSettingsDto, BlameLineDto, BranchInfoDto, ChangedFileDto,
    CommitLocationDto, CommitMessageSettingsDto, CommitOutcomeDto, CommitSignatureDto,
    CompareRangeDto, ConflictSegmentDto, DiffHunkDto, DiffSettingsDto, ExternalToolSettingsDto,
    FetchSettingsDto, FileContentDto, FileHistoryEntryDto, FileSidesDto, ForgeRepositoryDto,
    GitToolsDto, GraphCommitDto, HeadInfoDto, LineHistoryDto, MarkResolvedOutcomeDto,
    MergeOptionsDto, MergeOutcomeDto, OpenRepoEntryDto, PickOutcomeDto, PullOutcomeDto,
    PullRequestDto, PullRequestListDto, PullStrategyDto, RebaseAbortDto, RebaseInfoDto,
    RebasePlanCommitDto, RebaseStepResultDto, ReflogEntryDto, RemoteInfoDto, SearchPageDto,
    SequenceAbortDto, SequenceOutcomeDto, SequenceProgressDto, StashApplyOutcomeDto, StashEntryDto,
    StashFilesDto, StashSwitchOutcomeDto, StatusEntryDto, SubmoduleInfoDto, TagInfoDto,
    TerminalRebaseDto, TerminalSequenceDto, TreeListingDto, UndoOutcomeDto, UndoStateDto,
    UpdateSettingsDto, UpstreamInfoDto, WorkspaceDto, WorktreeInfoDto,
};

#[path = "../../../test-support/wire_fixtures.rs"]
#[macro_use]
mod fixtures;

use fixtures::Cases;

/// What the sidecar sends: every handler answers a `serde_json::Value` (`to_value`) and every
/// notification wraps its DTO with `json!`, then `main.rs` and the relays write
/// `serde_json::to_string` of the message. `Value` objects sort their keys (serde_json's
/// `preserve_order` feature is off), so fields arrive in alphabetical order.
fn wire<T: Serialize>(value: &T) -> String {
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
    s.add(
        "TransferProgress",
        fixtures::transfer_events(),
        transfer_progress_dto,
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

    // Sidecar only: `list_open_repos`.
    s.add("OpenRepos", fixtures::open_repo_entries(), |entry| {
        let active = Some(entry.path.clone());
        OpenReposDto::from((vec![entry], active))
    });
    s.0.push((
        "OpenRepos/none".to_string(),
        wire(&OpenReposDto::from((Vec::new(), None))),
    ));
    // Every method's error, as the JSON-RPC error's `data` (#496).
    s.add("RepoErrorPayload", fixtures::errors(), |error| {
        crate::protocol::JsonRpcResponse::err(1, error)
            .error
            .expect("an error response")
            .data
    });
    s.0
}

#[test]
fn wire_snapshots_are_current() {
    let file = test_support::wire_snapshot_file(
        "# Generated: the VSCode sidecar's wire bytes, one `<dto>/<case> <json>` line per case (#306).\n\
         # Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar wire_snapshots\n",
        &snapshots(),
    );
    test_support::assert_generated_file(&sidecar_snapshot_path(), &file);
}

fn sidecar_snapshot_path() -> PathBuf {
    test_support::repo_root().join("crates/vscode-sidecar/tests/wire/dto-snapshots.txt")
}

/// What only the desktop app sends: its event names, clone and init (desktop only), the merge
/// tool outcome (the sidecar hands the merge tool to VSCode instead), and `list_open_repos`'s
/// tuple (the sidecar answers `OpenRepos`).
const DESKTOP_ONLY: &[&str] = &[
    // Tauri event names; the sidecar's notification methods are pinned by the relay tests below.
    "TransferProgressEvent",
    "HookProgressEvent",
    "CloneOutcome",
    "CloneErrorPayload",
    "MergeToolOutcome",
    "ListOpenRepos",
];

/// DTOs only the sidecar sends: `list_open_repos`'s object.
const SIDECAR_ONLY: &[&str] = &["OpenRepos"];

fn dto_of(case: &str) -> &str {
    case.split_once('/').map_or(case, |(dto, _)| dto)
}

/// Compares the desktop app's checked-in snapshot file (its own test keeps it equal to that
/// transport's output) with this transport's snapshots. Every case of a shared DTO must exist in both. Where the bytes of a
/// shared case differ, `crates/vscode-sidecar/tests/wire/cross-transport-differences.txt` lists
/// it as `key-order` (same JSON value, other field order) or `value` (a different value). That
/// file pins today's differences; it is regenerated like the snapshots.
#[test]
fn shared_dtos_match_across_transports() {
    let desktop: BTreeMap<String, String> = test_support::read_wire_snapshot(
        &test_support::repo_root().join("crates/tauri-app/tests/wire/dto-snapshots.txt"),
    )
    .into_iter()
    .collect();
    // This transport's side comes from `snapshots()` itself, which `wire_snapshots_are_current`
    // keeps equal to its file.
    let sidecar: BTreeMap<String, String> = snapshots().into_iter().collect();

    for dto in DESKTOP_ONLY {
        assert!(
            desktop.keys().any(|case| dto_of(case) == *dto),
            "{dto} is listed as desktop-only but has no desktop snapshot"
        );
    }
    for dto in SIDECAR_ONLY {
        assert!(
            sidecar.keys().any(|case| dto_of(case) == *dto),
            "{dto} is listed as sidecar-only but has no sidecar snapshot"
        );
    }
    for case in desktop.keys() {
        assert!(
            sidecar.contains_key(case) || DESKTOP_ONLY.contains(&dto_of(case)),
            "{case} has a desktop snapshot but no sidecar snapshot"
        );
    }
    for case in sidecar.keys() {
        assert!(
            desktop.contains_key(case) || SIDECAR_ONLY.contains(&dto_of(case)),
            "{case} has a sidecar snapshot but no desktop snapshot"
        );
    }

    let mut differences = String::from(
        "# Generated: shared wire DTO cases whose bytes differ between the desktop app and the\n\
         # VSCode sidecar (#306). `key-order`: same JSON value, fields in another order.\n\
         # `value`: a different JSON value.\n\
         # Do not edit. Regenerate: UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar wire_snapshots\n",
    );
    for (case, desktop_json) in &desktop {
        let Some(sidecar_json) = sidecar.get(case) else {
            continue;
        };
        if desktop_json == sidecar_json {
            continue;
        }
        let parse = |json: &str| -> Value {
            serde_json::from_str(json).unwrap_or_else(|err| panic!("{case}: {err}"))
        };
        let kind = if parse(desktop_json) == parse(sidecar_json) {
            "key-order"
        } else {
            "value"
        };
        differences.push_str(&format!("{case} {kind}\n"));
    }
    test_support::assert_generated_file(
        &test_support::repo_root()
            .join("crates/vscode-sidecar/tests/wire/cross-transport-differences.txt"),
        &differences,
    );
}

/// The lines a relay writes to `out` once every event is sent and its sender dropped.
fn relayed_lines(
    relay: impl FnOnce(Arc<Mutex<Vec<u8>>>) -> std::thread::JoinHandle<()>,
) -> Vec<String> {
    let out = Arc::new(Mutex::new(Vec::new()));
    relay(Arc::clone(&out)).join().unwrap();
    let bytes = out.lock().unwrap().clone();
    String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

/// Checks `wire()` against the real writer: every transfer event through
/// `spawn_progress_relay` is its snapshot line inside the notification, byte for byte.
#[test]
fn the_progress_relay_writes_the_snapshot_bytes() {
    let snapshots: BTreeMap<String, String> = snapshots().into_iter().collect();
    let events = fixtures::transfer_events();
    let names: Vec<String> = events.iter().map(|(name, _)| name.clone()).collect();
    let lines = relayed_lines(|out| {
        let (event_tx, event_rx) = mpsc::channel();
        for (_, event) in events {
            event_tx.send(event).unwrap();
        }
        drop(event_tx);
        spawn_progress_relay(event_rx, out)
    });
    assert_eq!(lines.len(), names.len());
    for (name, line) in names.iter().zip(&lines) {
        let params = &snapshots[&format!("TransferProgress/{name}")];
        assert_eq!(
            *line,
            format!(r#"{{"jsonrpc":"2.0","method":"transferProgress","params":{params}}}"#),
            "{name}"
        );
    }
}

/// Same for `spawn_hook_relay` and the hook events.
#[test]
fn the_hook_relay_writes_the_snapshot_bytes() {
    let snapshots: BTreeMap<String, String> = snapshots().into_iter().collect();
    let events = fixtures::hook_events();
    let names: Vec<String> = events.iter().map(|(name, _)| name.clone()).collect();
    let lines = relayed_lines(|out| {
        let (event_tx, event_rx) = mpsc::channel();
        for (_, event) in events {
            event_tx.send(event).unwrap();
        }
        drop(event_tx);
        spawn_hook_relay(fixtures::HOOK_REPO_PATH.to_string(), event_rx, out)
    });
    assert_eq!(lines.len(), names.len());
    for (name, line) in names.iter().zip(&lines) {
        let params = &snapshots[&format!("HookProgress/{name}")];
        assert_eq!(
            *line,
            format!(r#"{{"jsonrpc":"2.0","method":"hookProgress","params":{params}}}"#),
            "{name}"
        );
    }
}
