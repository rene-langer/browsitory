import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { error as logError } from "@tauri-apps/plugin-log";
import { validateRemoteUrls } from "./validateRemoteUrls";
import { toCloneError } from "./RepoClient";
import type {
  DiffSource,
  ExternalToolSettings,
  GitTools,
  MarkResolvedOutcome,
  MergeToolOutcome,
  CommitOutcome,
  CommitSignature,
  HookProgress,
  BlameLine,
  BranchInfo,
  CloneOutcome,
  CloneRequest,
  ConflictSegment,
  CreatePullRequest,
  DiffHunk,
  DiffOptions,
  DiffSettings,
  FileConflictChoice,
  FileHistoryEntry,
  ChangedFile,
  CompareRange,
  SearchPage,
  ForgeProvider,
  ForgeRepository,
  GraphCommit,
  MergeOptions,
  MergeOutcome,
  OpenRepoEntry,
  PullOutcome,
  PullRequest,
  PullRequestList,
  RebasePlanCommit,
  RebasePlanEntry,
  ReflogEntry,
  RebaseStepResult,
  RemoteInfo,
  RemoteAuthMode,
  RepoClient,
  StashApplyOutcome,
  StashEntry,
  StashFiles,
  StashOptions,
  StatusEntry,
  SubmoduleInfo,
  TagInfo,
  TransferProgress,
  UpstreamInfo,
  Workspace,
  WorktreeInfo,
  HeadInfo,
  PickOutcome,
  SequenceAbort,
  SequenceOutcome,
  ConflictSide,
  UndoOutcome,
  UndoState,
  ResetMode,
} from "./RepoClient";

let transferListenersReady: Promise<void> = Promise.resolve();
let hookListenersReady: Promise<void> = Promise.resolve();

// Single choke point for frontend failure logging: writes into the same rotated log file as
// the Rust backend (see crates/tauri-app/src/main.rs's log plugin setup), so a bug report
// doesn't need console access to a running dev session to diagnose. Backs both this module's
// own `loggedInvoke` below and the `RepoClient.logFrontendError` method exposed to
// `frontend/src/lib/logger.ts`'s global `error`/`unhandledrejection` handlers.
function logFrontendError(context: string, error: unknown): Promise<void> {
  return logError(`${context}: ${String(error)}`);
}

// Every backend command failure surfaces to the UI as a rejected `invoke` promise, so this
// is the single point that can log all of them (including ones a component only shows in its
// own error state, never in the browser console) without instrumenting each RepoClient method.
function loggedInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args).catch((error: unknown) => {
    void logFrontendError(`IPC ${command} failed`, error);
    throw error;
  });
}

export const tauriRepoClient: RepoClient = {
  pickRepoFolder: () => loggedInvoke<string | null>("pick_repo_folder"),
  listRecentRepos: () => loggedInvoke<string[]>("list_recent_repos"),
  getAppVersion: () => loggedInvoke<string>("get_app_version"),
  getLastSeenVersion: () => loggedInvoke<string | null>("get_last_seen_version"),
  setLastSeenVersion: (version: string) => loggedInvoke("set_last_seen_version", { version }),
  openRepo: (path: string) => loggedInvoke("open_repo", { path }),
  closeRepo: (repoPath: string) => loggedInvoke("close_repo", { repoPath }),
  listOpenRepos: () =>
    loggedInvoke<[OpenRepoEntry[], string | null]>("list_open_repos").then(([entries, activePath]) => ({
      entries,
      activePath,
    })),
  persistOpenRepos: (entries: OpenRepoEntry[], activePath: string | null) =>
    loggedInvoke("persist_open_repos", { entries, activePath }),
  scanReposInRoot: (root: string) => loggedInvoke<string[]>("scan_repos_in_root", { root }),
  supportsCloneAndInit: true,
  // `loggedInvoke` logs only the command name and the rejection payload (`{ kind, message }`),
  // never the arguments, so the `request.https.token` is not logged.
  cloneRepo: (operationId: string, request: CloneRequest) =>
    loggedInvoke<CloneOutcome>("clone_repo", { operationId, request }).catch((error: unknown) => {
      throw toCloneError(error);
    }),
  cancelClone: (operationId: string) => loggedInvoke<void>("cancel_clone", { operationId }),
  initRepo: (path: string) =>
    loggedInvoke<void>("init_repo", { path }).catch((error: unknown) => {
      throw toCloneError(error);
    }),
  listWorkspaces: () => loggedInvoke<Workspace[]>("list_workspaces"),
  saveWorkspace: (name: string, root: string, members: string[]) =>
    loggedInvoke<string>("save_workspace", { name, root, members }),
  updateWorkspace: (id: string, name: string, members: string[]) =>
    loggedInvoke("update_workspace", { id, name, members }),
  deleteWorkspace: (id: string) => loggedInvoke("delete_workspace", { id }),
  getStatus: (repoPath: string) => loggedInvoke<StatusEntry[]>("get_status", { repoPath }),
  getCommitGraph: (repoPath: string, limit: number, selectedBranches: string[] | null) =>
    loggedInvoke<GraphCommit[]>("get_commit_graph", { repoPath, limit, selectedBranches }),
  getGraphBranchSelection: (repoPath: string) =>
    loggedInvoke<string[] | null>("get_graph_branch_selection", { repoPath }),
  setGraphBranchSelection: (repoPath: string, selectedBranches: string[]) =>
    loggedInvoke("set_graph_branch_selection", { repoPath, selectedBranches }),
  getGraphRemoteBranchSelection: (repoPath: string) =>
    loggedInvoke<string[] | null>("get_graph_remote_branch_selection", { repoPath }),
  setGraphRemoteBranchSelection: (repoPath: string, selectedBranches: string[]) =>
    loggedInvoke("set_graph_remote_branch_selection", { repoPath, selectedBranches }),
  getWorkingDiff: (repoPath: string, path: string, staged: boolean, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_working_diff", { repoPath, path, staged, opts }),
  getCommitDiff: (repoPath: string, commitId: string, path: string, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_commit_diff", { repoPath, commitId, path, opts }),
  getFileHistory: (repoPath: string, path: string, limit: number) =>
    loggedInvoke<FileHistoryEntry[]>("get_file_history", { repoPath, path, limit }),
  // Plain `invoke` on purpose: query errors and cancellations are expected while typing and are
  // shown in the UI, not logged.
  searchCommits: (repoPath: string, query: string, refs: string[] | null, cursor: string | null) =>
    invoke<SearchPage>("search_commits", { repoPath, query, refs, cursor }),
  cancelSearch: (repoPath: string) => loggedInvoke<void>("cancel_search", { repoPath }),
  getCommitRenameDiff: (repoPath: string, commitId: string, oldPath: string, newPath: string, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_commit_rename_diff", { repoPath, commitId, oldPath, newPath, opts }),
  getCommitFiles: (repoPath: string, commitId: string) =>
    loggedInvoke<string[]>("get_commit_files", { repoPath, commitId }),
  getRangeFiles: (repoPath: string, fromId: string, toId: string) =>
    loggedInvoke<ChangedFile[]>("get_range_files", { repoPath, fromId, toId }),
  getRangeDiff: (repoPath: string, fromId: string, toId: string, path: string, oldPath: string | null, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_range_diff", { repoPath, fromId, toId, path, oldPath, opts }),
  getWorkingTreeCompareFiles: (repoPath: string, commitId: string) =>
    loggedInvoke<ChangedFile[]>("get_working_tree_compare_files", { repoPath, commitId }),
  getWorkingTreeCompareDiff: (repoPath: string, commitId: string, path: string, oldPath: string | null, opts: DiffOptions) =>
    loggedInvoke<DiffHunk[]>("get_working_tree_compare_diff", { repoPath, commitId, path, oldPath, opts }),
  getBranchCompareRange: (repoPath: string, refName: string) =>
    loggedInvoke<CompareRange>("get_branch_compare_range", { repoPath, refName }),
  getCommitMessage: (repoPath: string, commitId: string) =>
    loggedInvoke<string>("get_commit_message", { repoPath, commitId }),
  getCommitSignature: (repoPath: string, commitId: string) =>
    loggedInvoke<CommitSignature>("get_commit_signature", { repoPath, commitId }),
  stageFile: (repoPath: string, path: string) => loggedInvoke("stage_file", { repoPath, path }),
  unstageFile: (repoPath: string, path: string) => loggedInvoke("unstage_file", { repoPath, path }),
  stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number) =>
    loggedInvoke("stage_hunk", { repoPath, path, oldStart, newStart, contextLines }),
  unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number) =>
    loggedInvoke("unstage_hunk", { repoPath, path, oldStart, newStart, contextLines }),
  discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number) =>
    loggedInvoke("discard_hunk", { repoPath, path, oldStart, newStart, contextLines }),
  stageLines: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) =>
    loggedInvoke("stage_lines", { repoPath, path, oldStart, newStart, contextLines, lineIndices, expectedLines }),
  unstageLines: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) =>
    loggedInvoke("unstage_lines", { repoPath, path, oldStart, newStart, contextLines, lineIndices, expectedLines }),
  discardLines: (repoPath: string, path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) =>
    loggedInvoke("discard_lines", { repoPath, path, oldStart, newStart, contextLines, lineIndices, expectedLines }),
  discardFile: (repoPath: string, path: string) => loggedInvoke("discard_file", { repoPath, path }),
  discardAllUnstaged: (repoPath: string, paths: string[]) => loggedInvoke("discard_all_unstaged", { repoPath, paths }),
  ignorePath: (repoPath: string, pattern: string) => loggedInvoke("ignore_path", { repoPath, pattern }),
  getDiffSettings: () => loggedInvoke<DiffSettings>("get_diff_settings"),
  setDiffSettings: (settings: DiffSettings) => loggedInvoke("set_diff_settings", { settings }),
  commit: async (repoPath: string, message: string, noVerify: boolean) => {
    await hookListenersReady;
    return loggedInvoke<CommitOutcome>("commit", { repoPath, message, noVerify });
  },
  listBranches: (repoPath: string) => loggedInvoke<BranchInfo[]>("list_branches", { repoPath }),
  createBranch: (repoPath: string, name: string, startPoint: string) =>
    loggedInvoke("create_branch", { repoPath, name, startPoint }),
  switchBranch: (repoPath: string, name: string) => loggedInvoke("switch_branch", { repoPath, name }),
  deleteBranch: (repoPath: string, name: string, force: boolean) =>
    loggedInvoke("delete_branch", { repoPath, name, force }),
  renameBranch: (repoPath: string, oldName: string, newName: string) =>
    loggedInvoke("rename_branch", { repoPath, oldName, newName }),
  listWorktrees: (repoPath: string) => loggedInvoke<WorktreeInfo[]>("list_worktrees", { repoPath }),
  createWorktree: (
    repoPath: string,
    name: string,
    path: string,
    branch: string,
    startPoint: string | null,
  ) => loggedInvoke("create_worktree", { repoPath, name, path, branch, startPoint }),
  removeWorktree: (repoPath: string, name: string) => loggedInvoke("remove_worktree", { repoPath, name }),
  pruneWorktrees: (repoPath: string) => loggedInvoke("prune_worktrees", { repoPath }),
  listSubmodules: (repoPath: string) => loggedInvoke<SubmoduleInfo[]>("list_submodules", { repoPath }),
  initSubmodule: (repoPath: string, path: string) => loggedInvoke("init_submodule", { repoPath, path }),
  updateSubmodule: (repoPath: string, path: string, recursive: boolean) =>
    loggedInvoke("update_submodule", { repoPath, path, recursive }),
  listReflogRefs: (repoPath: string) => loggedInvoke<string[]>("list_reflog_refs", { repoPath }),
  getReflog: (repoPath: string, reference: string) => loggedInvoke<ReflogEntry[]>("get_reflog", { repoPath, reference }),
  restoreReflogEntry: (repoPath: string, reference: string, newId: string) =>
    loggedInvoke("restore_reflog_entry", { repoPath, reference, newId }),
  listRemotes: (repoPath: string) => loggedInvoke<RemoteInfo[]>("list_remotes", { repoPath }),
  listRemoteBranches: (repoPath: string, remoteName: string) =>
    loggedInvoke<string[]>("list_remote_branches", { repoPath, remoteName }),
  getCurrentUpstream: (repoPath: string) => loggedInvoke<UpstreamInfo | null>("get_current_upstream", { repoPath }),
  getRemoteUpstreams: (repoPath: string, name: string) => loggedInvoke<UpstreamInfo[]>("get_remote_upstreams", { repoPath, name }),
  addRemote: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return loggedInvoke("add_remote", { repoPath, name, fetchUrl, pushUrl });
  },
  renameRemote: (repoPath: string, oldName: string, newName: string) =>
    loggedInvoke("rename_remote", { repoPath, oldName, newName }),
  updateRemoteUrls: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return loggedInvoke("update_remote_urls", { repoPath, name, fetchUrl, pushUrl });
  },
  removeRemote: (repoPath: string, name: string, clearUpstreams: boolean) => loggedInvoke("remove_remote", { repoPath, name, clearUpstreams }),
  saveHttpsCredential: (repoPath: string, remoteName: string, username: string, token: string) =>
    loggedInvoke("save_https_credential", { repoPath, remoteName, username, token }),
  forgetHttpsCredential: (repoPath: string, remoteName: string) => loggedInvoke("forget_https_credential", { repoPath, remoteName }),
  setRemoteAuthMode: (repoPath: string, remoteName: string, mode: RemoteAuthMode, username: string | null) =>
    loggedInvoke("set_remote_auth_mode", { repoPath, remoteName, mode, username }),
  setCurrentUpstream: (repoPath: string, remoteName: string, remoteBranch: string) =>
    loggedInvoke("set_current_upstream", { repoPath, remoteName, remoteBranch }),
  clearCurrentUpstream: (repoPath: string) => loggedInvoke("clear_current_upstream", { repoPath }),
  listTags: (repoPath: string) => loggedInvoke<TagInfo[]>("list_tags", { repoPath }),
  createTag: (repoPath: string, name: string, message: string | null) => loggedInvoke("create_tag", { repoPath, name, message }),
  deleteTag: (repoPath: string, name: string) => loggedInvoke("delete_tag", { repoPath, name }),
  fetchRemote: async (repoPath: string, remoteName: string, prune: boolean | null) => {
    await transferListenersReady;
    return loggedInvoke<string>("fetch_remote", { repoPath, remoteName, prune });
  },
  fetchAllRemotes: async (repoPath: string, prune: boolean | null) => {
    await transferListenersReady;
    return loggedInvoke<string>("fetch_all_remotes", { repoPath, prune });
  },
  pushBranch: async (repoPath: string, remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify: boolean) => {
    await transferListenersReady;
    return loggedInvoke<string>("push_branch", { repoPath, remoteName, localBranch, remoteBranch, setUpstream, noVerify });
  },
  deleteRemoteBranch: async (repoPath: string, remoteName: string, branchName: string, noVerify: boolean) => {
    await transferListenersReady;
    return loggedInvoke<string>("delete_remote_branch", { repoPath, remoteName, branchName, noVerify });
  },
  pushCurrentBranch: async (repoPath: string, remoteName: string, noVerify: boolean) => {
    await transferListenersReady;
    return loggedInvoke<string>("push_current_branch", { repoPath, remoteName, noVerify });
  },
  pushTags: async (repoPath: string, remoteName: string, names: string[], noVerify: boolean) => {
    await transferListenersReady;
    return loggedInvoke<string>("push_tags", { repoPath, remoteName, names, noVerify });
  },
  pullCurrentUpstream: async (repoPath: string) => {
    await transferListenersReady;
    return loggedInvoke<PullOutcome>("pull_current_upstream", { repoPath });
  },
  // No `transferListenersReady` await, unlike the calls above: this one starts no transfer and
  // so emits no events of its own — the cancellation shows up on the listeners the cancelled
  // operation already set up.
  cancelTransfer: (repoPath: string, operationId: string) =>
    loggedInvoke("cancel_transfer", { repoPath, operationId }),
  subscribeTransferProgress: (listener: (progress: TransferProgress) => void) => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    const registrations = ["transfer-progress", "transfer-complete"].map((event) =>
      listen<TransferProgress>(event, ({ payload }) => listener(payload)),
    );
    transferListenersReady = Promise.all(registrations).then((stops) => {
      for (const stop of stops) {
        if (disposed) stop();
        else unlisten.push(stop);
      }
    });
    void transferListenersReady.catch(() => {});
    return () => {
      if (disposed) return;
      disposed = true;
      for (const stop of unlisten) stop();
    };
  },
  subscribeHookProgress: (listener: (event: HookProgress) => void) => {
    let disposed = false;
    let stop: (() => void) | null = null;
    const registration = listen<HookProgress>("hook-progress", ({ payload }) => listener(payload));
    // `commit`/`amendCommit` wait for this so a fast hook's events are not emitted before
    // anyone listens (same reason as `transferListenersReady`).
    hookListenersReady = registration.then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    void hookListenersReady.catch(() => {});
    return () => {
      if (disposed) return;
      disposed = true;
      stop?.();
    };
  },
  listStashes: (repoPath: string) => loggedInvoke<StashEntry[]>("list_stashes", { repoPath }),
  saveStash: (repoPath: string, options: StashOptions) => loggedInvoke("save_stash", { repoPath, options }),
  applyStash: (repoPath: string, index: number, commitId: string, restoreIndex: boolean) =>
    loggedInvoke<StashApplyOutcome>("apply_stash", { repoPath, index, commitId, restoreIndex }),
  popStash: (repoPath: string, index: number, commitId: string, restoreIndex: boolean) =>
    loggedInvoke<StashApplyOutcome>("pop_stash", { repoPath, index, commitId, restoreIndex }),
  getStashFiles: (repoPath: string, commitId: string) =>
    loggedInvoke<StashFiles>("get_stash_files", { repoPath, commitId }),
  dropStash: (repoPath: string, index: number, commitId: string) =>
    loggedInvoke("drop_stash", { repoPath, index, commitId }),
  getBlame: (repoPath: string, commitId: string, path: string) =>
    loggedInvoke<BlameLine[]>("get_blame", { repoPath, commitId, path }),
  mergeBranch: (repoPath: string, branchName: string, options: MergeOptions) =>
    loggedInvoke<MergeOutcome>("start_merge", { repoPath, branchName, options }),
  getMergeOptions: (repoPath: string) => loggedInvoke<MergeOptions>("get_merge_options", { repoPath }),
  setMergeOptions: (repoPath: string, options: MergeOptions) =>
    loggedInvoke("set_merge_options", { repoPath, options }),
  getConflictHunks: (repoPath: string, path: string) =>
    loggedInvoke<ConflictSegment[]>("get_conflict_hunks", { repoPath, path }),
  resolveConflict: (repoPath: string, path: string, resolvedContent: string) =>
    loggedInvoke("resolve_conflict", { repoPath, path, resolvedContent }),
  abortMerge: (repoPath: string) => loggedInvoke("abort_merge", { repoPath }),
  getMergeMessage: (repoPath: string) => loggedInvoke<string | null>("get_merge_message", { repoPath }),
  getHeadInfo: (repoPath: string) => loggedInvoke<HeadInfo>("get_head_info", { repoPath }),
  amendCommit: async (repoPath: string, message: string, noVerify: boolean) => {
    await hookListenersReady;
    return loggedInvoke<CommitOutcome>("amend_commit", { repoPath, message, noVerify });
  },
  getUndoState: (repoPath: string) => loggedInvoke<UndoState>("get_undo_state", { repoPath }),
  undo: (repoPath: string) => loggedInvoke<UndoOutcome>("undo_last", { repoPath }),
  redo: (repoPath: string) => loggedInvoke<UndoOutcome>("redo_last", { repoPath }),
  cherryPick: (repoPath: string, commitId: string, mainline?: number | null) =>
    loggedInvoke<PickOutcome>("cherry_pick", { repoPath, commitId, mainline: mainline ?? null }),
  revertCommit: (repoPath: string, commitId: string, mainline?: number | null) =>
    loggedInvoke<PickOutcome>("revert_commit", { repoPath, commitId, mainline: mainline ?? null }),
  cherryPickMany: (repoPath: string, commitIds: string[]) =>
    loggedInvoke<SequenceOutcome>("cherry_pick_many", { repoPath, commitIds }),
  revertMany: (repoPath: string, commitIds: string[]) =>
    loggedInvoke<SequenceOutcome>("revert_many", { repoPath, commitIds }),
  continueSequence: (repoPath: string) => loggedInvoke<SequenceOutcome>("continue_sequence", { repoPath }),
  abortSequence: (repoPath: string) => loggedInvoke<SequenceAbort>("abort_sequence", { repoPath }),
  resolveWholeFile: (repoPath: string, path: string, side: ConflictSide) =>
    loggedInvoke<void>("resolve_whole_file", { repoPath, path, side }),
  resetToCommit: (repoPath: string, commitId: string, mode: ResetMode) =>
    loggedInvoke<void>("reset_to_commit", { repoPath, commitId, mode }),
  checkoutCommit: (repoPath: string, commitId: string) => loggedInvoke<void>("checkout_commit", { repoPath, commitId }),
  forcePushCurrentBranch: (repoPath: string, remoteName: string, noVerify: boolean) =>
    loggedInvoke<string>("force_push_current_branch", { repoPath, remoteName, noVerify }),
  resolveAddDeleteConflict: (repoPath: string, path: string, choice: FileConflictChoice) =>
    loggedInvoke("resolve_add_delete_conflict", { repoPath, path, choice }),
  commitsSince: (repoPath: string, onto: string) =>
    loggedInvoke<RebasePlanCommit[]>("commits_since", { repoPath, onto }),
  startRebase: (repoPath: string, onto: string, plan: RebasePlanEntry[]) =>
    loggedInvoke<RebaseStepResult>("start_rebase", { repoPath, onto, plan }),
  rebaseContinue: (repoPath: string) => loggedInvoke<RebaseStepResult>("rebase_continue", { repoPath }),
  abortRebase: (repoPath: string) => loggedInvoke("abort_rebase", { repoPath }),
  getRebaseProgress: (repoPath: string) =>
    loggedInvoke<{ currentStep: number; totalSteps: number } | null>("get_rebase_progress", { repoPath }),
  detectForgeRepository: (repoPath: string) =>
    loggedInvoke<ForgeRepository[]>("detect_forge_repository", { repoPath }),
  saveForgeToken: (repoPath: string, provider: ForgeProvider, account: string, token: string) =>
    loggedInvoke("save_forge_token", { repoPath, provider, account, token }),
  forgetForgeToken: (repoPath: string, provider: ForgeProvider, account: string) =>
    loggedInvoke("forget_forge_token", { repoPath, provider, account }),
  listPullRequests: (repoPath: string, remoteName: string, account: string) =>
    loggedInvoke<PullRequestList>("list_pull_requests", { repoPath, remoteName, account }),
  createPullRequest: (repoPath: string, remoteName: string, account: string, pullRequest: CreatePullRequest) =>
    loggedInvoke<PullRequest>("create_pull_request", { repoPath, remoteName, account, pullRequest }),
  openExternalUrl: (url: string) => loggedInvoke("open_external_url", { url }),
  openInEditor: (repoPath: string, path: string, line: number | null) =>
    loggedInvoke("open_in_editor", { repoPath, path, line }),
  revealInFileManager: (repoPath: string, path: string | null) =>
    loggedInvoke("reveal_in_file_manager", { repoPath, path }),
  openTerminal: (repoPath: string) => loggedInvoke("open_terminal", { repoPath }),
  openDiffTool: (repoPath: string, path: string, source: DiffSource) =>
    loggedInvoke("open_diff_tool", { repoPath, path, source }),
  openMergeTool: (repoPath: string, path: string) =>
    loggedInvoke<MergeToolOutcome>("open_merge_tool", { repoPath, path }),
  markResolved: (repoPath: string, path: string, force: boolean) =>
    loggedInvoke<MarkResolvedOutcome>("mark_resolved", { repoPath, path, force }),
  getExternalToolSettings: () => loggedInvoke<ExternalToolSettings>("get_external_tool_settings"),
  setExternalToolSettings: (settings: ExternalToolSettings) =>
    loggedInvoke("set_external_tool_settings", { settings }),
  getGitTools: (repoPath: string) => loggedInvoke<GitTools>("get_git_tools", { repoPath }),
  logFrontendError: (context: string, error: unknown) => logFrontendError(context, error),
};
