import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { logFrontendError } from "../lib/logger";
import { validateRemoteUrls } from "./validateRemoteUrls";
import type {
  BlameLine,
  BranchInfo,
  ConflictSegment,
  CreatePullRequest,
  DiffHunk,
  FileConflictChoice,
  ForgeProvider,
  ForgeRepository,
  GraphCommit,
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
  StashEntry,
  StatusEntry,
  SubmoduleInfo,
  TagInfo,
  TransferProgress,
  UpstreamInfo,
  Workspace,
  WorktreeInfo,
} from "./RepoClient";

let transferListenersReady: Promise<void> = Promise.resolve();

// Every backend command failure surfaces to the UI as a rejected `invoke` promise, so this
// is the single point that can log all of them (including ones a component only shows in its
// own error state, never in the browser console) without instrumenting each RepoClient method.
function loggedInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args).catch((error: unknown) => {
    logFrontendError(`IPC ${command} failed`, error);
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
  getWorkingDiff: (repoPath: string, path: string, staged: boolean) =>
    loggedInvoke<DiffHunk[]>("get_working_diff", { repoPath, path, staged }),
  getCommitDiff: (repoPath: string, commitId: string, path: string) =>
    loggedInvoke<DiffHunk[]>("get_commit_diff", { repoPath, commitId, path }),
  getCommitFiles: (repoPath: string, commitId: string) =>
    loggedInvoke<string[]>("get_commit_files", { repoPath, commitId }),
  stageFile: (repoPath: string, path: string) => loggedInvoke("stage_file", { repoPath, path }),
  unstageFile: (repoPath: string, path: string) => loggedInvoke("unstage_file", { repoPath, path }),
  stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    loggedInvoke("stage_hunk", { repoPath, path, oldStart, newStart }),
  unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    loggedInvoke("unstage_hunk", { repoPath, path, oldStart, newStart }),
  discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    loggedInvoke("discard_hunk", { repoPath, path, oldStart, newStart }),
  commit: (repoPath: string, message: string) => loggedInvoke("commit", { repoPath, message }),
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
  fetchRemote: async (repoPath: string, remoteName: string) => {
    await transferListenersReady;
    return loggedInvoke<string>("fetch_remote", { repoPath, remoteName });
  },
  pushCurrentBranch: async (repoPath: string, remoteName: string) => {
    await transferListenersReady;
    return loggedInvoke<string>("push_current_branch", { repoPath, remoteName });
  },
  pushTags: async (repoPath: string, remoteName: string, names: string[]) => {
    await transferListenersReady;
    return loggedInvoke<string>("push_tags", { repoPath, remoteName, names });
  },
  pullCurrentUpstream: async (repoPath: string) => {
    await transferListenersReady;
    return loggedInvoke<PullOutcome>("pull_current_upstream", { repoPath });
  },
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
  listStashes: (repoPath: string) => loggedInvoke<StashEntry[]>("list_stashes", { repoPath }),
  saveStash: (repoPath: string) => loggedInvoke("save_stash", { repoPath }),
  applyStash: (repoPath: string, index: number) => loggedInvoke("apply_stash", { repoPath, index }),
  dropStash: (repoPath: string, index: number) => loggedInvoke("drop_stash", { repoPath, index }),
  getBlame: (repoPath: string, commitId: string, path: string) =>
    loggedInvoke<BlameLine[]>("get_blame", { repoPath, commitId, path }),
  mergeBranch: (repoPath: string, branchName: string) =>
    loggedInvoke<MergeOutcome>("start_merge", { repoPath, branchName }),
  getConflictHunks: (repoPath: string, path: string) =>
    loggedInvoke<ConflictSegment[]>("get_conflict_hunks", { repoPath, path }),
  resolveConflict: (repoPath: string, path: string, resolvedContent: string) =>
    loggedInvoke("resolve_conflict", { repoPath, path, resolvedContent }),
  abortMerge: (repoPath: string) => loggedInvoke("abort_merge", { repoPath }),
  getMergeMessage: (repoPath: string) => loggedInvoke<string | null>("get_merge_message", { repoPath }),
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
};
