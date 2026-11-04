//! #306: byte-exact snapshots of the sidecar's wire output, checked in as
//! `crates/vscode-sidecar/tests/wire/dto-snapshots.txt`. Each line is one DTO built from the
//! shared domain values in `crates/test-support/wire_fixtures.rs` the way this file's handlers
//! build it, serialized the way the sidecar sends it. A second test compares the cases both
//! transports share with the desktop app's snapshots
//! (`crates/tauri-app/src/commands/wire_snapshots.rs`).
//! Regenerate with `UPDATE_TRANSPORT_METHODS=1 cargo test -p vscode-sidecar wire_snapshots`
//! (after the desktop app's, for the comparison).

use std::collections::BTreeMap;

use serde::Serialize;

use super::*;

#[path = "../../../test-support/wire_fixtures.rs"]
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
    // As `get_file_history` builds it.
    s.add("FileHistoryEntry", fixtures::file_history_entries(), |e| {
        FileHistoryEntryDto {
            commit: GraphCommitDto::from(e.commit),
            path: e.path,
            old_path: e.old_path,
        }
    });
    // As `search_commits` builds it.
    s.add("SearchPage", fixtures::search_pages(), |page| {
        SearchPageDto {
            commits: page.commits.into_iter().map(GraphCommitDto::from).collect(),
            next_cursor: page.next_cursor,
            scanned: page.scanned,
        }
    });
    s.add(
        "ChangedFile",
        fixtures::changed_files(),
        ChangedFileDto::from,
    );
    // As `get_branch_compare_range` builds it.
    s.add("CompareRange", fixtures::compare_ranges(), |range| {
        CompareRangeDto {
            from_id: range.from_id,
            to_id: range.to_id,
        }
    });
    s.add(
        "OpenRepoEntry",
        fixtures::open_repo_entries(),
        OpenRepoEntryDto::from,
    );
    s.add("Workspace", fixtures::workspaces(), WorkspaceDto::from);
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
    // As `get_commit_message_settings` builds it.
    s.add(
        "CommitMessageSettings",
        fixtures::commit_message_settings(),
        |settings| CommitMessageSettingsDto {
            summary_limit: settings.summary_limit,
            body_ruler: settings.body_ruler,
            show_ruler: settings.show_ruler,
        },
    );
    // As `get_git_tools` builds it.
    s.add("GitTools", fixtures::git_tools(), |tools| GitToolsDto {
        diff_tool: tools.diff_tool.map(Into::into),
        merge_tool: tools.merge_tool.map(Into::into),
    });
    s.add(
        "MarkResolvedOutcome",
        fixtures::mark_resolved_outcomes(),
        MarkResolvedOutcomeDto::from,
    );
    // As `get_file_sides` builds it.
    s.add("FileSides", fixtures::file_sides(), |sides| FileSidesDto {
        old: sides.old.into(),
        new: sides.new.into(),
    });
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
    // As `abort_rebase` builds it.
    s.add("RebaseAbort", fixtures::rebase_aborts(), |abort| {
        RebaseAbortDto {
            restored: abort.restored,
        }
    });
    s.add(
        "SequenceOutcome",
        fixtures::sequence_outcomes(),
        SequenceOutcomeDto::from,
    );
    // As `abort_sequence` builds it.
    s.add("SequenceAbort", fixtures::sequence_aborts(), |abort| {
        SequenceAbortDto {
            rewound: abort.rewound,
        }
    });
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
    // As `get_stash_files` builds it.
    s.add("StashFiles", fixtures::stash_files(), |files| {
        StashFilesDto {
            tracked: files.tracked,
            untracked: files.untracked,
            untracked_commit_id: files.untracked_commit_id,
        }
    });
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
    // As `list_recent_authors` builds it.
    s.add("SignatureInput", fixtures::recent_authors(), |author| {
        SignatureInputDto {
            name: author.name,
            email: author.email,
        }
    });

    // Sidecar only: `list_open_repos`, as `list_open_repos_handler` builds it.
    s.add("OpenRepos", fixtures::open_repo_entries(), |entry| {
        let active_path = Some(entry.path.to_string_lossy().into_owned());
        OpenReposDto {
            entries: vec![OpenRepoEntryDto::from(entry)],
            active_path,
        }
    });
    s.0.push((
        "OpenRepos/none".to_string(),
        wire(&OpenReposDto {
            entries: Vec::new(),
            active_path: None,
        }),
    ));
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

/// DTOs only the desktop app sends: clone and init (desktop only), the merge tool outcome (the
/// sidecar hands the merge tool to VSCode instead), and `list_open_repos`'s tuple (the sidecar
/// answers `OpenRepos`).
const DESKTOP_ONLY: &[&str] = &[
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

/// Compares the two checked-in snapshot files, which their own tests keep equal to each
/// transport's output. Every case of a shared DTO must exist in both. Where the bytes of a
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
    let sidecar: BTreeMap<String, String> =
        test_support::read_wire_snapshot(&sidecar_snapshot_path())
            .into_iter()
            .collect();

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
