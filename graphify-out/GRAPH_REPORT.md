# Graph Report - browsitory  (2026-08-22)

## Corpus Check
- 134 files · ~275,966 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2044 nodes · 5144 edges · 179 communities (86 shown, 93 thin omitted)
- Extraction: 90% EXTRACTED · 10% INFERRED · 0% AMBIGUOUS · INFERRED: 489 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Result
- commands.rs
- pull requests.rs
- src/remote.rs
- credentials.rs
- useAppState()
- RepoClient
- src/submodule.rs
- Browsitory Phase 1 Design
- RepoClient.ts
- src/worktree.rs
- add remote()
- commit all()
- useAppState.test.ts
- write file()
- config/src/lib.rs
- src/rebase.rs
- init repo()
- Merge (with Conflict Resolution)
- devDependencies
- tests/merge.rs
- Toolbar.tsx
- compilerOptions
- CommitGraph.tsx
- src/merge.rs
- devDependencies
- Phase 3 Remote Workflows
- Phase 1: full repo
- App.tsx
- BranchSwitcher.tsx
- compilerOptions
- wdio.conf.ts
- DiffPane.tsx
- src/diff.rs
- commands.ts
- RebasePlanner.tsx
- useOpenRepos.ts
- Task 1.C.01: Worker +
- graphify SKILL.md main pipeline
- src/reflog.rs
- sidebar.ts
- pull-requests.spec.ts
- forgeFixtureServer.ts
- status()
- src/stash.rs
- frontend/package.json
- graft-hooks.cjs
- graft-statusline.cjs
- blame file()
- compilerOptions
- SplitView()
- reflog missing log.rs
- default.json
- git core::remote module (remotes
- ConfigSearchPathOverride
- graph log()
- dependencies
- Visual Design System
- Browsitory Phase 4 Design
- e2e/package.json
- Task File Template Format
- commit()
- open()
- tests/graph.rs
- Repository Overview Interface
- Task 1.E.04: DiffPane and
- SubmodulePanel.tsx
- Task 1.C.02: Tauri commands
- prunes stale worktree metadata
- Browsitory Source App Icon
- remote-transfer.spec.ts
- screenshots.docs.ts
- worktree.spec.ts
- RepoClient IPC Boundary
- Multi-Repo Tabs Implementation
- multi-repo.spec.ts
- graft tools reference (Codex
- Rust Token Killer
- Layout Primitives
- @types/node
- frontend/tsconfig.json
- graft
- tauri-app
- Graph Query
- Phase 4 Pull Requests
- Phase 4 Reflog Implementation
- Phase 4 Submodules Implementation
- Phase 4 Worktrees Implementation
- Command Palette Registry
- blame-viewer.spec.ts
- commit-graph.spec.ts
- first-flow.spec.ts
- hunk-staging.spec.ts
- rebase.spec.ts
- eslint
- typescript-eslint
- RepoWorkspace()
- credentialFailureMessage()
- git2 API gotchas (AGENTS.md)
- Threading model summary (AGENTS.md)
- Folder Watcher
- URL Ingestion
- Graph Exports
- Semantic Extraction
- Cross Repository Merge
- Commit Hook
- Media Transcription
- Incremental Update
- Project Workflow
- Token reduction benchmark
- --falkordb / --falkordb-push export
- --mcp stdio server export
- --svg / --graphml export
- Cross-repo / monorepo graph
- Native CLAUDE.md integration
- --cluster-only re-clustering
- BlameLine
- BranchInfo
- ConflictSegment
- DiffHunk
- ForgeProvider
- ForgeRepository
- From
- GraphCommit
- MergeOutcome
- PullOutcome
- PullRequest
- PullRequestList
- RebasePlanCommit
- RebaseStepResult
- ReflogEntry
- RemoteAuthMode
- RemoteInfo
- StashEntry
- SubmoduleInfo
- TagInfo
- TransferProgress
- UpstreamInfo
- WorktreeInfo
- A
- BlameLine
- BranchInfo
- ConflictSegment
- DiffHunk
- ForgeProvider
- ForgeRepository
- GraphCommit
- MergeOutcome
- PullOutcome
- PullRequest
- PullRequestList
- RebasePlanCommit
- RebasePlanEntry
- RebaseStepResult
- RefCell
- ReflogEntry
- RemoteAuthMode
- RemoteInfo
- S
- StashEntry
- SubmoduleInfo
- TagInfo
- TempDir
- TransferProgress
- UpstreamInfo
- VecDeque
- WorktreeInfo
- CreatePullRequest
- Original App Mark
- StatusEntry
- TransferErrorKind
- TransferOperation

## God Nodes (most connected - your core abstractions)
1. `init_repo()` - 91 edges
2. `RepoClient` - 90 edges
3. `write_file()` - 88 edges
4. `commit_all()` - 84 edges
5. `AppState` - 74 edges
6. `worker_handle()` - 72 edges
7. `WorkerHandle` - 72 edges
8. `useAppState()` - 70 edges
9. `UseAppStateResult` - 57 edges
10. `init_repo()` - 46 edges

## Surprising Connections (you probably didn't know these)
- `Browsitory conventions (Claude Code variant)` --semantically_similar_to--> `Browsitory conventions (Codex/AGENTS variant)`  [INFERRED] [semantically similar]
  .claude/skills/browsitory-conventions/SKILL.md → .agents/skills/browsitory-conventions/SKILL.md
- `Example Task 1.A.02: Diff Viewer` --references--> `DiffView()`  [EXTRACTED]
  docs/TASK_TEMPLATE.md → frontend/src/components/DiffView.tsx
- `Phase 3 credentials Task 3 report` --references--> `RepoClient`  [EXTRACTED]
  .superpowers/sdd/2026-08-14-phase3-credentials/task-3-report.md → frontend/src/ipc/RepoClient.ts
- `Phase 3 credentials Task 3 report` --references--> `tauriRepoClient`  [EXTRACTED]
  .superpowers/sdd/2026-08-14-phase3-credentials/task-3-report.md → frontend/src/ipc/tauriRepoClient.ts
- `Phase 3 credentials Task 3 report` --references--> `useAppState()`  [EXTRACTED]
  .superpowers/sdd/2026-08-14-phase3-credentials/task-3-report.md → frontend/src/state/useAppState.ts

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Graph Lifecycle** — agents_skills_graphify_references_query_graph_query [EXTRACTED 1.00]
- **Phase 5 Visual System Delivery** — docs_superpowers_specs_2026_08_18_browsitory_phase5_design_visual_design_system, docs_superpowers_plans_2026_08_18_phase5_foundation_design_system_foundation, docs_superpowers_plans_2026_08_18_phase5_rollout_design_system_rollout, docs_superpowers_plans_2026_08_18_phase5_app_icon_original_app_mark [EXTRACTED 1.00]
- **Multi-Repository Evolution** — docs_superpowers_specs_2026_08_20_multi_repo_tabs_design_multi_repo_tab_architecture, docs_superpowers_plans_2026_08_20_multi_repo_tabs_multi_repo_tabs_implementation, docs_superpowers_specs_2026_08_21_multi_repo_workspaces_design_multi_repo_workspace_architecture, docs_superpowers_plans_2026_08_22_multi_repo_workspaces_multi_repo_workspaces_implementation [EXTRACTED 1.00]
- **Repository Management Interface Views** — docs_assets_branches_branch_switcher, docs_assets_remotes_remote_management, docs_assets_tags_tag_management [INFERRED 0.85]
- **graphify SKILL.md pipeline and its reference docs** — claude_skills_graphify_skill_pipeline, claude_skills_graphify_references_github_and_merge_clone, claude_skills_graphify_references_extraction_spec_prompt, claude_skills_graphify_references_query_vocab_expansion, claude_skills_graphify_references_update_incremental, claude_skills_graphify_references_exports_wiki, claude_skills_graphify_references_hooks_post_commit, claude_skills_graphify_references_transcribe_video, claude_skills_graphify_references_add_watch_add_command [EXTRACTED 1.00]
- **Phase 1 git-core-to-IPC dependency pipeline (log/diff/stage+commit feed the Tauri command task)** — docs_tasks_phase_1_a_01_log, docs_tasks_phase_1_a_02_diff, docs_tasks_phase_1_a_03_stage_and_commit, docs_tasks_phase_1_c_01_git_ipc_commands [EXTRACTED 1.00]
- **Phase 2's six subsystems (branch mgmt, stash, blame, commit graph, merge, rebase)** — docs_superpowers_specs_2026_08_12_branch_management_design, docs_superpowers_specs_2026_08_12_stash_design, docs_superpowers_specs_2026_08_13_blame_design, docs_superpowers_specs_2026_08_13_commit_graph_design, docs_superpowers_specs_2026_08_14_merge_design, docs_superpowers_specs_2026_08_15_interactive_rebase_design [EXTRACTED 1.00]
- **Phase 3 remote-workflow plans all extending crates/git-core/src/remote.rs** — docs_superpowers_plans_2026_08_14_phase3_credentials_credentialservice, docs_superpowers_plans_2026_08_14_phase3_pull_pulloutcome, docs_superpowers_plans_2026_08_14_phase3_push_tags_tag_push_functions, docs_superpowers_plans_2026_08_14_phase3_remote_management_remote_module, docs_superpowers_plans_2026_08_14_phase3_transfer_progress_transfer_contract [EXTRACTED 1.00]
- **Phase 4's four independently shippable workstreams** — phase4_worktrees, phase4_submodules, phase4_reflog, phase4_pull_requests [EXTRACTED 1.00]
- **Parallel Claude Code / Codex agent guidance files** — agents_md_overview, claude_skills_browsitory_conventions_skill_rules, agents_skills_browsitory_conventions_skill_rules, claude_skills_graft_skill_tools, agents_skills_graft_skill_tools [INFERRED 0.85]
- **Phase 2 subsystems sharing the Worker/Command/Tauri-command/RepoClient chain** — docs_superpowers_plans_2026_08_12_branch_management_branch_module, docs_superpowers_plans_2026_08_12_stash_stash_module, docs_superpowers_plans_2026_08_13_blame_blame_module, docs_superpowers_plans_2026_08_13_commit_graph_graph_module, docs_superpowers_plans_2026_08_14_merge_merge_module, docs_superpowers_plans_2026_08_15_interactive_rebase_rebase_module [INFERRED 0.85]
- **Phase 3 credential and push/tag delivery reports** — superpowers_sdd_2026_08_14_phase3_credentials_task_3_report_delivered, superpowers_sdd_2026_08_14_phase3_credentials_task_4_report_delivered, superpowers_sdd_2026_08_14_phase3_push_tags_task_3_report_delivered, frontend_src_components_remotepanel_remotepanel [INFERRED 0.85]
- **Phase 4 subsystems following the same git-core -> Worker -> Tauri command -> RepoClient vertical slice pattern** — docs_superpowers_plans_2026_08_15_browsitory_phase4_worktrees_worktree_module, docs_superpowers_plans_2026_08_15_browsitory_phase4_submodules_submodule_module, docs_superpowers_plans_2026_08_15_browsitory_phase4_reflog_reflog_module, docs_superpowers_plans_2026_08_15_browsitory_phase4_pull_requests_forge_module [INFERRED 0.85]

## Communities (179 total, 93 thin omitted)

### Community 0 - "Result"
Cohesion: 0.05
Nodes (94): A, Arc, discard_hunk(), Error, Repository, Result, stage_file(), stage_hunk() (+86 more)

### Community 1 - "commands.rs"
Cohesion: 0.05
Nodes (134): AppHandle, abort_merge(), abort_rebase(), add_remote(), apply_stash(), AppState, BlameLineDto, BranchInfoDto (+126 more)

### Community 2 - "pull requests.rs"
Cohesion: 0.07
Nodes (77): Client, a_401_response_becomes_a_secret_free_unauthorized_error(), a_bitbucket_pull_request_missing_display_name_falls_back_to_an_unknown_author(), a_bitbucket_validation_response_surfaces_the_nested_error_message(), a_github_link_header_without_rel_next_is_not_truncated(), a_github_pull_request_with_a_null_user_falls_back_to_an_unknown_author(), a_github_validation_response_surfaces_the_providers_message_without_the_token(), a_timeout_from_the_transport_becomes_a_secret_free_timeout_error() (+69 more)

### Community 3 - "src/remote.rs"
Cohesion: 0.07
Nodes (81): add_remote(), clear_current_upstream(), clear_remote_auth_profile(), contains_embedded_credentials(), create_tag(), CredentialProvider, current_local_branch_name(), current_upstream() (+73 more)

### Community 4 - "credentials.rs"
Cohesion: 0.07
Nodes (52): a_saved_https_credential_is_never_returned_by_the_forge_token_lookup(), CredentialKey, CredentialService, CredentialService<S>, CredentialStore, CredentialStoreError, derives_a_key_without_the_default_https_port(), derives_an_ipv6_key_without_the_default_https_port() (+44 more)

### Community 5 - "useAppState()"
Cohesion: 0.05
Nodes (4): buildCommands(), goToSidebarSection(), useAppState(), UseAppStateResult

### Community 7 - "src/submodule.rs"
Cohesion: 0.14
Nodes (46): collect_gitlinks(), download_target_from_origin(), download_target_from_url(), ensure_nested_submodules_initialized_at(), ensure_recursive_update_is_safe(), find_submodule(), init_submodule(), list_submodules() (+38 more)

### Community 8 - "Browsitory Phase 1 Design"
Cohesion: 0.06
Nodes (42): Browsitory Phase 0 Setup Plan, git_core::repo::open, RepoClient IPC Interface, git_core::status::status, Worker (per-repo thread), Worker Thread Message-Passing Rationale, git_core::branch module, BranchSwitcher component (+34 more)

### Community 9 - "RepoClient.ts"
Cohesion: 0.12
Nodes (28): ForgeRepositorySectionProps, PullRequestPanel(), bitbucketRepo, githubRepo, openPullRequest, branches, linkedWorktree, mainWorktree (+20 more)

### Community 10 - "src/worktree.rs"
Cohesion: 0.13
Nodes (35): BranchError, BranchInfo, create_branch(), delete_branch(), list_branches(), rename_branch(), resolve_start_point(), Commit (+27 more)

### Community 11 - "add remote()"
Cohesion: 0.12
Nodes (35): classify_host_and_path(), classify_remote_url(), classify_scheme_url(), ClassifyError, detect_forge_repositories(), ForgeError, ForgeIdentity, ForgeProvider (+27 more)

### Community 12 - "commit all()"
Cohesion: 0.15
Nodes (35): commit_all(), a_clean_multi_pick_rebase_lands_every_commit_and_finishes(), a_conflicting_pick_pauses_and_resolving_then_continuing_lands_it(), a_drop_between_a_squash_groups_leader_and_member_does_not_panic(), a_mixed_squash_and_fixup_group_still_collapses_to_one_commit(), a_squash_group_without_an_explicit_combined_message_falls_back_to_the_leaders_message(), abort_rebase_after_a_conflict_also_recovers_cleanly(), abort_rebase_restores_the_original_branch_and_tip_exactly() (+27 more)

### Community 13 - "useAppState.test.ts"
Cohesion: 0.08
Nodes (25): TransferErrorKind::MissingCredential, forge-fixture-override Cargo feature, RemotePanel(), backup, IsOptional, origin, upstream, TagPanel() (+17 more)

### Community 14 - "write file()"
Cohesion: 0.10
Nodes (26): blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point(), blame_file_attributes_all_lines_to_the_single_commit(), blame_file_on_a_missing_path_returns_an_error(), blame_file_reattributes_only_the_changed_lines_after_an_edit(), write_file(), commit_diff_on_the_first_commit_shows_every_line_as_added(), commit_diff_shows_the_change_introduced_by_that_commit(), commit_files_lists_every_changed_path() (+18 more)

### Community 15 - "config/src/lib.rs"
Cohesion: 0.22
Nodes (27): Config, add_recent_repo(), add_recent_repo_at(), config_file_path(), ConfigError, ConfigFile, list_open_repos(), list_open_repos_at() (+19 more)

### Community 16 - "src/rebase.rs"
Cohesion: 0.25
Nodes (26): abort_rebase(), advance(), commits_since(), conflicted_paths(), ends_a_group(), finish(), land_current_step(), next_non_drop_action() (+18 more)

### Community 17 - "init repo()"
Cohesion: 0.12
Nodes (23): create_branch_from_a_specific_commit_uses_that_commit_as_start_point(), create_branch_from_head_adds_and_switches_to_it(), delete_branch_fails_when_deleting_the_current_branch_even_with_force(), delete_branch_with_force_deletes_an_unmerged_branch(), delete_branch_without_force_fails_on_an_unmerged_branch(), delete_branch_without_force_succeeds_when_fully_merged(), list_branches_reports_the_current_branch(), rename_branch_updates_head_when_renaming_the_current_branch() (+15 more)

### Community 18 - "Merge (with Conflict Resolution)"
Cohesion: 0.10
Nodes (27): Branch Management Design, Stash Design, Blame Design, Commit Graph Design, Merge (with Conflict Resolution) Design, Interactive Rebase Design, abort_merge function, BranchSwitcher 'merge into current branch' action (+19 more)

### Community 19 - "devDependencies"
Cohesion: 0.07
Nodes (27): @eslint/js, eslint-plugin-react-hooks, eslint-plugin-react-refresh, devDependencies, @eslint/js, eslint-plugin-react-hooks, eslint-plugin-react-refresh, globals (+19 more)

### Community 20 - "tests/merge.rs"
Cohesion: 0.12
Nodes (25): a_commit_made_after_resolving_a_conflict_has_two_parents(), abort_merge_errors_and_leaves_uncommitted_work_untouched_when_no_merge_is_in_progress(), abort_merge_restores_the_pre_merge_working_tree_and_clears_conflicts(), commit_after_a_fast_forward_has_a_single_parent_as_before(), conflict_hunks_errors_for_a_path_with_no_conflict(), conflict_hunks_returns_clean_and_conflict_segments_for_a_conflicted_file(), conflict_hunks_round_trip_preserves_the_original_files_trailing_newline(), make_conflicted_repo() (+17 more)

### Community 21 - "Toolbar.tsx"
Cohesion: 0.14
Nodes (14): CommitBox(), ConflictResolutionPane(), Resolution, fakeClient(), segments, unused(), Panel(), Toolbar() (+6 more)

### Community 22 - "compilerOptions"
Cohesion: 0.08
Nodes (23): compilerOptions, allowArbitraryExtensions, allowImportingTsExtensions, erasableSyntaxOnly, jsx, lib, module, moduleDetection (+15 more)

### Community 23 - "CommitGraph.tsx"
Cohesion: 0.16
Nodes (13): CommitGraph(), rowsEqual(), commits, status, CommitLaneGraphic(), laneCenterX(), laneColor(), LaneBraid() (+5 more)

### Community 24 - "src/merge.rs"
Cohesion: 0.26
Nodes (21): abort_merge(), conflict_hunks(), conflict_path(), ConflictSegment, FileConflictChoice, find_conflict(), is_merging(), merge_message() (+13 more)

### Community 25 - "devDependencies"
Cohesion: 0.09
Nodes (22): devDependencies, tsx, @types/mocha, typescript, @wdio/cli, @wdio/globals, @wdio/local-runner, @wdio/mocha-framework (+14 more)

### Community 26 - "Phase 3 Remote Workflows"
Cohesion: 0.11
Nodes (21): git-core module shape convention (thiserror enum + &Repository fns + real temp-dir tests), RepoClient extension pattern: typed union methods, tauriRepoClient sole @tauri-apps/api importer, Phase 3 Remote Workflows Design, commit() merge-aware parent selection, start_merge function, OS-keychain-backed HTTPS credential service, keyring 4.1.6 crate dependency, Five independently shippable Phase 3 plans (+13 more)

### Community 27 - "Phase 1: full repo"
Cohesion: 0.19
Nodes (20): RepoClient interface extension, runMutation try/refresh/catch helper, useAppState hook, Browsitory Phase 1 Design, Task 1.D.01: Extend RepoClient, Task 1.D.02: useAppState hook, Task 1.E.01: RepoPicker component, Task 1.E.03: HistoryList component (+12 more)

### Community 28 - "App.tsx"
Cohesion: 0.22
Nodes (10): App(), Overlay(), Sidebar(), formatBytes(), TransferPanel(), applyTheme(), loadStoredTheme(), persistTheme() (+2 more)

### Community 29 - "BranchSwitcher.tsx"
Cohesion: 0.16
Nodes (9): BranchSwitcher(), branches, AccordionSection(), loadOpen(), ListRow(), ReflogPanel(), entry, ReflogEntry (+1 more)

### Community 30 - "compilerOptions"
Cohesion: 0.10
Nodes (19): compilerOptions, allowImportingTsExtensions, erasableSyntaxOnly, lib, module, moduleDetection, noEmit, noFallthroughCasesInSwitch (+11 more)

### Community 31 - "wdio.conf.ts"
Cohesion: 0.11
Nodes (11): config, CREDENTIAL_CERT_DIR, CREDENTIAL_CERT_PATH, CREDENTIAL_KEY_PATH, __dirname, E2E_CONFIG_DIR, E2E_PARENT_SOURCE_PATH, E2E_REPO_PATH (+3 more)

### Community 32 - "DiffPane.tsx"
Cohesion: 0.19
Nodes (11): BlameView(), lines, DiffPane(), fakeClient(), unused(), DiffView(), originPrefix(), BlameLine (+3 more)

### Community 33 - "src/diff.rs"
Cohesion: 0.29
Nodes (17): commit_and_parent_trees(), commit_diff(), commit_files(), DiffError, DiffHunk, DiffLine, DiffLineOrigin, hunks_from_diff() (+9 more)

### Community 34 - "commands.ts"
Cohesion: 0.23
Nodes (12): CommandPalette(), handleKeyDown(), runCommand(), Command, filterAndSortCommands(), loadRecentCommandIds(), recordCommandUsed(), scoreCommand() (+4 more)

### Community 35 - "RebasePlanner.tsx"
Cohesion: 0.19
Nodes (16): ActionKind, defaultCombinedMessage(), groupFingerprints(), isGroupMember(), isLeader(), nextNonDropActionKind(), RebasePlanner(), recomputeGroupLeaders() (+8 more)

### Community 36 - "useOpenRepos.ts"
Cohesion: 0.16
Nodes (7): RepoTabs(), noneBusy, repos, displayNameFor(), OpenRepo, useOpenRepos(), UseOpenReposResult

### Community 37 - "Task 1.C.01: Worker +"
Cohesion: 0.19
Nodes (16): CommitInfo struct, log() function (git-core::log), commit_diff function, commit_files function, RefCell shared-borrow pattern for Diff::foreach callbacks, working_diff function, commit() function (git-core::commit), stage_file function (+8 more)

### Community 38 - "graphify SKILL.md main pipeline"
Cohesion: 0.15
Nodes (16): .claude/CLAUDE.md graphify trigger, /graphify add URL flow, --watch folder watcher, --neo4j / --neo4j-push export, --wiki export, Extraction subagent prompt spec, GitHub repo clone flow, post-commit auto-rebuild hook (+8 more)

### Community 39 - "src/reflog.rs"
Cohesion: 0.31
Nodes (15): is_local_reference(), list_reflog_refs(), read_reflog(), ReflogEntry, ReflogError, restore_reflog_entry(), Error, Option (+7 more)

### Community 40 - "sidebar.ts"
Cohesion: 0.17
Nodes (8): E2E_REPO_PATH, E2E_REPO_PATH, E2E_REPO_PATH, BARE_REMOTE_PATH, E2E_REPO_PATH, E2E_REPO_PATH, E2E_REPO_PATH, expandSidebarSection()

### Community 41 - "pull-requests.spec.ts"
Cohesion: 0.16
Nodes (5): BITBUCKET_CREATE_FIXTURE, BITBUCKET_LIST_FIXTURE, GITHUB_CREATE_FIXTURE, GITHUB_LIST_FIXTURE, ForgeFixtureClient

### Community 42 - "forgeFixtureServer.ts"
Cohesion: 0.20
Nodes (9): CapturedForgeRequest, classifyProviderRoute(), closeSharedForgeFixtureServer(), defaultResponses, ForgeFixtureServer, readJsonBody(), RouteKey, RouteResponse (+1 more)

### Community 43 - "status()"
Cohesion: 0.27
Nodes (12): Error, Option, Repository, Result, String, Vec, staged_kind(), status() (+4 more)

### Community 44 - "src/stash.rs"
Cohesion: 0.35
Nodes (11): apply_stash(), drop_stash(), list_stashes(), Error, Repository, Result, String, Vec (+3 more)

### Community 45 - "frontend/package.json"
Cohesion: 0.17
Nodes (11): name, packageManager, private, scripts, build, dev, lint, preview (+3 more)

### Community 46 - "graft-hooks.cjs"
Cohesion: 0.27
Nodes (10): best(), entry(), { execFileSync }, fromPkg(), fs, globalRoot(), newer(), path (+2 more)

### Community 47 - "graft-statusline.cjs"
Cohesion: 0.27
Nodes (10): best(), entry(), { execFileSync }, fromPkg(), fs, globalRoot(), newer(), path (+2 more)

### Community 48 - "blame file()"
Cohesion: 0.31
Nodes (10): blame_file(), BlameError, BlameLine, resolve_commit_id(), Error, Oid, Repository, Result (+2 more)

### Community 49 - "compilerOptions"
Cohesion: 0.18
Nodes (10): compilerOptions, module, moduleResolution, skipLibCheck, strict, target, types, node (+2 more)

### Community 50 - "SplitView()"
Cohesion: 0.31
Nodes (8): loadWidth(), SplitView(), clamp(), handleDoubleClick(), handleKeyDown(), handlePointerMove(), handlePointerUp(), persist()

### Community 51 - "reflog missing log.rs"
Cohesion: 0.44
Nodes (9): create_orphan_commit(), init_bare_repo(), reading_or_restoring_a_local_ref_without_a_reflog_does_not_create_one(), remove_reflog_file(), restoring_an_unborn_head_to_an_orphan_commit_does_not_create_a_ref_or_reflog(), Oid, Repository, TempDir (+1 more)

### Community 52 - "default.json"
Cohesion: 0.20
Nodes (9): description, identifier, permissions, $schema, windows, core:default, dialog:default, main (+1 more)

### Community 53 - "git core::remote module (remotes"
Cohesion: 0.22
Nodes (10): CredentialService / CredentialStore, Credential Handling Implementation Plan, Pull Merge and Rebase Implementation Plan, PullOutcome / pull_after_fetch, Push and Tag Management Implementation Plan, list_tags/create_tag/delete_tag/push_current_branch/push_tags, Remote and Upstream Management Implementation Plan, git_core::remote module (remotes + upstream) (+2 more)

### Community 54 - "ConfigSearchPathOverride"
Cohesion: 0.22
Nodes (8): ConfigLevel, ConfigSearchPathOverride, Path, Self, Vec, CString, Drop, MutexGuard

### Community 55 - "graph log()"
Cohesion: 0.33
Nodes (8): graph_log(), GraphCommit, GraphError, Error, Repository, Result, String, Vec

### Community 56 - "dependencies"
Cohesion: 0.22
Nodes (9): dependencies, lucide-react, react, react-dom, @tauri-apps/api, lucide-react, react, react-dom (+1 more)

### Community 57 - "Visual Design System"
Cohesion: 0.29
Nodes (8): License Compliance Policy, Design System Foundation, Design System Rollout, Visual Design System, frontend/index.html Vite entry point, frontend/README.md Vite template notes, frontend/src/main.tsx entry script, CI frontend job

### Community 58 - "Browsitory Phase 4 Design"
Cohesion: 0.32
Nodes (8): Browsitory Phase 4 Design, Provider-neutral forge HTTP client service, PullRequest DTO, Pull-request integration feature (GitHub/Bitbucket), Reflog viewer and recovery feature, Existing RepoClient->Worker->git-core boundary unchanged for Phase 4, Submodules feature, Worktrees feature

### Community 59 - "e2e/package.json"
Cohesion: 0.25
Nodes (7): name, packageManager, private, scripts, test, typecheck, type

### Community 60 - "Task File Template Format"
Cohesion: 0.38
Nodes (7): AGENTS.md project guidance, Browsitory conventions (Codex/AGENTS variant), Browsitory conventions (Claude Code variant), subagent-driven-development workstream grouping, git_core::diff::file_diff, Example Task 1.A.02: Diff Viewer, Task File Template Format

### Community 61 - "commit()"
Cohesion: 0.38
Nodes (6): commit(), CommitError, Error, Repository, Result, String

### Community 62 - "open()"
Cohesion: 0.43
Nodes (6): open(), RepoError, Error, Path, Repository, Result

### Community 63 - "tests/graph.rs"
Cohesion: 0.29
Nodes (6): graph_log_reports_branch_refs_only_for_tip_commits(), graph_log_reports_empty_branch_refs_for_a_non_tip_commit(), graph_log_reports_multiple_parent_ids_for_a_merge_commit(), graph_log_respects_the_limit(), graph_log_returns_an_empty_vec_for_a_repository_with_no_commits(), graph_log_shows_commits_from_every_local_branch()

### Community 64 - "Repository Overview Interface"
Cohesion: 0.29
Nodes (7): Branch Switcher Interface, Command Palette, Commit Diff View, Repository Overview Interface, Remote Management Interface, Staging Workflow Interface, Tag Management Interface

### Community 65 - "Task 1.E.04: DiffPane and"
Cohesion: 0.43
Nodes (7): Task 1.E.02: DiffView component, Task 1.E.04: DiffPane and CommitBox components, DiffView component, CommitBox component, DiffPane component, DiffPane branches uncommitted (staging controls) vs commit (read-only), CommitBox initialMessage prop

### Community 66 - "SubmodulePanel.tsx"
Cohesion: 0.38
Nodes (4): SubmodulePanel(), initializedSubmodule, uninitializedSubmodule, SubmoduleInfo

### Community 67 - "Task 1.C.02: Tauri commands"
Cohesion: 0.47
Nodes (6): list_recent_repos / add_recent_repo, pub _at functions split for tempdir testability, open_repo best-effort recent-repo recording, pick_repo_folder command, Task 1.B.01: config recent-repos registry, Task 1.C.02: Tauri commands for repo picking + recent repos

### Community 68 - "prunes stale worktree metadata"
Cohesion: 0.33
Nodes (5): creates_a_linked_worktree_for_an_existing_local_branch(), creates_a_missing_branch_at_the_requested_start_point(), prunes_stale_worktree_metadata_idempotently(), refuses_to_remove_a_dirty_linked_worktree_without_deleting_it(), rejects_an_existing_worktree_destination_without_removing_it()

### Community 69 - "Browsitory Source App Icon"
Cohesion: 0.40
Nodes (5): Browsitory High Resolution App Icon, Browsitory App Icon, Browsitory Small App Icon, Browsitory Favicon, Browsitory Source App Icon

### Community 70 - "remote-transfer.spec.ts"
Cohesion: 0.40
Nodes (3): BARE_REMOTE_PATH, E2E_REPO_PATH, REMOTE_SOURCE_PATH

### Community 71 - "screenshots.docs.ts"
Cohesion: 0.40
Nodes (3): __dirname, E2E_REPO_PATH, SCREENSHOT_DIR

### Community 72 - "worktree.spec.ts"
Cohesion: 0.50
Nodes (4): activeBranchSwitcher(), activeElement(), E2E_REPO_PATH, WORKTREE_PATH

### Community 73 - "RepoClient IPC Boundary"
Cohesion: 0.50
Nodes (4): RepoClient IPC Boundary, Repository Worker Thread, Tauri Web Frontend, Browsitory

### Community 74 - "Multi-Repo Tabs Implementation"
Cohesion: 0.67
Nodes (4): Multi-Repo Tabs Implementation, Multi-Repo Workspaces Implementation, Multi-Repo Tab Architecture, Multi-Repo Workspace Architecture

### Community 76 - "graft tools reference (Codex"
Cohesion: 0.67
Nodes (3): Graft repo-context-graph integration block, graft tools reference (Codex variant), graft tools reference (Claude Code variant)

### Community 77 - "Rust Token Killer"
Cohesion: 0.67
Nodes (3): RTK Instruction, RTK Token Optimized Commands, Rust Token Killer

### Community 78 - "Layout Primitives"
Cohesion: 1.00
Nodes (3): Layout Primitives, Layout Rollout, Layout Information Architecture

### Community 79 - "@types/node"
Cohesion: 0.67
Nodes (3): @types/node, @types/node, @types/node

### Community 82 - "tauri-app"
Cohesion: 0.67
Nodes (3): config, git-core, tauri-app

## Knowledge Gaps
- **285 isolated node(s):** `CapturedForgeRequest`, `RouteKey`, `RouteResponse`, `name`, `packageManager` (+280 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **93 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `write_file()` connect `write file()` to `prunes stale worktree metadata`, `src/submodule.rs`, `commit all()`, `init repo()`, `tests/merge.rs`, `tests/graph.rs`?**
  _High betweenness centrality (0.047) - this node is a cross-community bridge._
- **Why does `configured_submodule_checkout()` connect `src/submodule.rs` to `Result`, `commit all()`, `write file()`?**
  _High betweenness centrality (0.046) - this node is a cross-community bridge._
- **Why does `recursive_update_preserves_the_selected_head_when_a_nested_submodule_is_uninitialized()` connect `src/submodule.rs` to `Result`, `commit all()`, `write file()`?**
  _High betweenness centrality (0.044) - this node is a cross-community bridge._
- **Are the 88 inferred relationships involving `init_repo()` (e.g. with `blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point()` and `blame_file_attributes_all_lines_to_the_single_commit()`) actually correct?**
  _`init_repo()` has 88 INFERRED edges - model-reasoned connections that need verification._
- **Are the 86 inferred relationships involving `write_file()` (e.g. with `blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point()` and `blame_file_attributes_all_lines_to_the_single_commit()`) actually correct?**
  _`write_file()` has 86 INFERRED edges - model-reasoned connections that need verification._
- **Are the 82 inferred relationships involving `commit_all()` (e.g. with `blame_file_at_a_specific_historic_commit_only_sees_lines_up_to_that_point()` and `blame_file_attributes_all_lines_to_the_single_commit()`) actually correct?**
  _`commit_all()` has 82 INFERRED edges - model-reasoned connections that need verification._
- **What connects `CapturedForgeRequest`, `RouteKey`, `RouteResponse` to the rest of the system?**
  _285 weakly-connected nodes found - possible documentation gaps or missing edges._