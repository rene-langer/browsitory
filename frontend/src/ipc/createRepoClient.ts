import type {
  AmendOptions,
  AppearanceSettings,
  CommitChangeRequest,
  HunkOptions,
  PatchContent,
  PatchSource,
  CommitMessageSettings,
  FetchSettings,
  UpdateSettings,
  FileSides,
  FileContent,
  TreeListing,
  SignatureInput,
  Trailer,
  DiffSource,
  ExternalToolSettings,
  GitTools,
  MarkResolvedOutcome,
  MergeToolOutcome,
  CommitOutcome,
  CommitSignature,
  BlameLine,
  BranchInfo,
  ConflictSegment,
  CreatePullRequest,
  DiffHunk,
  DiffOptions,
  DiffSettings,
  FileConflictChoice,
  FileHistoryEntry,
  LineHistory,
  ChangedFile,
  CommitLocation,
  CommitStatsEntry,
  CommitDetails,
  GraphSelection,
  CompareRange,
  SearchPage,
  ForgeProvider,
  ForgeRepository,
  GraphCommit,
  MergeOptions,
  MergeOutcome,
  OpenRepoEntry,
  PullOutcome,
  PullStrategy,
  StashSwitchOutcome,
  PullRequest,
  PullRequestList,
  RebasePlanCommit,
  HistoryEditTarget,
  RebasePlanEntry,
  RebaseStepResult,
  ReflogEntry,
  RemoteAuthMode,
  RepoAlias,
  RemoteInfo,
  RepoClient,
  StashApplyOutcome,
  StashEntry,
  StashFiles,
  StashOptions,
  StatusEntry,
  SubmoduleInfo,
  TagInfo,
  UpstreamInfo,
  Workspace,
  WorktreeInfo,
  HeadInfo,
  PickOutcome,
  SequenceAbort,
  ConfirmedAbort,
  RebaseAbort,
  SequenceOutcome,
  ConflictSide,
  UndoOutcome,
  UndoState,
  ResetMode,
  RootFileName,
} from "./RepoClient";
import { validateRemoteUrls } from "./validateRemoteUrls";

/**
 * Sends one backend call: a Tauri command on the desktop, a JSON-RPC request to the extension
 * host (and through it the sidecar) in VSCode. `params` is left out for a method without
 * arguments; each transport decides what it puts on the wire for that.
 */
export type BackendCall = <T>(method: string, params?: Record<string, unknown>) => Promise<T>;

/**
 * The `RepoClient` members each transport implements itself, because they really differ:
 *
 * - clone and init: the desktop app clones and initializes; the extension leaves that to VSCode's
 *   own commands (`supportsCloneAndInit: false`).
 * - `listOpenRepos`: the Tauri command answers a `[entries, activePath]` tuple, the sidecar an
 *   object.
 * - the two progress subscriptions: Tauri events on the desktop, JSON-RPC notifications in VSCode.
 * - file drops (#649): the desktop webview's drag and drop events and `readPatchFile`; none in
 *   VSCode, which handles drops on its windows itself.
 * - `logFrontendError`: the desktop app writes through its log plugin and has no command for it.
 */
export type TransportSpecific =
  | "supportsCloneAndInit"
  | "cloneRepo"
  | "cancelClone"
  | "initRepo"
  | "listOpenRepos"
  | "subscribeTransferProgress"
  | "subscribeHookProgress"
  | "subscribeFileDrops"
  | "readPatchFile"
  | "logFrontendError";

export type SharedRepoClient = Omit<RepoClient, TransportSpecific>;

/**
 * Every `RepoClient` method both transports send the same way (#504): the same backend name with
 * the same params. A transport spreads this and adds the `TransportSpecific` members. Anything
 * a transport does around a call (logging, waiting for its event listeners, error mapping)
 * belongs in its `call`, not in an override here, so the params can't drift apart again.
 */
export function createRepoClient(call: BackendCall): SharedRepoClient {
  return {
    openRepo: (path: string) => call<void>("open_repo", { path }),
    closeRepo: (repoPath: string) => call<void>("close_repo", { repoPath }),
    getStatus: (repoPath: string) => call<StatusEntry[]>("get_status", { repoPath }),
    getCommitGraph: (repoPath: string, limit: number, selection: GraphSelection) =>
      call<GraphCommit[]>("get_commit_graph", { repoPath, limit, selection }),
    locateCommit: (repoPath: string, revision: string, selection: GraphSelection) =>
      call<CommitLocation | null>("locate_commit", { repoPath, revision, selection }),
    getMergedCommits: (repoPath: string, mergeId: string, limit: number) =>
      call<GraphCommit[]>("get_merged_commits", { repoPath, mergeId, limit }),
    getGraphCollapseMerges: (repoPath: string) => call<boolean>("get_graph_collapse_merges", { repoPath }),
    setGraphCollapseMerges: (repoPath: string, collapse: boolean) =>
      call<void>("set_graph_collapse_merges", { repoPath, collapse }),
    getWorkingDiff: (repoPath: string, path: string, staged: boolean, opts: DiffOptions, oldPath: string | null = null) =>
      call<DiffHunk[]>("get_working_diff", { repoPath, path, staged, opts, oldPath }),
    getCommitDiff: (repoPath: string, commitId: string, path: string, opts: DiffOptions) =>
      call<DiffHunk[]>("get_commit_diff", { repoPath, commitId, path, opts }),
    getFileHistory: (repoPath: string, path: string, limit: number) =>
      call<FileHistoryEntry[]>("get_file_history", { repoPath, path, limit }),
    getLineHistory: (repoPath: string, revision: string, path: string, startLine: number, endLine: number, limit: number) =>
      call<LineHistory>("get_line_history", { repoPath, revision, path, startLine, endLine, limit }),
    searchCommits: (repoPath: string, query: string, selection: GraphSelection, cursor: string | null) =>
      call<SearchPage>("search_commits", { repoPath, query, selection, cursor }),
    cancelSearch: (repoPath: string) => call<void>("cancel_search", { repoPath }),
    getCommitRenameDiff: (repoPath: string, commitId: string, oldPath: string, newPath: string, opts: DiffOptions) =>
      call<DiffHunk[]>("get_commit_rename_diff", { repoPath, commitId, oldPath, newPath, opts }),
  
    pickRepoFolder: () => call<string | null>("pick_repo_folder"),
    listRecentRepos: () => call<string[]>("list_recent_repos"),
    getAppVersion: () => call<string>("get_app_version"),
    getLastSeenVersion: () => call<string | null>("get_last_seen_version"),
    setLastSeenVersion: (version: string) =>
      call<void>("set_last_seen_version", { version }),
    persistOpenRepos: (entries: OpenRepoEntry[], activePath: string | null) =>
      call<void>("persist_open_repos", { entries, activePath }),
    listRepoAliases: () => call<RepoAlias[]>("list_repo_aliases"),
    setRepoAlias: (repoPath: string, alias: string | null) => call<void>("set_repo_alias", { repoPath, alias }),
    scanReposInRoot: (root: string) => call<string[]>("scan_repos_in_root", { root }),
    listWorkspaces: () => call<Workspace[]>("list_workspaces"),
    saveWorkspace: (name: string, root: string, members: string[]) =>
      call<string>("save_workspace", { name, root, members }),
    updateWorkspace: (id: string, name: string, members: string[]) =>
      call<void>("update_workspace", { id, name, members }),
    deleteWorkspace: (id: string) => call<void>("delete_workspace", { id }),
    getGraphBranchSelection: (repoPath: string) =>
      call<string[] | null>("get_graph_branch_selection", { repoPath }),
    setGraphBranchSelection: (repoPath: string, local: string[]) =>
      call<void>("set_graph_branch_selection", { repoPath, local }),
    clearGraphBranchSelection: (repoPath: string) => call<void>("clear_graph_branch_selection", { repoPath }),
    getGraphRemoteBranchSelection: (repoPath: string) =>
      call<string[] | null>("get_graph_remote_branch_selection", { repoPath }),
    setGraphRemoteBranchSelection: (repoPath: string, remote: string[]) =>
      call<void>("set_graph_remote_branch_selection", { repoPath, remote }),
    clearGraphRemoteBranchSelection: (repoPath: string) =>
      call<void>("clear_graph_remote_branch_selection", { repoPath }),
    getCommitFiles: (repoPath: string, commitId: string) =>
      call<string[]>("get_commit_files", { repoPath, commitId }),
    getCommitDetails: (repoPath: string, commitId: string) =>
      call<CommitDetails>("get_commit_details", { repoPath, commitId }),
    getCommitStats: (repoPath: string, commitIds: string[], withFiles: boolean) =>
      call<CommitStatsEntry[]>("get_commit_stats", { repoPath, commitIds, withFiles }),
    getRangeFiles: (repoPath: string, fromId: string, toId: string) =>
      call<ChangedFile[]>("get_range_files", { repoPath, fromId, toId }),
    getRangeDiff: (repoPath: string, fromId: string, toId: string, path: string, oldPath: string | null, opts: DiffOptions) =>
      call<DiffHunk[]>("get_range_diff", { repoPath, fromId, toId, path, oldPath, opts }),
    getWorkingTreeCompareFiles: (repoPath: string, commitId: string) =>
      call<ChangedFile[]>("get_working_tree_compare_files", { repoPath, commitId }),
    getWorkingTreeCompareDiff: (repoPath: string, commitId: string, path: string, oldPath: string | null, opts: DiffOptions) =>
      call<DiffHunk[]>("get_working_tree_compare_diff", { repoPath, commitId, path, oldPath, opts }),
    getBranchCompareRange: (repoPath: string, refName: string) =>
      call<CompareRange>("get_branch_compare_range", { repoPath, refName }),
    getCommitMessage: (repoPath: string, commitId: string) =>
      call<string>("get_commit_message", { repoPath, commitId }),
    getCommitSignature: (repoPath: string, commitId: string) =>
      call<CommitSignature>("get_commit_signature", { repoPath, commitId }),
    stageFile: (repoPath: string, path: string) => call<void>("stage_file", { repoPath, path }),
    unstageFile: (repoPath: string, path: string) => call<void>("unstage_file", { repoPath, path }),
    stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions) =>
      call<void>("stage_hunk", { repoPath, path, oldStart, newStart, ...hunk }),
    unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions) =>
      call<void>("unstage_hunk", { repoPath, path, oldStart, newStart, ...hunk }),
    discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions) =>
      call<void>("discard_hunk", { repoPath, path, oldStart, newStart, ...hunk }),
    stageLines: (repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) =>
      call<void>("stage_lines", { repoPath, path, oldStart, newStart, ...hunk, lineIndices, expectedLines }),
    unstageLines: (repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) =>
      call<void>("unstage_lines", { repoPath, path, oldStart, newStart, ...hunk, lineIndices, expectedLines }),
    discardLines: (repoPath: string, path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) =>
      call<void>("discard_lines", { repoPath, path, oldStart, newStart, ...hunk, lineIndices, expectedLines }),
    discardFile: (repoPath: string, path: string) => call<void>("discard_file", { repoPath, path }),
    discardAllUnstaged: (repoPath: string, paths: string[]) => call<void>("discard_all_unstaged", { repoPath, paths }),
    applyCommitChange: (repoPath: string, change: CommitChangeRequest) =>
      call<void>("apply_commit_change", { repoPath, change }),
    getPatch: (repoPath: string, source: PatchSource) => call<PatchContent>("get_patch", { repoPath, source }),
    applyPatch: (repoPath: string, content: PatchContent) => call<void>("apply_patch", { repoPath, content }),
    savePatchFile: (suggestedName: string, content: PatchContent) =>
      call<string | null>("save_patch_file", { suggestedName, content }),
    openPatchFile: () => call<PatchContent | null>("open_patch_file"),
    ignorePath: (repoPath: string, pattern: string) => call<void>("ignore_path", { repoPath, pattern }),
    getDiffSettings: () => call<DiffSettings>("get_diff_settings"),
    setDiffSettings: (settings: DiffSettings) => call<void>("set_diff_settings", { settings }),
    commit: (repoPath: string, message: string, noVerify: boolean, trailers: Trailer[] = []) =>
      call<CommitOutcome>("commit", { repoPath, message, noVerify, trailers }),
    listBranches: (repoPath: string) => call<BranchInfo[]>("list_branches", { repoPath }),
    createBranch: (repoPath: string, name: string, startPoint: string, checkout: boolean) =>
      call<void>("create_branch", { repoPath, name, startPoint, checkout }),
    switchBranch: (repoPath: string, name: string) => call<void>("switch_branch", { repoPath, name }),
    getSwitchConflicts: (repoPath: string, name: string) =>
      call<string[]>("get_switch_conflicts", { repoPath, name }),
    stashAndSwitch: (repoPath: string, name: string, reapply: boolean) =>
      call<StashSwitchOutcome>("stash_and_switch", { repoPath, name, reapply }),
    setIdentity: (repoPath: string, name: string, email: string, global: boolean) =>
      call<void>("set_identity", { repoPath, name, email, global }),
    deleteBranch: (repoPath: string, name: string, force: boolean) =>
      call<void>("delete_branch", { repoPath, name, force }),
    renameBranch: (repoPath: string, oldName: string, newName: string) =>
      call<void>("rename_branch", { repoPath, oldName, newName }),
    listWorktrees: (repoPath: string) => call<WorktreeInfo[]>("list_worktrees", { repoPath }),
    createWorktree: (
      repoPath: string,
      name: string,
      path: string,
      branch: string,
      startPoint: string | null,
    ) => call<void>("create_worktree", { repoPath, name, path, branch, startPoint }),
    removeWorktree: (repoPath: string, name: string) => call<void>("remove_worktree", { repoPath, name }),
    pruneWorktrees: (repoPath: string) => call<void>("prune_worktrees", { repoPath }),
    listSubmodules: (repoPath: string) => call<SubmoduleInfo[]>("list_submodules", { repoPath }),
    initSubmodule: (repoPath: string, path: string) => call<void>("init_submodule", { repoPath, path }),
    updateSubmodule: (repoPath: string, path: string, recursive: boolean) =>
      call<void>("update_submodule", { repoPath, path, recursive }),
    listReflogRefs: (repoPath: string) => call<string[]>("list_reflog_refs", { repoPath }),
    getReflog: (repoPath: string, reference: string) =>
      call<ReflogEntry[]>("get_reflog", { repoPath, reference }),
    restoreReflogEntry: (repoPath: string, reference: string, newId: string) =>
      call<void>("restore_reflog_entry", { repoPath, reference, newId }),
    listRemotes: (repoPath: string) => call<RemoteInfo[]>("list_remotes", { repoPath }),
    listRemoteBranches: (repoPath: string, remoteName: string) =>
      call<string[]>("list_remote_branches", { repoPath, remoteName }),
    getCurrentUpstream: (repoPath: string) =>
      call<UpstreamInfo | null>("get_current_upstream", { repoPath }),
    getRemoteUpstreams: (repoPath: string, name: string) =>
      call<UpstreamInfo[]>("get_remote_upstreams", { repoPath, name }),
    addRemote: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
      validateRemoteUrls(fetchUrl, pushUrl);
      return call<void>("add_remote", { repoPath, name, fetchUrl, pushUrl });
    },
    renameRemote: (repoPath: string, oldName: string, newName: string) =>
      call<void>("rename_remote", { repoPath, oldName, newName }),
    updateRemoteUrls: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
      validateRemoteUrls(fetchUrl, pushUrl);
      return call<void>("update_remote_urls", { repoPath, name, fetchUrl, pushUrl });
    },
    removeRemote: (repoPath: string, name: string, clearUpstreams: boolean) =>
      call<void>("remove_remote", { repoPath, name, clearUpstreams }),
    saveHttpsCredential: (repoPath: string, remoteName: string, username: string, token: string) =>
      call<void>("save_https_credential", { repoPath, remoteName, username, token }),
    forgetHttpsCredential: (repoPath: string, remoteName: string) =>
      call<void>("forget_https_credential", { repoPath, remoteName }),
    setRemoteAuthMode: (repoPath: string, remoteName: string, mode: RemoteAuthMode, username: string | null) =>
      call<void>("set_remote_auth_mode", { repoPath, remoteName, mode, username }),
    setCurrentUpstream: (repoPath: string, remoteName: string, remoteBranch: string) =>
      call<void>("set_current_upstream", { repoPath, remoteName, remoteBranch }),
    clearCurrentUpstream: (repoPath: string) => call<void>("clear_current_upstream", { repoPath }),
    listTags: (repoPath: string) => call<TagInfo[]>("list_tags", { repoPath }),
    createTag: (repoPath: string, name: string, message: string | null, target: string | null) =>
      call<void>("create_tag", { repoPath, name, message, target }),
    deleteTag: (repoPath: string, name: string) => call<void>("delete_tag", { repoPath, name }),
    // Fetch and the pushes resolve with the operation id as soon as the backend has one — the
    // transfer itself is still running, and reports through `subscribeTransferProgress`.
    // `pullCurrentUpstream` is the exception: it settles only once the whole pull is done.
    fetchRemote: (repoPath: string, remoteName: string, prune: boolean | null) =>
      call<string>("fetch_remote", { repoPath, remoteName, prune }),
    fetchAllRemotes: (repoPath: string, prune: boolean | null) =>
      call<string>("fetch_all_remotes", { repoPath, prune }),
    backgroundFetch: (repoPath: string, prune: boolean | null, intervalMinutes: number) =>
      call<string | null>("background_fetch", { repoPath, prune, intervalMinutes }),
    pushBranch: (repoPath: string, remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify: boolean) =>
      call<string>("push_branch", { repoPath, remoteName, localBranch, remoteBranch, setUpstream, noVerify }),
    deleteRemoteBranch: (repoPath: string, remoteName: string, branchName: string, noVerify: boolean) =>
      call<string>("delete_remote_branch", { repoPath, remoteName, branchName, noVerify }),
    deleteRemoteTag: (repoPath: string, remoteName: string, tagName: string, noVerify: boolean) =>
      call<string>("delete_remote_tag", { repoPath, remoteName, tagName, noVerify }),
    pushCurrentBranch: (repoPath: string, remoteName: string, noVerify: boolean) =>
      call<string>("push_current_branch", { repoPath, remoteName, noVerify }),
    pushTags: (repoPath: string, remoteName: string, names: string[], noVerify: boolean) =>
      call<string>("push_tags", { repoPath, remoteName, names, noVerify }),
    pullCurrentUpstream: (repoPath: string) =>
      call<PullOutcome>("pull_current_upstream", { repoPath }),
    getPullStrategy: (repoPath: string) => call<PullStrategy | null>("get_pull_strategy", { repoPath }),
    setPullStrategy: (repoPath: string, strategy: PullStrategy) =>
      call<void>("set_pull_strategy", { repoPath, strategy }),
    // Starts no transfer, so the desktop's `call` doesn't wait for the transfer listeners: the
    // cancellation shows up on the listeners the cancelled operation already set up.
    cancelTransfer: (repoPath: string, operationId: string) =>
      call<void>("cancel_transfer", { repoPath, operationId }),
    listStashes: (repoPath: string) => call<StashEntry[]>("list_stashes", { repoPath }),
    saveStash: (repoPath: string, options: StashOptions) => call<void>("save_stash", { repoPath, options }),
    applyStash: (repoPath: string, index: number, commitId: string, restoreIndex: boolean) =>
      call<StashApplyOutcome>("apply_stash", { repoPath, index, commitId, restoreIndex }),
    popStash: (repoPath: string, index: number, commitId: string, restoreIndex: boolean) =>
      call<StashApplyOutcome>("pop_stash", { repoPath, index, commitId, restoreIndex }),
    getStashFiles: (repoPath: string, commitId: string) =>
      call<StashFiles>("get_stash_files", { repoPath, commitId }),
    dropStash: (repoPath: string, index: number, commitId: string) =>
      call<void>("drop_stash", { repoPath, index, commitId }),
    getBlame: (repoPath: string, commitId: string, path: string) =>
      call<BlameLine[]>("get_blame", { repoPath, commitId, path }),
    mergeBranch: (repoPath: string, branchName: string, options: MergeOptions) =>
      call<MergeOutcome>("start_merge", { repoPath, branchName, options }),
    getMergeOptions: (repoPath: string) => call<MergeOptions>("get_merge_options", { repoPath }),
    setMergeOptions: (repoPath: string, options: MergeOptions) =>
      call<void>("set_merge_options", { repoPath, options }),
    getConflictHunks: (repoPath: string, path: string) =>
      call<ConflictSegment[]>("get_conflict_hunks", { repoPath, path }),
    resolveConflict: (repoPath: string, path: string, resolvedContent: string) =>
      call<void>("resolve_conflict", { repoPath, path, resolvedContent }),
    abortMerge: (repoPath: string) => call<void>("abort_merge", { repoPath }),
    getMergeMessage: (repoPath: string) => call<string | null>("get_merge_message", { repoPath }),
    getHeadInfo: (repoPath: string) => call<HeadInfo>("get_head_info", { repoPath }),
    amendCommit: (repoPath: string, message: string, noVerify: boolean, options?: AmendOptions) =>
      call<CommitOutcome>("amend_commit", {
        repoPath,
        message,
        noVerify,
        author: options?.author ?? null,
        trailers: options?.trailers ?? [],
      }),
    getFileSides: (repoPath: string, path: string, source: DiffSource) =>
      call<FileSides>("get_file_sides", { repoPath, path, source }),
    getCommitTree: (repoPath: string, commitId: string, dir: string, filter: string | null) =>
      call<TreeListing>("get_commit_tree", { repoPath, commitId, dir, filter }),
    getFileAtCommit: (repoPath: string, commitId: string, path: string) =>
      call<FileContent>("get_file_at_commit", { repoPath, commitId, path }),
    // In VSCode a native method: the extension host shows VSCode's save dialog, then has the
    // sidecar write (`savePatchFile` and `openPatchFile` show their dialogs there too).
    saveFileVersion: (repoPath: string, commitId: string, path: string) =>
      call<string | null>("save_file_version", { repoPath, commitId, path }),
    restoreFileVersion: (repoPath: string, commitId: string, path: string) =>
      call<void>("restore_file_version", { repoPath, commitId, path }),
    listRecentAuthors: (repoPath: string) => call<SignatureInput[]>("list_recent_authors", { repoPath }),
    getCommitMessageSettings: () => call<CommitMessageSettings>("get_commit_message_settings"),
    setCommitMessageSettings: (settings: CommitMessageSettings) =>
      call<void>("set_commit_message_settings", { settings }),
    getFetchSettings: () => call<FetchSettings>("get_fetch_settings"),
    setFetchSettings: (settings: FetchSettings) => call<void>("set_fetch_settings", { settings }),
    getAppearanceSettings: () => call<AppearanceSettings>("get_appearance_settings"),
    setAppearanceSettings: (settings: AppearanceSettings) => call<void>("set_appearance_settings", { settings }),
    getUpdateSettings: () => call<UpdateSettings>("get_update_settings"),
    setUpdateSettings: (settings: UpdateSettings) => call<void>("set_update_settings", { settings }),
    getUndoState: (repoPath: string) => call<UndoState>("get_undo_state", { repoPath }),
    undo: (repoPath: string) => call<UndoOutcome>("undo_last", { repoPath }),
    redo: (repoPath: string) => call<UndoOutcome>("redo_last", { repoPath }),
    cherryPick: (repoPath: string, commitId: string, mainline?: number | null) =>
      call<PickOutcome>("cherry_pick", { repoPath, commitId, mainline: mainline ?? null }),
    revertCommit: (repoPath: string, commitId: string, mainline?: number | null) =>
      call<PickOutcome>("revert_commit", { repoPath, commitId, mainline: mainline ?? null }),
    cherryPickMany: (repoPath: string, commitIds: string[]) =>
      call<SequenceOutcome>("cherry_pick_many", { repoPath, commitIds }),
    revertMany: (repoPath: string, commitIds: string[]) => call<SequenceOutcome>("revert_many", { repoPath, commitIds }),
    continueSequence: (repoPath: string, noVerify?: boolean) =>
      call<SequenceOutcome>("continue_sequence", { repoPath, noVerify: noVerify ?? false }),
    abortSequence: (repoPath: string, confirmed?: ConfirmedAbort | null) =>
      call<SequenceAbort>("abort_sequence", { repoPath, confirmed: confirmed ?? null }),
    resolveWholeFile: (repoPath: string, path: string, side: ConflictSide) =>
      call<void>("resolve_whole_file", { repoPath, path, side }),
    resetToCommit: (repoPath: string, commitId: string, mode: ResetMode) =>
      call<void>("reset_to_commit", { repoPath, commitId, mode }),
    checkoutCommit: (repoPath: string, commitId: string) => call<void>("checkout_commit", { repoPath, commitId }),
    forcePushCurrentBranch: (repoPath: string, remoteName: string, noVerify: boolean) =>
      call<string>("force_push_current_branch", { repoPath, remoteName, noVerify }),
    resolveAddDeleteConflict: (repoPath: string, path: string, choice: FileConflictChoice) =>
      call<void>("resolve_add_delete_conflict", { repoPath, path, choice }),
    commitsSince: (repoPath: string, onto: string) =>
      call<RebasePlanCommit[]>("commits_since", { repoPath, onto }),
    startRebase: (repoPath: string, onto: string, plan: RebasePlanEntry[], edit?: HistoryEditTarget | null) =>
      call<RebaseStepResult>("start_rebase", { repoPath, onto, plan, edit: edit ?? null }),
    rebaseContinue: (repoPath: string) => call<RebaseStepResult>("rebase_continue", { repoPath }),
    abortRebase: (repoPath: string) => call<RebaseAbort>("abort_rebase", { repoPath }),
    detectForgeRepository: (repoPath: string) =>
      call<ForgeRepository[]>("detect_forge_repository", { repoPath }),
    saveForgeToken: (repoPath: string, provider: ForgeProvider, account: string, token: string) =>
      call<void>("save_forge_token", { repoPath, provider, account, token }),
    forgetForgeToken: (repoPath: string, provider: ForgeProvider, account: string) =>
      call<void>("forget_forge_token", { repoPath, provider, account }),
    listPullRequests: (repoPath: string, remoteName: string, account: string) =>
      call<PullRequestList>("list_pull_requests", { repoPath, remoteName, account }),
    createPullRequest: (repoPath: string, remoteName: string, account: string, pullRequest: CreatePullRequest) =>
      call<PullRequest>("create_pull_request", { repoPath, remoteName, account, pullRequest }),
    openExternalUrl: (url: string) =>
      call<void>("open_external_url", { url }),
    // In VSCode these are handled natively by the extension host (see `sidecarBridge.ts`'s
    // `NATIVE_METHODS`).
    openInEditor: (repoPath: string, path: string, line: number | null) =>
      call<void>("open_in_editor", { repoPath, path, line }),
    revealInFileManager: (repoPath: string, path: string | null) =>
      call<void>("reveal_in_file_manager", { repoPath, path }),
    openTerminal: (repoPath: string) => call<void>("open_terminal", { repoPath }),
    getRootFiles: (repoPath: string) => call<RootFileName[] | null>("get_root_files", { repoPath }),
    createRootFile: (repoPath: string, name: RootFileName) => call<void>("create_root_file", { repoPath, name }),
    openDiffTool: (repoPath: string, path: string, source: DiffSource) =>
      call<void>("open_diff_tool", { repoPath, path, source }),
    openMergeTool: (repoPath: string, path: string) =>
      call<MergeToolOutcome>("open_merge_tool", { repoPath, path }),
    markResolved: (repoPath: string, path: string, force: boolean) =>
      call<MarkResolvedOutcome>("mark_resolved", { repoPath, path, force }),
    getExternalToolSettings: () => call<ExternalToolSettings>("get_external_tool_settings"),
    setExternalToolSettings: (settings: ExternalToolSettings) =>
      call<void>("set_external_tool_settings", { settings }),
    getGitTools: (repoPath: string) => call<GitTools>("get_git_tools", { repoPath }),  };
}
