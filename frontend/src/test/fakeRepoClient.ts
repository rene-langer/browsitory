import type { GraphCommit, GraphPage, RepoClient } from "../ipc/RepoClient";

/**
 * What a fake answers for a method the test didn't override:
 * - `"throw"`: the call fails with `not used in this test: <method>`, so a test only passes when
 *   it overrides every method the code under test calls.
 * - `"empty"`: the call resolves the method's entry in {@link EMPTY_RESULTS} (an empty list, an
 *   idle state, settings at their defaults). A method without an entry still fails, so a new
 *   `RepoClient` method needs no edit here until a test wants an empty answer for it.
 */
export type FakeFallback = "throw" | "empty";

/** Throws the error an unused method of a fake throws; for overrides that must fail. */
export function unused(method = "this method"): never {
  throw new Error(`not used in this test: ${method}`);
}

/**
 * Empty answers for `"empty"` mode: what a freshly opened repository with nothing in it, and an
 * app with default settings, would return. Only methods some test relies on are listed.
 */
export const EMPTY_RESULTS: Partial<RepoClient> = {
  listRecentRepos: async () => [],
  listMissingRecentRepos: async () => [],
  removeRecentRepo: async () => {},
  listPinnedRepos: async () => [],
  pinRecentRepo: async () => {},
  unpinRecentRepo: async () => {},
  getAppVersion: async () => "0.0.0",
  getLastSeenVersion: async () => "0.0.0",
  setLastSeenVersion: async () => {},
  openRepo: async () => {},
  listOpenRepos: async () => ({ entries: [], activePath: null }),
  listRepoAliases: async () => [],
  setRepoAlias: async () => {},
  scanReposInRoot: async () => [],
  listWorkspaces: async () => [],
  saveWorkspace: async () => "workspace-id",
  updateWorkspace: async () => {},
  deleteWorkspace: async () => {},
  getGraphBranchSelection: async () => null,
  // No `getChangeStamp`: without a stamp every refresh reads everything (#685), which tests that
  // change what the fake returns between refreshes rely on; checks for changes then stay quiet.
  // Not watched: the tests that don't care see today's polling.
  watchRepo: async () => false,
  unwatchRepo: async () => {},
  getGraphRemoteBranchSelection: async () => null,
  getGraphCollapseMerges: async () => false,
  setGraphCollapseMerges: async () => {},
  getMergedCommits: async () => [],
  getWorkingDiff: async () => [],
  getCommitDiff: async () => [],
  getCommitStats: async () => [],
  getCommitDetails: async () => ({
    author: { name: "", email: "", timestamp: 0 },
    committer: { name: "", email: "", timestamp: 0 },
    branches: [],
    remoteBranches: [],
    tags: [],
    complete: true,
  }),
  searchCommits: async () => ({ commits: [], nextCursor: null, scanned: 0 }),
  cancelSearch: async () => {},
  getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
  listWorktrees: async () => [],
  listSubmodules: async () => [],
  listReflogRefs: async () => [],
  getReflog: async () => [],
  listRemotes: async () => [],
  listRemoteBranches: async () => [],
  getPullStrategy: async () => null,
  getCurrentUpstream: async () => null,
  getRemoteUpstreams: async () => [],
  listTags: async () => [],
  subscribeTransferProgress: () => () => {},
  subscribeHookProgress: () => () => {},
  subscribeRepoWatch: () => () => {},
  subscribeFileDrops: () => () => {},
  subscribeHostRequests: () => () => {},
  getMergeMessage: async () => null,
  getHeadInfo: async () => ({
    headId: null,
    detachedAt: null,
    headPushed: false,
    operation: null,
    operationLeftover: false,
    squashMessage: null,
    commitHooks: [],
    sequence: null,
    rebase: null,
    bisect: null,
    terminalSequence: null,
    terminalRebase: null,
  }),
  getUndoState: async () => ({ undo: null, redo: null }),
  undo: async () => ({ kind: "Nothing" as const }),
  redo: async () => ({ kind: "Nothing" as const }),
  detectForgeRepository: async () => [],
  openInEditor: async () => {},
  revealInFileManager: async () => {},
  openTerminal: async () => {},
  getRootFiles: async () => [],
  createRootFile: async () => {},
  openDiffTool: async () => {},
  openMergeTool: async () => "Unresolved" as const,
  markResolved: async () => "Resolved" as const,
  resolveWholeFile: async () => {},
  cherryPickMany: async () => ({ kind: "Completed" as const, applied: 0, skipped: [] }),
  revertMany: async () => ({ kind: "Completed" as const, applied: 0, skipped: [] }),
  continueSequence: async () => ({ kind: "Completed" as const, applied: 0, skipped: [] }),
  abortSequence: async () => ({ rewound: true }),
  getExternalToolSettings: async () => ({ editor: null, terminal: null }),
  setExternalToolSettings: async () => {},
  getGitTools: async () => ({ diffTool: null, mergeTool: null }),
  getFileSides: async () => ({ old: { kind: "Missing" as const }, new: { kind: "Missing" as const } }),
  getCommitTree: async () => ({ entries: [], truncated: false }),
  getFileAtCommit: async () => ({ kind: "Binary" as const, size: 0 }),
  saveFileVersion: async () => null,
  restoreFileVersion: async () => {},
  listRecentAuthors: async () => [],
  getCommitMessageSettings: async () => ({ summaryLimit: 50, bodyRuler: 72, showRuler: true }),
  setCommitMessageSettings: async () => {},
  getFetchSettings: async () => ({ autoFetchMinutes: 0 }),
  setFetchSettings: async () => {},
  getAppearanceSettings: async () => ({ codeFont: null, codeFontSize: null, uiSize: "Normal" as const, tabSize: 4, dateFormat: "Iso" as const }),
  setAppearanceSettings: async () => {},
  listFontFamilies: async () => [],
  getUpdateSettings: async () => ({ checkAutomatically: true }),
  setUpdateSettings: async () => {},
};

// The `RepoClient` methods that return synchronously; every other method returns a promise.
const SYNC_METHODS = new Set<string>(["subscribeTransferProgress", "subscribeHookProgress", "subscribeRepoWatch", "subscribeFileDrops", "subscribeHostRequests"]);

// Names other code probes on any object (promise resolution, serializers, matchers, React, the
// test runner's pretty-printer). The fake must not answer them with a method stub: `await` on a
// fake that has `then` would call it, and a printer that finds `toJSON` would call that.
const NOT_METHODS = new Set<string>(["then", "toJSON", "constructor", "asymmetricMatch", "nodeType", "tagName", "toString", "valueOf"]);

function throwingStub(method: string): (...args: unknown[]) => unknown {
  if (SYNC_METHODS.has(method)) return () => unused(method);
  return async () => unused(method);
}

/**
 * A `RepoClient` for tests: each method is the one in `overrides`, or else the `fallback`'s answer
 * (see {@link FakeFallback}). It is a `Proxy`, so a method added to `RepoClient` needs no edit
 * here. `supportsCloneAndInit` and `supportsHostGitCommands` default to `false`.
 *
 * Spreading the result (`{ ...fakeRepoClient() }`) copies only the overrides; pass every method
 * through `overrides` instead.
 */
export function fakeRepoClient(overrides: Partial<RepoClient> = {}, fallback: FakeFallback = "throw"): RepoClient {
  const target: Partial<RepoClient> = { ...overrides };
  // One stub per method, so that `client.x === client.x` holds, as it does for a real client
  // (hooks list methods in dependency arrays).
  const stubs = new Map<string, unknown>();
  return new Proxy(target, {
    get(obj, key, receiver) {
      if (typeof key !== "string" || key in obj || NOT_METHODS.has(key)) return Reflect.get(obj, key, receiver);
      if (key === "supportsCloneAndInit" || key === "supportsHostGitCommands") return false;
      if (key === "getStatusWithStamp") {
        if (!stubs.has(key)) stubs.set(key, combinedRead(receiver as RepoClient));
        return stubs.get(key);
      }
      if (fallback === "empty" && key in EMPTY_RESULTS) return EMPTY_RESULTS[key as keyof RepoClient];
      if (!stubs.has(key)) stubs.set(key, throwingStub(key));
      return stubs.get(key);
    },
  }) as RepoClient;
}

/**
 * `getStatusWithStamp` (#841) when a test doesn't override it: the fake's `getChangeStamp`, then
 * its `getStatus`, as a refresh read them before the combined read. Without a stamp (no
 * `getChangeStamp`, or it fails) each answer gets a stamp of its own, so every refresh reads
 * everything (#685) as it did without one.
 */
let unstamped = 0;
function combinedRead(client: RepoClient): RepoClient["getStatusWithStamp"] {
  return async (repoPath) => {
    const stamp = await Promise.resolve()
      .then(() => client.getChangeStamp(repoPath))
      .catch(() => {
        unstamped += 1;
        return { refs: `unstamped-${unstamped}`, workTree: `unstamped-${unstamped}` };
      });
    return { status: await client.getStatus(repoPath), stamp };
  };
}

/** A `getCommitGraph` answer (#685): `commits`, then `nextCursor` (`null`: the walk is done). */
export function graphPage(commits: GraphCommit[], nextCursor: string | null = null): GraphPage {
  return { commits, nextCursor, stale: false };
}
