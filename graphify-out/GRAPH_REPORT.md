# Graph Report - browsitory  (2026-08-18)

## Corpus Check
- 196 files · ~191,886 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1870 nodes · 4814 edges · 121 communities (79 shown, 42 thin omitted)
- Extraction: 91% EXTRACTED · 9% INFERRED · 0% AMBIGUOUS · INFERRED: 441 edges (avg confidence: 0.8)
- Token cost: 833,377 input · 0 output

## Community Hubs (Navigation)
- Worker Thread & Command Channel
- Remote & Credential Provider (git-core)
- Forge Pull Request Client
- OS Keychain Credential Service
- RepoClient Interface Methods
- RepoClient IPC & Forge UI
- Submodule Management (git-core)
- Pull Request Fixture Tests
- Phase 0 Setup & Worker Rationale
- Commit Graph Tests
- Branch Management (git-core)
- Interactive Rebase Tests
- Forge Repository Detection
- Agent Guidance Docs (CLAUDE/AGENTS.md)
- Commit Creation (git-core)
- Tauri Command Layer
- Diff & Branch DTOs
- Forge & Conflict DTOs
- E2E Test Dependencies
- Pull Request & Worktree Commands
- Forge/Remote Frontend Panels
- Rebase Plan Execution (git-core)
- Blame & Branch Tests
- Phase 2 Design Docs
- Merge Conflict Tests
- Credential Release & RemotePanel Tests
- Blame/Commit/Diff UI Components
- App Shell & Reflog Panel
- Frontend TypeScript App Config
- Recent Repos Config
- Merge Conflict Resolution (git-core)
- Phase 3 Remote Workflows Design
- RepoClient & useAppState Tasks
- Frontend TypeScript Node Config
- Commit Graph UI Components
- Diff Extraction (git-core)
- Rebase Planner UI
- Log & Diff Core Functions
- Reflog (git-core)
- Remote/Tag/Reflog DTOs
- Status (git-core)
- ESLint Frontend Config
- Branch Switcher & Worktree UI
- Pull Request Panel Tests
- Stash (git-core)
- Transfer & Pull/Push Commands
- Blame (git-core)
- Frontend Base tsconfig
- Reflog Missing-Log Edge Cases
- Tauri Default Permissions
- Phase 3 Implementation Plans
- Graft Hooks Script
- Graft Statusline Script
- Commit Graph (git-core)
- Rebase Continue/Step DTOs
- Phase 4 Design Doc
- Conflict Resolution Pane UI
- License Policy & Vite Entry Docs
- Repo Open (git-core)
- Pull Request List DTOs
- DiffView/DiffPane/CommitBox Tasks
- Frontend Runtime Dependencies
- Recent Repos Tasks
- Frontend package.json Identity
- Frontend npm Scripts
- Remote Transfer E2E Spec
- Threading Model Summary
- Reflog HEAD Restoration Tests
- Blame DTO/Command
- Submodule DTO/Command
- Tag DTO/Command
- Worktree DTO/Command
- MCP Graft Server Config
- Graft Integration Guidance
- Reflog E2E Spec
- Remote Management E2E Spec
- Worktree E2E Spec
- tsconfig References
- Workspace Crate Set
- git2 vs gitoxide Rationale
- Phase 4 Pull Requests Plan
- Phase 4 Reflog Plan
- Phase 4 Submodules Plan
- Phase 4 Worktrees Plan
- Blame Viewer E2E Spec
- Branch Management E2E Spec
- Commit Graph E2E Spec
- First-Flow E2E Spec
- Merge E2E Spec
- Rebase E2E Spec
- Stash Management E2E Spec
- Submodule E2E Spec
- React Hooks ESLint Plugin
- Globals Dependency
- Tauri CLI Dependency
- Testing Library jest-dom
- Testing Library React
- Types Node Dependency
- Types React Dependency
- TypeScript Dependency
- TypeScript ESLint Dependency
- Vite React Plugin
- Vitest Dependency
- Transfer Failure Messages
- Graphify Token Benchmark
- Graphify FalkorDB Export
- Graphify MCP Server
- Graphify SVG/GraphML Export
- Graphify Cross-Repo Merge
- Graphify CLAUDE.md Hook
- Graphify Cluster-Only Mode
- App Icon 128x128@2x
- App Icon 128x128
- App Icon 32x32
- Frontend Favicon Icon
- Frontend Icon Sprite Sheet

## God Nodes (most connected - your core abstractions)
1. `init_repo()` - 97 edges
2. `write_file()` - 95 edges
3. `commit_all()` - 84 edges
4. `RepoClient` - 84 edges
5. `AppState` - 69 edges
6. `worker_handle()` - 69 edges
7. `WorkerHandle` - 69 edges
8. `useAppState()` - 68 edges
9. `UseAppStateResult` - 53 edges
10. `Command` - 35 edges

## Surprising Connections (you probably didn't know these)
- `git2 API gotchas (AGENTS.md)` --semantically_similar_to--> `CLAUDE.md project guidance`  [INFERRED] [semantically similar]
  AGENTS.md → CLAUDE.md
- `CLAUDE.md project guidance` --references--> `DiffPane()`  [EXTRACTED]
  CLAUDE.md → frontend/src/components/DiffPane.tsx
- `Example Task 1.A.02: Diff Viewer` --references--> `DiffView()`  [EXTRACTED]
  docs/TASK_TEMPLATE.md → frontend/src/components/DiffView.tsx
- `Error handling across the IPC boundary` --references--> `RepoClient`  [EXTRACTED]
  docs/ARCHITECTURE.md → frontend/src/ipc/RepoClient.ts
- `The RepoClient IPC boundary` --references--> `RepoClient`  [EXTRACTED]
  docs/ARCHITECTURE.md → frontend/src/ipc/RepoClient.ts

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Phase 2 subsystems sharing the Worker/Command/Tauri-command/RepoClient chain** — docs_superpowers_plans_2026_08_12_branch_management_branch_module, docs_superpowers_plans_2026_08_12_stash_stash_module, docs_superpowers_plans_2026_08_13_blame_blame_module, docs_superpowers_plans_2026_08_13_commit_graph_graph_module, docs_superpowers_plans_2026_08_14_merge_merge_module, docs_superpowers_plans_2026_08_15_interactive_rebase_rebase_module [INFERRED 0.85]
- **Phase 3 remote-workflow plans all extending crates/git-core/src/remote.rs** — docs_superpowers_plans_2026_08_14_phase3_credentials_credentialservice, docs_superpowers_plans_2026_08_14_phase3_pull_pulloutcome, docs_superpowers_plans_2026_08_14_phase3_push_tags_tag_push_functions, docs_superpowers_plans_2026_08_14_phase3_remote_management_remote_module, docs_superpowers_plans_2026_08_14_phase3_transfer_progress_transfer_contract [EXTRACTED 1.00]
- **Phase 4 subsystems following the same git-core -> Worker -> Tauri command -> RepoClient vertical slice pattern** — docs_superpowers_plans_2026_08_15_browsitory_phase4_worktrees_worktree_module, docs_superpowers_plans_2026_08_15_browsitory_phase4_submodules_submodule_module, docs_superpowers_plans_2026_08_15_browsitory_phase4_reflog_reflog_module, docs_superpowers_plans_2026_08_15_browsitory_phase4_pull_requests_forge_module [INFERRED 0.85]
- **Phase 2's six subsystems (branch mgmt, stash, blame, commit graph, merge, rebase)** — docs_superpowers_specs_2026_08_12_branch_management_design, docs_superpowers_specs_2026_08_12_stash_design, docs_superpowers_specs_2026_08_13_blame_design, docs_superpowers_specs_2026_08_13_commit_graph_design, docs_superpowers_specs_2026_08_14_merge_design, docs_superpowers_specs_2026_08_15_interactive_rebase_design [EXTRACTED 1.00]
- **Phase 4's four independently shippable workstreams** — phase4_worktrees, phase4_submodules, phase4_reflog, phase4_pull_requests [EXTRACTED 1.00]
- **Phase 1 git-core-to-IPC dependency pipeline (log/diff/stage+commit feed the Tauri command task)** — docs_tasks_phase_1_a_01_log, docs_tasks_phase_1_a_02_diff, docs_tasks_phase_1_a_03_stage_and_commit, docs_tasks_phase_1_c_01_git_ipc_commands [EXTRACTED 1.00]
- **graphify SKILL.md pipeline and its reference docs** — claude_skills_graphify_skill_pipeline, claude_skills_graphify_references_github_and_merge_clone, claude_skills_graphify_references_extraction_spec_prompt, claude_skills_graphify_references_query_vocab_expansion, claude_skills_graphify_references_update_incremental, claude_skills_graphify_references_exports_wiki, claude_skills_graphify_references_hooks_post_commit, claude_skills_graphify_references_transcribe_video, claude_skills_graphify_references_add_watch_add_command [EXTRACTED 1.00]
- **Parallel Claude Code / Codex agent guidance files** — claude_md_overview, agents_md_overview, claude_skills_browsitory_conventions_skill_rules, agents_skills_browsitory_conventions_skill_rules, claude_skills_graft_skill_tools, agents_skills_graft_skill_tools [INFERRED 0.85]
- **Phase 3 credential and push/tag delivery reports** — superpowers_sdd_2026_08_14_phase3_credentials_task_3_report_delivered, superpowers_sdd_2026_08_14_phase3_credentials_task_4_report_delivered, superpowers_sdd_2026_08_14_phase3_push_tags_task_3_report_delivered, docs_architecture_credential_release_acceptance, frontend_src_components_remotepanel_remotepanel [INFERRED 0.85]

## Communities (121 total, 42 thin omitted)

### Community 0 - "Worker Thread & Command Channel"
Cohesion: 0.06
Nodes (85): Arc, abort_merge_round_trips_through_the_worker(), apply_then_drop_stash_round_trips_through_the_worker(), ChannelReporter, Command, commit_all(), commits_since_and_start_rebase_round_trip_through_the_worker(), create_pull_request_round_trips_through_the_worker_without_exposing_the_token() (+77 more)

### Community 1 - "Remote & Credential Provider (git-core)"
Cohesion: 0.06
Nodes (85): add_remote(), clear_current_upstream(), clear_remote_auth_profile(), contains_embedded_credentials(), create_tag(), CredentialProvider, current_local_branch_name(), current_upstream() (+77 more)

### Community 2 - "Forge Pull Request Client"
Cohesion: 0.07
Nodes (77): Client, a_401_response_becomes_a_secret_free_unauthorized_error(), a_bitbucket_pull_request_missing_display_name_falls_back_to_an_unknown_author(), a_bitbucket_validation_response_surfaces_the_nested_error_message(), a_github_link_header_without_rel_next_is_not_truncated(), a_github_pull_request_with_a_null_user_falls_back_to_an_unknown_author(), a_github_validation_response_surfaces_the_providers_message_without_the_token(), a_timeout_from_the_transport_becomes_a_secret_free_timeout_error() (+69 more)

### Community 3 - "OS Keychain Credential Service"
Cohesion: 0.07
Nodes (55): a_saved_https_credential_is_never_returned_by_the_forge_token_lookup(), CredentialKey, CredentialService, CredentialService<S>, CredentialStore, CredentialStoreError, derives_a_key_without_the_default_https_port(), derives_an_ipv6_key_without_the_default_https_port() (+47 more)

### Community 5 - "RepoClient IPC & Forge UI"
Cohesion: 0.04
Nodes (3): Error handling across the IPC boundary, ForgeRepositorySection(), RepoClient

### Community 6 - "Submodule Management (git-core)"
Cohesion: 0.14
Nodes (45): collect_gitlinks(), download_target_from_origin(), download_target_from_url(), ensure_nested_submodules_initialized_at(), ensure_recursive_update_is_safe(), find_submodule(), init_submodule(), list_submodules() (+37 more)

### Community 7 - "Pull Request Fixture Tests"
Cohesion: 0.05
Nodes (23): BITBUCKET_CREATE_FIXTURE, BITBUCKET_LIST_FIXTURE, GITHUB_CREATE_FIXTURE, GITHUB_LIST_FIXTURE, CapturedForgeRequest, classifyProviderRoute(), closeSharedForgeFixtureServer(), defaultResponses (+15 more)

### Community 8 - "Phase 0 Setup & Worker Rationale"
Cohesion: 0.06
Nodes (42): Browsitory Phase 0 Setup Plan, git_core::repo::open, RepoClient IPC Interface, git_core::status::status, Worker (per-repo thread), Worker Thread Message-Passing Rationale, git_core::branch module, BranchSwitcher component (+34 more)

### Community 9 - "Commit Graph Tests"
Cohesion: 0.09
Nodes (32): init_repo(), Path, Repository, TempDir, graph_log_reports_branch_refs_only_for_tip_commits(), graph_log_reports_empty_branch_refs_for_a_non_tip_commit(), graph_log_reports_multiple_parent_ids_for_a_merge_commit(), graph_log_respects_the_limit() (+24 more)

### Community 10 - "Branch Management (git-core)"
Cohesion: 0.13
Nodes (35): BranchError, BranchInfo, create_branch(), delete_branch(), list_branches(), rename_branch(), resolve_start_point(), Commit (+27 more)

### Community 11 - "Interactive Rebase Tests"
Cohesion: 0.15
Nodes (35): commit_all(), a_clean_multi_pick_rebase_lands_every_commit_and_finishes(), a_conflicting_pick_pauses_and_resolving_then_continuing_lands_it(), a_drop_between_a_squash_groups_leader_and_member_does_not_panic(), a_mixed_squash_and_fixup_group_still_collapses_to_one_commit(), a_squash_group_without_an_explicit_combined_message_falls_back_to_the_leaders_message(), abort_rebase_after_a_conflict_also_recovers_cleanly(), abort_rebase_restores_the_original_branch_and_tip_exactly() (+27 more)

### Community 12 - "Forge Repository Detection"
Cohesion: 0.14
Nodes (32): classify_host_and_path(), classify_remote_url(), classify_scheme_url(), ClassifyError, detect_forge_repositories(), ForgeError, ForgeIdentity, ForgeProvider (+24 more)

### Community 13 - "Agent Guidance Docs (CLAUDE/AGENTS.md)"
Cohesion: 0.08
Nodes (34): git2 API gotchas (AGENTS.md), AGENTS.md project guidance, Browsitory conventions (Codex/AGENTS variant), .claude/CLAUDE.md graphify trigger, graphify usage rules (CLAUDE.md), CLAUDE.md project guidance, RTK token-optimized command instructions, Task workflow section (CLAUDE.md) (+26 more)

### Community 14 - "Commit Creation (git-core)"
Cohesion: 0.10
Nodes (29): ConfigLevel, commit(), CommitError, Error, Repository, Result, String, Error (+21 more)

### Community 15 - "Tauri Command Layer"
Cohesion: 0.21
Nodes (33): abort_merge(), abort_rebase(), apply_stash(), AppState, clear_current_upstream(), commit(), create_branch(), delete_branch() (+25 more)

### Community 16 - "Diff & Branch DTOs"
Cohesion: 0.12
Nodes (26): BranchInfoDto, commits_since(), DiffHunkDto, DiffLineDto, get_commit_diff(), get_commit_files(), get_commit_graph(), get_remote_upstreams() (+18 more)

### Community 17 - "Forge & Conflict DTOs"
Cohesion: 0.10
Nodes (23): ConflictSegmentDto, detect_forge_repository(), FileConflictChoiceDto, ForgeProviderDto, ForgeRepositoryDto, forget_forge_token(), get_conflict_hunks(), get_current_upstream() (+15 more)

### Community 18 - "E2E Test Dependencies"
Cohesion: 0.06
Nodes (30): devDependencies, tsx, @types/mocha, @types/node, typescript, @wdio/cli, @wdio/globals, @wdio/local-runner (+22 more)

### Community 19 - "Pull Request & Worktree Commands"
Cohesion: 0.11
Nodes (20): crate::pull_requests::CreatePullRequest, create_pull_request(), create_worktree(), CreatePullRequestDto, expected_diff_origin_wire_value(), MergeOutcomeDto, missing_credential_failure_is_emitted_as_a_safe_terminal_kind(), open_repo() (+12 more)

### Community 20 - "Forge/Remote Frontend Panels"
Cohesion: 0.20
Nodes (24): ForgeRepositorySectionProps, CreatePullRequest, DiffLine, FileConflictChoice, ForgeProvider, ForgeRepository, MergeOutcome, PullOutcome (+16 more)

### Community 21 - "Rebase Plan Execution (git-core)"
Cohesion: 0.25
Nodes (26): abort_rebase(), advance(), commits_since(), conflicted_paths(), ends_a_group(), finish(), land_current_step(), next_non_drop_action() (+18 more)

### Community 22 - "Blame & Branch Tests"
Cohesion: 0.11
Nodes (24): blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point(), blame_file_attributes_all_lines_to_the_single_commit(), blame_file_on_a_missing_path_returns_an_error(), blame_file_reattributes_only_the_changed_lines_after_an_edit(), create_branch_from_a_specific_commit_uses_that_commit_as_start_point(), create_branch_from_head_adds_and_switches_to_it(), delete_branch_fails_when_deleting_the_current_branch_even_with_force(), delete_branch_with_force_deletes_an_unmerged_branch() (+16 more)

### Community 23 - "Phase 2 Design Docs"
Cohesion: 0.10
Nodes (27): Branch Management Design, Stash Design, Blame Design, Commit Graph Design, Merge (with Conflict Resolution) Design, Interactive Rebase Design, abort_merge function, BranchSwitcher 'merge into current branch' action (+19 more)

### Community 24 - "Merge Conflict Tests"
Cohesion: 0.12
Nodes (25): a_commit_made_after_resolving_a_conflict_has_two_parents(), abort_merge_errors_and_leaves_uncommitted_work_untouched_when_no_merge_is_in_progress(), abort_merge_restores_the_pre_merge_working_tree_and_clears_conflicts(), commit_after_a_fast_forward_has_a_single_parent_as_before(), conflict_hunks_errors_for_a_path_with_no_conflict(), conflict_hunks_returns_clean_and_conflict_segments_for_a_conflicted_file(), conflict_hunks_round_trip_preserves_the_original_files_trailing_newline(), make_conflicted_repo() (+17 more)

### Community 25 - "Credential Release & RemotePanel Tests"
Cohesion: 0.09
Nodes (19): TransferErrorKind::MissingCredential, forge-fixture-override Cargo feature, Credential release acceptance checklist, Testing strategy, RemotePanel(), backup, IsOptional, origin (+11 more)

### Community 26 - "Blame/Commit/Diff UI Components"
Cohesion: 0.14
Nodes (13): BlameView(), lines, CommitBox(), DiffPane(), fakeClient(), unused(), DiffView(), originPrefix() (+5 more)

### Community 27 - "App Shell & Reflog Panel"
Cohesion: 0.13
Nodes (12): App(), ReflogPanel(), entry, RepoPicker(), fakeClient(), unimplemented(), SubmodulePanel(), initializedSubmodule (+4 more)

### Community 28 - "Frontend TypeScript App Config"
Cohesion: 0.08
Nodes (23): compilerOptions, allowArbitraryExtensions, allowImportingTsExtensions, erasableSyntaxOnly, jsx, lib, module, moduleDetection (+15 more)

### Community 29 - "Recent Repos Config"
Cohesion: 0.26
Nodes (19): Config, add_recent_repo(), add_recent_repo_at(), config_file_path(), ConfigError, ConfigFile, list_recent_repos(), list_recent_repos_at() (+11 more)

### Community 30 - "Merge Conflict Resolution (git-core)"
Cohesion: 0.26
Nodes (21): abort_merge(), conflict_hunks(), conflict_path(), ConflictSegment, FileConflictChoice, find_conflict(), is_merging(), merge_message() (+13 more)

### Community 31 - "Phase 3 Remote Workflows Design"
Cohesion: 0.11
Nodes (21): git-core module shape convention (thiserror enum + &Repository fns + real temp-dir tests), RepoClient extension pattern: typed union methods, tauriRepoClient sole @tauri-apps/api importer, Phase 3 Remote Workflows Design, commit() merge-aware parent selection, start_merge function, OS-keychain-backed HTTPS credential service, keyring 4.1.6 crate dependency, Five independently shippable Phase 3 plans (+13 more)

### Community 32 - "RepoClient & useAppState Tasks"
Cohesion: 0.19
Nodes (20): RepoClient interface extension, runMutation try/refresh/catch helper, useAppState hook, Browsitory Phase 1 Design, Task 1.D.01: Extend RepoClient, Task 1.D.02: useAppState hook, Task 1.E.01: RepoPicker component, Task 1.E.03: HistoryList component (+12 more)

### Community 33 - "Frontend TypeScript Node Config"
Cohesion: 0.10
Nodes (19): compilerOptions, allowImportingTsExtensions, erasableSyntaxOnly, lib, module, moduleDetection, noEmit, noFallthroughCasesInSwitch (+11 more)

### Community 34 - "Commit Graph UI Components"
Cohesion: 0.20
Nodes (12): CommitGraph(), rowsEqual(), commits, status, CommitLaneGraphic(), LANE_COLORS, laneCenterX(), laneColor() (+4 more)

### Community 35 - "Diff Extraction (git-core)"
Cohesion: 0.29
Nodes (17): commit_and_parent_trees(), commit_diff(), commit_files(), DiffError, DiffHunk, DiffLine, DiffLineOrigin, hunks_from_diff() (+9 more)

### Community 36 - "Rebase Planner UI"
Cohesion: 0.19
Nodes (16): ActionKind, defaultCombinedMessage(), groupFingerprints(), isGroupMember(), isLeader(), nextNonDropActionKind(), RebasePlanner(), recomputeGroupLeaders() (+8 more)

### Community 37 - "Log & Diff Core Functions"
Cohesion: 0.19
Nodes (16): CommitInfo struct, log() function (git-core::log), commit_diff function, commit_files function, RefCell shared-borrow pattern for Diff::foreach callbacks, working_diff function, commit() function (git-core::commit), stage_file function (+8 more)

### Community 38 - "Reflog (git-core)"
Cohesion: 0.31
Nodes (15): is_local_reference(), list_reflog_refs(), read_reflog(), ReflogEntry, ReflogError, restore_reflog_entry(), Error, Option (+7 more)

### Community 39 - "Remote/Tag/Reflog DTOs"
Cohesion: 0.18
Nodes (13): add_remote(), create_tag(), get_merge_message(), get_reflog(), ReflogEntryDto, RemoteAuthModeDto, RemoteInfoDto, Option (+5 more)

### Community 40 - "Status (git-core)"
Cohesion: 0.27
Nodes (12): Error, Option, Repository, Result, String, Vec, staged_kind(), status() (+4 more)

### Community 41 - "ESLint Frontend Config"
Cohesion: 0.15
Nodes (13): eslint, @eslint/js, eslint-plugin-react-refresh, devDependencies, eslint, @eslint/js, eslint-plugin-react-refresh, jsdom (+5 more)

### Community 42 - "Branch Switcher & Worktree UI"
Cohesion: 0.21
Nodes (7): BranchSwitcher(), branches, branches, linkedWorktree, mainWorktree, WorktreePanel(), BranchInfo

### Community 43 - "Pull Request Panel Tests"
Cohesion: 0.17
Nodes (10): PullRequestPanel(), bitbucketRepo, githubRepo, openPullRequest, PullRequest, remote, remoteManagementClient, transferClient() (+2 more)

### Community 44 - "Stash (git-core)"
Cohesion: 0.35
Nodes (11): apply_stash(), drop_stash(), list_stashes(), Error, Repository, Result, String, Vec (+3 more)

### Community 45 - "Transfer & Pull/Push Commands"
Cohesion: 0.27
Nodes (10): AppHandle, emit_transfer_events(), fetch_remote(), pick_repo_folder(), pull_current_upstream(), PullOutcomeDto, push_current_branch(), push_tags() (+2 more)

### Community 46 - "Blame (git-core)"
Cohesion: 0.31
Nodes (10): blame_file(), BlameError, BlameLine, resolve_commit_id(), Error, Oid, Repository, Result (+2 more)

### Community 47 - "Frontend Base tsconfig"
Cohesion: 0.18
Nodes (10): compilerOptions, module, moduleResolution, skipLibCheck, strict, target, types, node (+2 more)

### Community 48 - "Reflog Missing-Log Edge Cases"
Cohesion: 0.44
Nodes (9): create_orphan_commit(), init_bare_repo(), reading_or_restoring_a_local_ref_without_a_reflog_does_not_create_one(), remove_reflog_file(), restoring_an_unborn_head_to_an_orphan_commit_does_not_create_a_ref_or_reflog(), Oid, Repository, TempDir (+1 more)

### Community 49 - "Tauri Default Permissions"
Cohesion: 0.20
Nodes (9): description, identifier, permissions, $schema, windows, core:default, dialog:default, main (+1 more)

### Community 50 - "Phase 3 Implementation Plans"
Cohesion: 0.22
Nodes (10): CredentialService / CredentialStore, Credential Handling Implementation Plan, Pull Merge and Rebase Implementation Plan, PullOutcome / pull_after_fetch, Push and Tag Management Implementation Plan, list_tags/create_tag/delete_tag/push_current_branch/push_tags, Remote and Upstream Management Implementation Plan, git_core::remote module (remotes + upstream) (+2 more)

### Community 51 - "Graft Hooks Script"
Cohesion: 0.31
Nodes (8): candidates(), entry(), { execFileSync }, fromPkg(), fs, globalRoot(), path, { pathToFileURL }

### Community 52 - "Graft Statusline Script"
Cohesion: 0.31
Nodes (8): candidates(), entry(), { execFileSync }, fromPkg(), fs, globalRoot(), path, { pathToFileURL }

### Community 53 - "Commit Graph (git-core)"
Cohesion: 0.33
Nodes (8): graph_log(), GraphCommit, GraphError, Error, Repository, Result, String, Vec

### Community 54 - "Rebase Continue/Step DTOs"
Cohesion: 0.32
Nodes (6): git_core::rebase::RebasePlanEntry, rebase_continue(), RebasePlanEntryDto, RebaseStepResultDto, RebaseStepResult, start_rebase()

### Community 55 - "Phase 4 Design Doc"
Cohesion: 0.32
Nodes (8): Browsitory Phase 4 Design, Provider-neutral forge HTTP client service, PullRequest DTO, Pull-request integration feature (GitHub/Bitbucket), Reflog viewer and recovery feature, Existing RepoClient->Worker->git-core boundary unchanged for Phase 4, Submodules feature, Worktrees feature

### Community 56 - "Conflict Resolution Pane UI"
Cohesion: 0.36
Nodes (6): ConflictResolutionPane(), Resolution, fakeClient(), segments, unused(), ConflictSegment

### Community 57 - "License Policy & Vite Entry Docs"
Cohesion: 0.29
Nodes (7): License policy summary (CLAUDE.md), License compliance dependency table, Dependency-addition license verification process, frontend/index.html Vite entry point, frontend/README.md Vite template notes, frontend/src/main.tsx entry script, CI frontend job

### Community 58 - "Repo Open (git-core)"
Cohesion: 0.43
Nodes (6): open(), RepoError, Error, Path, Repository, Result

### Community 59 - "Pull Request List DTOs"
Cohesion: 0.38
Nodes (5): list_pull_requests(), PullRequestDto, PullRequestListDto, PullRequest, PullRequestList

### Community 60 - "DiffView/DiffPane/CommitBox Tasks"
Cohesion: 0.43
Nodes (7): Task 1.E.02: DiffView component, Task 1.E.04: DiffPane and CommitBox components, DiffView component, CommitBox component, DiffPane component, DiffPane branches uncommitted (staging controls) vs commit (read-only), CommitBox initialMessage prop

### Community 61 - "Frontend Runtime Dependencies"
Cohesion: 0.29
Nodes (7): dependencies, react, react-dom, @tauri-apps/api, react, react-dom, @tauri-apps/api

### Community 62 - "Recent Repos Tasks"
Cohesion: 0.47
Nodes (6): list_recent_repos / add_recent_repo, pub _at functions split for tempdir testability, open_repo best-effort recent-repo recording, pick_repo_folder command, Task 1.B.01: config recent-repos registry, Task 1.C.02: Tauri commands for repo picking + recent repos

### Community 63 - "Frontend package.json Identity"
Cohesion: 0.33
Nodes (5): name, packageManager, private, type, version

### Community 64 - "Frontend npm Scripts"
Cohesion: 0.33
Nodes (6): scripts, build, dev, lint, preview, test

### Community 65 - "Remote Transfer E2E Spec"
Cohesion: 0.40
Nodes (3): BARE_REMOTE_PATH, E2E_REPO_PATH, REMOTE_SOURCE_PATH

### Community 66 - "Threading Model Summary"
Cohesion: 0.50
Nodes (4): Threading model summary (AGENTS.md), tauri-app commands.rs, Worker::spawn, Threading model (per-repo worker-thread message passing)

### Community 67 - "Reflog HEAD Restoration Tests"
Cohesion: 0.50
Nodes (3): rejects_head_restoration_when_symbolic_head_resolves_to_a_remote_tracking_ref(), restores_a_detached_head_directly(), restores_an_attached_symbolic_head_by_moving_its_local_branch()

### Community 68 - "Blame DTO/Command"
Cohesion: 0.67
Nodes (3): BlameLineDto, get_blame(), BlameLine

### Community 69 - "Submodule DTO/Command"
Cohesion: 0.67
Nodes (3): list_submodules(), SubmoduleInfo, SubmoduleInfoDto

### Community 70 - "Tag DTO/Command"
Cohesion: 0.67
Nodes (3): list_tags(), TagInfo, TagInfoDto

### Community 71 - "Worktree DTO/Command"
Cohesion: 0.67
Nodes (3): list_worktrees(), WorktreeInfo, WorktreeInfoDto

### Community 72 - "MCP Graft Server Config"
Cohesion: 0.50
Nodes (3): npx, graft, @nanonets/graft

### Community 73 - "Graft Integration Guidance"
Cohesion: 0.67
Nodes (3): Graft repo-context-graph integration block, graft tools reference (Codex variant), graft tools reference (Claude Code variant)

### Community 78 - "Workspace Crate Set"
Cohesion: 0.67
Nodes (3): config, git-core, tauri-app

## Knowledge Gaps
- **272 isolated node(s):** `path`, `fs`, `{ pathToFileURL }`, `{ execFileSync }`, `path` (+267 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **42 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `write_file()` connect `Blame & Branch Tests` to `Reflog HEAD Restoration Tests`, `Submodule Management (git-core)`, `Commit Graph Tests`, `Interactive Rebase Tests`, `Commit Creation (git-core)`, `Merge Conflict Tests`?**
  _High betweenness centrality (0.051) - this node is a cross-community bridge._
- **Why does `commit_all()` connect `Interactive Rebase Tests` to `Reflog HEAD Restoration Tests`, `Submodule Management (git-core)`, `Commit Graph Tests`, `Blame & Branch Tests`, `Merge Conflict Tests`?**
  _High betweenness centrality (0.039) - this node is a cross-community bridge._
- **Why does `recursive_update_preserves_the_selected_head_when_a_nested_submodule_is_uninitialized()` connect `Submodule Management (git-core)` to `Worker Thread & Command Channel`, `Interactive Rebase Tests`, `Blame & Branch Tests`?**
  _High betweenness centrality (0.034) - this node is a cross-community bridge._
- **Are the 94 inferred relationships involving `init_repo()` (e.g. with `blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point()` and `blame_file_attributes_all_lines_to_the_single_commit()`) actually correct?**
  _`init_repo()` has 94 INFERRED edges - model-reasoned connections that need verification._
- **Are the 93 inferred relationships involving `write_file()` (e.g. with `blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point()` and `blame_file_attributes_all_lines_to_the_single_commit()`) actually correct?**
  _`write_file()` has 93 INFERRED edges - model-reasoned connections that need verification._
- **Are the 82 inferred relationships involving `commit_all()` (e.g. with `blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point()` and `blame_file_attributes_all_lines_to_the_single_commit()`) actually correct?**
  _`commit_all()` has 82 INFERRED edges - model-reasoned connections that need verification._
- **What connects `path`, `fs`, `{ pathToFileURL }` to the rest of the system?**
  _272 weakly-connected nodes found - possible documentation gaps or missing edges._