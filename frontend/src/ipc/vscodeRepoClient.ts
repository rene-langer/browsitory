import type {
  BranchInfo,
  DiffHunk,
  GraphCommit,
  OpenRepoEntry,
  ReflogEntry,
  RepoClient,
  StatusEntry,
  SubmoduleInfo,
  Workspace,
  WorktreeInfo,
} from "./RepoClient";

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

let vscode: VsCodeWebviewApi | undefined;
let listenerRegistered = false;
let nextRequestId = 1;
// Entries are only ever removed by a matching reply (see the `message` listener below). If the
// sidecar process dies, or the host never replies, a pending entry's promise hangs forever —
// there is no timeout and no "transport closed" signal today. A future task (once the
// `extension/` host exists and can detect sidecar exit) should add either a per-request timeout
// or a mechanism to reject every pending entry when the host reports the transport is closed.
const pending = new Map<
  number,
  { resolve: (value: unknown) => void; reject: (error: Error) => void }
>();

// Lazily acquire the VSCode webview API and register the message listener on first use, rather
// than at module load, so this module stays safe to statically import into a bundle that
// doesn't inject `acquireVsCodeApi` (e.g. today's Tauri build).
function ensureInitialized(): VsCodeWebviewApi {
  if (!vscode) {
    vscode = acquireVsCodeApi();
  }
  if (!listenerRegistered) {
    listenerRegistered = true;
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
  }
  return vscode;
}

function call<T>(method: string, params: Record<string, unknown>): Promise<T> {
  const api = ensureInitialized();
  const id = nextRequestId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    api.postMessage({ jsonrpc: "2.0", id, method, params });
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
  listRecentRepos: () => call<string[]>("list_recent_repos", {}),
  getAppVersion: notImplemented("getAppVersion"),
  getLastSeenVersion: notImplemented("getLastSeenVersion"),
  setLastSeenVersion: notImplemented("setLastSeenVersion"),
  listOpenRepos: () =>
    call<{ entries: OpenRepoEntry[]; activePath: string | null }>("list_open_repos", {}),
  persistOpenRepos: (entries: OpenRepoEntry[], activePath: string | null) =>
    call<void>("persist_open_repos", { entries, activePath }),
  scanReposInRoot: (root: string) => call<string[]>("scan_repos_in_root", { root }),
  listWorkspaces: () => call<Workspace[]>("list_workspaces", {}),
  saveWorkspace: (name: string, root: string, members: string[]) =>
    call<string>("save_workspace", { name, root, members }),
  updateWorkspace: (id: string, name: string, members: string[]) =>
    call<void>("update_workspace", { id, name, members }),
  deleteWorkspace: (id: string) => call<void>("delete_workspace", { id }),
  getGraphBranchSelection: (repoPath: string) =>
    call<string[] | null>("get_graph_branch_selection", { repoPath }),
  setGraphBranchSelection: (repoPath: string, selectedBranches: string[]) =>
    call<void>("set_graph_branch_selection", { repoPath, selectedBranches }),
  getCommitFiles: (repoPath: string, commitId: string) =>
    call<string[]>("get_commit_files", { repoPath, commitId }),
  stageFile: (repoPath: string, path: string) => call<void>("stage_file", { repoPath, path }),
  unstageFile: (repoPath: string, path: string) => call<void>("unstage_file", { repoPath, path }),
  stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("stage_hunk", { repoPath, path, oldStart, newStart }),
  unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("unstage_hunk", { repoPath, path, oldStart, newStart }),
  discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("discard_hunk", { repoPath, path, oldStart, newStart }),
  commit: (repoPath: string, message: string) => call<string>("commit", { repoPath, message }),
  listBranches: (repoPath: string) => call<BranchInfo[]>("list_branches", { repoPath }),
  createBranch: (repoPath: string, name: string, startPoint: string) =>
    call<void>("create_branch", { repoPath, name, startPoint }),
  switchBranch: (repoPath: string, name: string) => call<void>("switch_branch", { repoPath, name }),
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
  subscribeTransferProgress: () => {
    console.warn(
      "vscodeRepoClient: subscribeTransferProgress is not wired up on this transport yet",
    );
    return () => {};
  },
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
