import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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

export const tauriRepoClient: RepoClient = {
  pickRepoFolder: () => invoke<string | null>("pick_repo_folder"),
  listRecentRepos: () => invoke<string[]>("list_recent_repos"),
  openRepo: (path: string) => invoke("open_repo", { path }),
  closeRepo: (repoPath: string) => invoke("close_repo", { repoPath }),
  listOpenRepos: () =>
    invoke<[OpenRepoEntry[], string | null]>("list_open_repos").then(([entries, activePath]) => ({
      entries,
      activePath,
    })),
  persistOpenRepos: (entries: OpenRepoEntry[], activePath: string | null) =>
    invoke("persist_open_repos", { entries, activePath }),
  scanReposInRoot: (root: string) => invoke<string[]>("scan_repos_in_root", { root }),
  listWorkspaces: () => invoke<Workspace[]>("list_workspaces"),
  saveWorkspace: (name: string, root: string, members: string[]) =>
    invoke<string>("save_workspace", { name, root, members }),
  updateWorkspace: (id: string, name: string, members: string[]) =>
    invoke("update_workspace", { id, name, members }),
  deleteWorkspace: (id: string) => invoke("delete_workspace", { id }),
  getStatus: (repoPath: string) => invoke<StatusEntry[]>("get_status", { repoPath }),
  getCommitGraph: (repoPath: string, limit: number) =>
    invoke<GraphCommit[]>("get_commit_graph", { repoPath, limit }),
  getWorkingDiff: (repoPath: string, path: string, staged: boolean) =>
    invoke<DiffHunk[]>("get_working_diff", { repoPath, path, staged }),
  getCommitDiff: (repoPath: string, commitId: string, path: string) =>
    invoke<DiffHunk[]>("get_commit_diff", { repoPath, commitId, path }),
  getCommitFiles: (repoPath: string, commitId: string) =>
    invoke<string[]>("get_commit_files", { repoPath, commitId }),
  stageFile: (repoPath: string, path: string) => invoke("stage_file", { repoPath, path }),
  unstageFile: (repoPath: string, path: string) => invoke("unstage_file", { repoPath, path }),
  stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    invoke("stage_hunk", { repoPath, path, oldStart, newStart }),
  unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    invoke("unstage_hunk", { repoPath, path, oldStart, newStart }),
  discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    invoke("discard_hunk", { repoPath, path, oldStart, newStart }),
  commit: (repoPath: string, message: string) => invoke("commit", { repoPath, message }),
  listBranches: (repoPath: string) => invoke<BranchInfo[]>("list_branches", { repoPath }),
  createBranch: (repoPath: string, name: string, startPoint: string) =>
    invoke("create_branch", { repoPath, name, startPoint }),
  switchBranch: (repoPath: string, name: string) => invoke("switch_branch", { repoPath, name }),
  deleteBranch: (repoPath: string, name: string, force: boolean) =>
    invoke("delete_branch", { repoPath, name, force }),
  renameBranch: (repoPath: string, oldName: string, newName: string) =>
    invoke("rename_branch", { repoPath, oldName, newName }),
  listWorktrees: (repoPath: string) => invoke<WorktreeInfo[]>("list_worktrees", { repoPath }),
  createWorktree: (
    repoPath: string,
    name: string,
    path: string,
    branch: string,
    startPoint: string | null,
  ) => invoke("create_worktree", { repoPath, name, path, branch, startPoint }),
  removeWorktree: (repoPath: string, name: string) => invoke("remove_worktree", { repoPath, name }),
  pruneWorktrees: (repoPath: string) => invoke("prune_worktrees", { repoPath }),
  listSubmodules: (repoPath: string) => invoke<SubmoduleInfo[]>("list_submodules", { repoPath }),
  initSubmodule: (repoPath: string, path: string) => invoke("init_submodule", { repoPath, path }),
  updateSubmodule: (repoPath: string, path: string, recursive: boolean) =>
    invoke("update_submodule", { repoPath, path, recursive }),
  listReflogRefs: (repoPath: string) => invoke<string[]>("list_reflog_refs", { repoPath }),
  getReflog: (repoPath: string, reference: string) => invoke<ReflogEntry[]>("get_reflog", { repoPath, reference }),
  restoreReflogEntry: (repoPath: string, reference: string, newId: string) =>
    invoke("restore_reflog_entry", { repoPath, reference, newId }),
  listRemotes: (repoPath: string) => invoke<RemoteInfo[]>("list_remotes", { repoPath }),
  listRemoteBranches: (repoPath: string, remoteName: string) =>
    invoke<string[]>("list_remote_branches", { repoPath, remoteName }),
  getCurrentUpstream: (repoPath: string) => invoke<UpstreamInfo | null>("get_current_upstream", { repoPath }),
  getRemoteUpstreams: (repoPath: string, name: string) => invoke<UpstreamInfo[]>("get_remote_upstreams", { repoPath, name }),
  addRemote: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return invoke("add_remote", { repoPath, name, fetchUrl, pushUrl });
  },
  renameRemote: (repoPath: string, oldName: string, newName: string) =>
    invoke("rename_remote", { repoPath, oldName, newName }),
  updateRemoteUrls: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return invoke("update_remote_urls", { repoPath, name, fetchUrl, pushUrl });
  },
  removeRemote: (repoPath: string, name: string, clearUpstreams: boolean) => invoke("remove_remote", { repoPath, name, clearUpstreams }),
  saveHttpsCredential: (repoPath: string, remoteName: string, username: string, token: string) =>
    invoke("save_https_credential", { repoPath, remoteName, username, token }),
  forgetHttpsCredential: (repoPath: string, remoteName: string) => invoke("forget_https_credential", { repoPath, remoteName }),
  setRemoteAuthMode: (repoPath: string, remoteName: string, mode: RemoteAuthMode, username: string | null) =>
    invoke("set_remote_auth_mode", { repoPath, remoteName, mode, username }),
  setCurrentUpstream: (repoPath: string, remoteName: string, remoteBranch: string) =>
    invoke("set_current_upstream", { repoPath, remoteName, remoteBranch }),
  clearCurrentUpstream: (repoPath: string) => invoke("clear_current_upstream", { repoPath }),
  listTags: (repoPath: string) => invoke<TagInfo[]>("list_tags", { repoPath }),
  createTag: (repoPath: string, name: string, message: string | null) => invoke("create_tag", { repoPath, name, message }),
  deleteTag: (repoPath: string, name: string) => invoke("delete_tag", { repoPath, name }),
  fetchRemote: async (repoPath: string, remoteName: string) => {
    await transferListenersReady;
    return invoke<string>("fetch_remote", { repoPath, remoteName });
  },
  pushCurrentBranch: async (repoPath: string, remoteName: string) => {
    await transferListenersReady;
    return invoke<string>("push_current_branch", { repoPath, remoteName });
  },
  pushTags: async (repoPath: string, remoteName: string, names: string[]) => {
    await transferListenersReady;
    return invoke<string>("push_tags", { repoPath, remoteName, names });
  },
  pullCurrentUpstream: async (repoPath: string) => {
    await transferListenersReady;
    return invoke<PullOutcome>("pull_current_upstream", { repoPath });
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
  listStashes: (repoPath: string) => invoke<StashEntry[]>("list_stashes", { repoPath }),
  saveStash: (repoPath: string) => invoke("save_stash", { repoPath }),
  applyStash: (repoPath: string, index: number) => invoke("apply_stash", { repoPath, index }),
  dropStash: (repoPath: string, index: number) => invoke("drop_stash", { repoPath, index }),
  getBlame: (repoPath: string, commitId: string, path: string) =>
    invoke<BlameLine[]>("get_blame", { repoPath, commitId, path }),
  mergeBranch: (repoPath: string, branchName: string) =>
    invoke<MergeOutcome>("start_merge", { repoPath, branchName }),
  getConflictHunks: (repoPath: string, path: string) =>
    invoke<ConflictSegment[]>("get_conflict_hunks", { repoPath, path }),
  resolveConflict: (repoPath: string, path: string, resolvedContent: string) =>
    invoke("resolve_conflict", { repoPath, path, resolvedContent }),
  abortMerge: (repoPath: string) => invoke("abort_merge", { repoPath }),
  getMergeMessage: (repoPath: string) => invoke<string | null>("get_merge_message", { repoPath }),
  resolveAddDeleteConflict: (repoPath: string, path: string, choice: FileConflictChoice) =>
    invoke("resolve_add_delete_conflict", { repoPath, path, choice }),
  commitsSince: (repoPath: string, onto: string) =>
    invoke<RebasePlanCommit[]>("commits_since", { repoPath, onto }),
  startRebase: (repoPath: string, onto: string, plan: RebasePlanEntry[]) =>
    invoke<RebaseStepResult>("start_rebase", { repoPath, onto, plan }),
  rebaseContinue: (repoPath: string) => invoke<RebaseStepResult>("rebase_continue", { repoPath }),
  abortRebase: (repoPath: string) => invoke("abort_rebase", { repoPath }),
  getRebaseProgress: (repoPath: string) =>
    invoke<{ currentStep: number; totalSteps: number } | null>("get_rebase_progress", { repoPath }),
  detectForgeRepository: (repoPath: string) =>
    invoke<ForgeRepository[]>("detect_forge_repository", { repoPath }),
  saveForgeToken: (repoPath: string, provider: ForgeProvider, account: string, token: string) =>
    invoke("save_forge_token", { repoPath, provider, account, token }),
  forgetForgeToken: (repoPath: string, provider: ForgeProvider, account: string) =>
    invoke("forget_forge_token", { repoPath, provider, account }),
  listPullRequests: (repoPath: string, remoteName: string, account: string) =>
    invoke<PullRequestList>("list_pull_requests", { repoPath, remoteName, account }),
  createPullRequest: (repoPath: string, remoteName: string, account: string, pullRequest: CreatePullRequest) =>
    invoke<PullRequest>("create_pull_request", { repoPath, remoteName, account, pullRequest }),
  openExternalUrl: (url: string) => invoke("open_external_url", { url }),
};

export function validateRemoteUrls(fetchUrl: string, pushUrl: string | null) {
  for (const url of [fetchUrl, pushUrl]) {
    if (url === null) continue;
    try {
      const parsed = new URL(url);
      if ((parsed.protocol === "http:" || parsed.protocol === "https:") && (parsed.username !== "" || parsed.password !== "")) {
        throw new Error("Remote URLs must not contain embedded credentials");
      }
    } catch (error) {
      if (error instanceof Error && error.message === "Remote URLs must not contain embedded credentials") throw error;
    }
  }
}
