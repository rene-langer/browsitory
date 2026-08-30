import type { DiffHunk, GraphCommit, RepoClient, StatusEntry } from "./RepoClient";

// No `vscode` npm package dependency here on purpose — the real extension host's webview API
// typings arrive with sub-phase (c)'s `extension/` package. This ambient declaration is the
// minimal shape this file actually calls.
interface VsCodeWebviewApi {
  postMessage(message: unknown): void;
}

declare function acquireVsCodeApi(): VsCodeWebviewApi;

interface JsonRpcSuccess {
  jsonrpc: "2.0";
  id: number;
  result: unknown;
}

interface JsonRpcFailure {
  jsonrpc: "2.0";
  id: number;
  error: { code: number; message: string };
}

type JsonRpcResponse = JsonRpcSuccess | JsonRpcFailure;

function isJsonRpcResponse(value: unknown): value is JsonRpcResponse {
  return (
    typeof value === "object" &&
    value !== null &&
    "jsonrpc" in value &&
    "id" in value &&
    ("result" in value || "error" in value)
  );
}

const vscode = acquireVsCodeApi();
let nextRequestId = 1;
const pending = new Map<
  number,
  { resolve: (value: unknown) => void; reject: (error: Error) => void }
>();

window.addEventListener("message", (event: MessageEvent) => {
  const message = event.data;
  if (!isJsonRpcResponse(message)) return;
  const waiting = pending.get(message.id);
  if (!waiting) return;
  pending.delete(message.id);
  if ("error" in message) {
    waiting.reject(new Error(message.error.message));
  } else {
    waiting.resolve(message.result);
  }
});

function call<T>(method: string, params: Record<string, unknown>): Promise<T> {
  const id = nextRequestId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    vscode.postMessage({ jsonrpc: "2.0", id, method, params });
  });
}

function notImplemented(method: string) {
  return (): Promise<never> =>
    Promise.reject(new Error(`vscodeRepoClient: ${method} is not implemented yet`));
}

export const vscodeRepoClient: RepoClient = {
  openRepo: (path: string) => call<void>("open_repo", { path }),
  closeRepo: (repoPath: string) => call<void>("close_repo", { repoPath }),
  getStatus: (repoPath: string) => call<StatusEntry[]>("get_status", { repoPath }),
  getCommitGraph: (repoPath: string, limit: number, selectedBranches: string[] | null) =>
    call<GraphCommit[]>("get_commit_graph", { repoPath, limit, selectedBranches }),
  getWorkingDiff: (repoPath: string, path: string, staged: boolean) =>
    call<DiffHunk[]>("get_working_diff", { repoPath, path, staged }),
  getCommitDiff: (repoPath: string, commitId: string, path: string) =>
    call<DiffHunk[]>("get_commit_diff", { repoPath, commitId, path }),

  pickRepoFolder: notImplemented("pickRepoFolder"),
  listRecentRepos: notImplemented("listRecentRepos"),
  getAppVersion: notImplemented("getAppVersion"),
  getLastSeenVersion: notImplemented("getLastSeenVersion"),
  setLastSeenVersion: notImplemented("setLastSeenVersion"),
  listOpenRepos: notImplemented("listOpenRepos"),
  persistOpenRepos: notImplemented("persistOpenRepos"),
  scanReposInRoot: notImplemented("scanReposInRoot"),
  listWorkspaces: notImplemented("listWorkspaces"),
  saveWorkspace: notImplemented("saveWorkspace"),
  updateWorkspace: notImplemented("updateWorkspace"),
  deleteWorkspace: notImplemented("deleteWorkspace"),
  getGraphBranchSelection: notImplemented("getGraphBranchSelection"),
  setGraphBranchSelection: notImplemented("setGraphBranchSelection"),
  getCommitFiles: notImplemented("getCommitFiles"),
  stageFile: notImplemented("stageFile"),
  unstageFile: notImplemented("unstageFile"),
  stageHunk: notImplemented("stageHunk"),
  unstageHunk: notImplemented("unstageHunk"),
  discardHunk: notImplemented("discardHunk"),
  commit: notImplemented("commit"),
  listBranches: notImplemented("listBranches"),
  createBranch: notImplemented("createBranch"),
  switchBranch: notImplemented("switchBranch"),
  deleteBranch: notImplemented("deleteBranch"),
  renameBranch: notImplemented("renameBranch"),
  listWorktrees: notImplemented("listWorktrees"),
  createWorktree: notImplemented("createWorktree"),
  removeWorktree: notImplemented("removeWorktree"),
  pruneWorktrees: notImplemented("pruneWorktrees"),
  listSubmodules: notImplemented("listSubmodules"),
  initSubmodule: notImplemented("initSubmodule"),
  updateSubmodule: notImplemented("updateSubmodule"),
  listReflogRefs: notImplemented("listReflogRefs"),
  getReflog: notImplemented("getReflog"),
  restoreReflogEntry: notImplemented("restoreReflogEntry"),
  listRemotes: notImplemented("listRemotes"),
  listRemoteBranches: notImplemented("listRemoteBranches"),
  getCurrentUpstream: notImplemented("getCurrentUpstream"),
  getRemoteUpstreams: notImplemented("getRemoteUpstreams"),
  addRemote: notImplemented("addRemote"),
  renameRemote: notImplemented("renameRemote"),
  updateRemoteUrls: notImplemented("updateRemoteUrls"),
  removeRemote: notImplemented("removeRemote"),
  saveHttpsCredential: notImplemented("saveHttpsCredential"),
  forgetHttpsCredential: notImplemented("forgetHttpsCredential"),
  setRemoteAuthMode: notImplemented("setRemoteAuthMode"),
  setCurrentUpstream: notImplemented("setCurrentUpstream"),
  clearCurrentUpstream: notImplemented("clearCurrentUpstream"),
  listTags: notImplemented("listTags"),
  createTag: notImplemented("createTag"),
  deleteTag: notImplemented("deleteTag"),
  fetchRemote: notImplemented("fetchRemote"),
  pushCurrentBranch: notImplemented("pushCurrentBranch"),
  pushTags: notImplemented("pushTags"),
  pullCurrentUpstream: notImplemented("pullCurrentUpstream"),
  subscribeTransferProgress: () => () => {},
  listStashes: notImplemented("listStashes"),
  saveStash: notImplemented("saveStash"),
  applyStash: notImplemented("applyStash"),
  dropStash: notImplemented("dropStash"),
  getBlame: notImplemented("getBlame"),
  mergeBranch: notImplemented("mergeBranch"),
  getConflictHunks: notImplemented("getConflictHunks"),
  resolveConflict: notImplemented("resolveConflict"),
  abortMerge: notImplemented("abortMerge"),
  getMergeMessage: notImplemented("getMergeMessage"),
  resolveAddDeleteConflict: notImplemented("resolveAddDeleteConflict"),
  commitsSince: notImplemented("commitsSince"),
  startRebase: notImplemented("startRebase"),
  rebaseContinue: notImplemented("rebaseContinue"),
  abortRebase: notImplemented("abortRebase"),
  getRebaseProgress: notImplemented("getRebaseProgress"),
  detectForgeRepository: notImplemented("detectForgeRepository"),
  saveForgeToken: notImplemented("saveForgeToken"),
  forgetForgeToken: notImplemented("forgetForgeToken"),
  listPullRequests: notImplemented("listPullRequests"),
  createPullRequest: notImplemented("createPullRequest"),
  openExternalUrl: notImplemented("openExternalUrl"),
};
