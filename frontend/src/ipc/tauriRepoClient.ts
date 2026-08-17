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
  PullOutcome,
  PullRequest,
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
  WorktreeInfo,
} from "./RepoClient";

let transferListenersReady: Promise<void> = Promise.resolve();

export const tauriRepoClient: RepoClient = {
  pickRepoFolder: () => invoke<string | null>("pick_repo_folder"),
  listRecentRepos: () => invoke<string[]>("list_recent_repos"),
  openRepo: (path: string) => invoke("open_repo", { path }),
  getStatus: () => invoke<StatusEntry[]>("get_status"),
  getCommitGraph: (limit: number) =>
    invoke<GraphCommit[]>("get_commit_graph", { limit }),
  getWorkingDiff: (path: string, staged: boolean) =>
    invoke<DiffHunk[]>("get_working_diff", { path, staged }),
  getCommitDiff: (commitId: string, path: string) =>
    invoke<DiffHunk[]>("get_commit_diff", { commitId, path }),
  getCommitFiles: (commitId: string) =>
    invoke<string[]>("get_commit_files", { commitId }),
  stageFile: (path: string) => invoke("stage_file", { path }),
  unstageFile: (path: string) => invoke("unstage_file", { path }),
  commit: (message: string) => invoke("commit", { message }),
  listBranches: () => invoke<BranchInfo[]>("list_branches"),
  createBranch: (name: string, startPoint: string) =>
    invoke("create_branch", { name, startPoint }),
  switchBranch: (name: string) => invoke("switch_branch", { name }),
  deleteBranch: (name: string, force: boolean) =>
    invoke("delete_branch", { name, force }),
  renameBranch: (oldName: string, newName: string) =>
    invoke("rename_branch", { oldName, newName }),
  listWorktrees: () => invoke<WorktreeInfo[]>("list_worktrees"),
  createWorktree: (
    name: string,
    path: string,
    branch: string,
    startPoint: string | null,
  ) => invoke("create_worktree", { name, path, branch, startPoint }),
  removeWorktree: (name: string) => invoke("remove_worktree", { name }),
  pruneWorktrees: () => invoke("prune_worktrees"),
  listSubmodules: () => invoke<SubmoduleInfo[]>("list_submodules"),
  initSubmodule: (path: string) => invoke("init_submodule", { path }),
  updateSubmodule: (path: string, recursive: boolean) =>
    invoke("update_submodule", { path, recursive }),
  listReflogRefs: () => invoke<string[]>("list_reflog_refs"),
  getReflog: (reference: string) => invoke<ReflogEntry[]>("get_reflog", { reference }),
  restoreReflogEntry: (reference: string, newId: string) =>
    invoke("restore_reflog_entry", { reference, newId }),
  listRemotes: () => invoke<RemoteInfo[]>("list_remotes"),
  getCurrentUpstream: () => invoke<UpstreamInfo | null>("get_current_upstream"),
  getRemoteUpstreams: (name: string) => invoke<UpstreamInfo[]>("get_remote_upstreams", { name }),
  addRemote: (name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return invoke("add_remote", { name, fetchUrl, pushUrl });
  },
  renameRemote: (oldName: string, newName: string) =>
    invoke("rename_remote", { oldName, newName }),
  updateRemoteUrls: (name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return invoke("update_remote_urls", { name, fetchUrl, pushUrl });
  },
  removeRemote: (name: string, clearUpstreams: boolean) => invoke("remove_remote", { name, clearUpstreams }),
  saveHttpsCredential: (remoteName: string, username: string, token: string) =>
    invoke("save_https_credential", { remoteName, username, token }),
  forgetHttpsCredential: (remoteName: string) => invoke("forget_https_credential", { remoteName }),
  setRemoteAuthMode: (remoteName: string, mode: RemoteAuthMode, username: string | null) =>
    invoke("set_remote_auth_mode", { remoteName, mode, username }),
  setCurrentUpstream: (remoteName: string, remoteBranch: string) =>
    invoke("set_current_upstream", { remoteName, remoteBranch }),
  clearCurrentUpstream: () => invoke("clear_current_upstream"),
  listTags: () => invoke<TagInfo[]>("list_tags"),
  createTag: (name: string, message: string | null) => invoke("create_tag", { name, message }),
  deleteTag: (name: string) => invoke("delete_tag", { name }),
  fetchRemote: async (remoteName: string) => {
    await transferListenersReady;
    return invoke<string>("fetch_remote", { remoteName });
  },
  pushCurrentBranch: async (remoteName: string) => {
    await transferListenersReady;
    return invoke<string>("push_current_branch", { remoteName });
  },
  pushTags: async (remoteName: string, names: string[]) => {
    await transferListenersReady;
    return invoke<string>("push_tags", { remoteName, names });
  },
  pullCurrentUpstream: async () => {
    await transferListenersReady;
    return invoke<PullOutcome>("pull_current_upstream");
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
  listStashes: () => invoke<StashEntry[]>("list_stashes"),
  saveStash: () => invoke("save_stash"),
  applyStash: (index: number) => invoke("apply_stash", { index }),
  dropStash: (index: number) => invoke("drop_stash", { index }),
  getBlame: (commitId: string, path: string) =>
    invoke<BlameLine[]>("get_blame", { commitId, path }),
  mergeBranch: (branchName: string) =>
    invoke<MergeOutcome>("start_merge", { branchName }),
  getConflictHunks: (path: string) =>
    invoke<ConflictSegment[]>("get_conflict_hunks", { path }),
  resolveConflict: (path: string, resolvedContent: string) =>
    invoke("resolve_conflict", { path, resolvedContent }),
  abortMerge: () => invoke("abort_merge"),
  getMergeMessage: () => invoke<string | null>("get_merge_message"),
  resolveAddDeleteConflict: (path: string, choice: FileConflictChoice) =>
    invoke("resolve_add_delete_conflict", { path, choice }),
  commitsSince: (onto: string) =>
    invoke<RebasePlanCommit[]>("commits_since", { onto }),
  startRebase: (onto: string, plan: RebasePlanEntry[]) =>
    invoke<RebaseStepResult>("start_rebase", { onto, plan }),
  rebaseContinue: () => invoke<RebaseStepResult>("rebase_continue"),
  abortRebase: () => invoke("abort_rebase"),
  getRebaseProgress: () =>
    invoke<{ currentStep: number; totalSteps: number } | null>("get_rebase_progress"),
  detectForgeRepository: () =>
    invoke<ForgeRepository[]>("detect_forge_repository"),
  saveForgeToken: (provider: ForgeProvider, account: string, token: string) =>
    invoke("save_forge_token", { provider, account, token }),
  forgetForgeToken: (provider: ForgeProvider, account: string) =>
    invoke("forget_forge_token", { provider, account }),
  listPullRequests: (remoteName: string, account: string) =>
    invoke<PullRequest[]>("list_pull_requests", { remoteName, account }),
  createPullRequest: (remoteName: string, account: string, pullRequest: CreatePullRequest) =>
    invoke<PullRequest>("create_pull_request", { remoteName, account, pullRequest }),
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
