//! The wire snapshot cases both transports share (#306, #488): every DTO in
//! `repo_service::wire`, built from the domain values in `wire_fixtures.rs` through the `From`
//! impls and helpers the commands and handlers call. Not part of the `test-support` library
//! (which can't depend on `repo-service`): both snapshot tests include this file with `#[path]`
//! next to `wire_fixtures.rs` (`crates/tauri-app/src/commands/wire_snapshots.rs` and
//! `crates/vscode-sidecar/src/dispatch/wire_snapshots.rs`). Each serializes the cases its own way
//! and adds what only it sends, so a DTO added here is snapshotted by both, and the
//! cross-transport test compares it.

use repo_service::wire::*;
use serde::Serialize;

use super::fixtures::{self, Cases};

/// Where a transport's own cases go among the shared ones, so its snapshot file keeps its order.
pub enum Slot {
    /// After `RepoWatch`: the desktop app's event names.
    AfterRepoWatch,
    /// After `TransferProgress`: the desktop app's event name for each.
    AfterTransferProgress,
}

/// A transport's snapshot list.
pub trait WireCases {
    /// One line per case, named `<dto>/<case>`, serialized as the transport sends it.
    fn add<T, D: Serialize>(&mut self, dto: &str, cases: Cases<T>, to_dto: impl Fn(T) -> D);

    /// Adds the transport's own cases that belong at `slot`.
    fn transport_cases(&mut self, slot: Slot);
}

/// Adds every shared case to `s`, in snapshot file order.
pub fn shared_wire_cases(s: &mut impl WireCases) {
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
        "FileHistoryPage",
        fixtures::file_history_pages(),
        FileHistoryPageDto::from,
    );
    s.add(
        "LineHistory",
        fixtures::line_histories(),
        LineHistoryDto::from,
    );
    s.add("SearchPage", fixtures::search_pages(), SearchPageDto::from);
    s.add("GraphPage", fixtures::graph_pages(), GraphPageDto::from);
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
        "UserCommand",
        fixtures::user_commands(),
        UserCommandDto::from,
    );
    s.add(
        "UserCommands",
        fixtures::user_command_lists(),
        UserCommandsDto::from,
    );
    s.add(
        "UserCommandCheck",
        fixtures::user_command_checks(),
        |(command_line, found)| {
            let template = git_core::user_command::parse(
                &command_line,
                None,
                git_core::process::CommandStyle::Posix,
            )
            .expect("fixture parses");
            UserCommandCheckDto::from_template(&template, |name| {
                found.then(|| std::path::PathBuf::from(format!("/usr/bin/{name}")))
            })
        },
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
    s.add("FontFamily", fixtures::font_families(), FontFamilyDto::from);
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
    s.add(
        "PatchApplyOutcome",
        fixtures::patch_apply_outcomes(),
        repo_service::wire::PatchApplyOutcomeDto::from,
    );
    s.add("HookProgress", fixtures::hook_events(), |event| {
        hook_progress_dto(fixtures::HOOK_REPO_PATH, event)
    });
    s.add("RepoWatch", fixtures::watch_events(), |event| {
        repo_service::wire::repo_watch_dto(fixtures::HOOK_REPO_PATH, event)
    });
    s.transport_cases(Slot::AfterRepoWatch);
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
    s.transport_cases(Slot::AfterTransferProgress);
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
        "ChangeStamp",
        fixtures::change_stamps(),
        ChangeStampDto::from,
    );
    s.add(
        "StatusWithStamp",
        fixtures::statuses_with_stamps(),
        StatusWithStampDto::from,
    );
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
}
