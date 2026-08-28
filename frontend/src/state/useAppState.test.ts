import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type {
  BranchInfo,
  GraphCommit,
  PullRequest,
  RemoteInfo,
  RepoClient,
  StashEntry,
  StatusEntry,
  TagInfo,
  ReflogEntry,
  UpstreamInfo,
  WorktreeInfo,
} from "../ipc/RepoClient";
import { useAppState } from "./useAppState";

function unimplemented(): never {
  throw new Error("not implemented in this fake");
}

const TEST_REPO_PATH = "/repo";

const remote: RemoteInfo = { name: "origin", fetchUrl: "../origin.git", pushUrl: null, authMode: null, authUsername: null };
const upstream: UpstreamInfo = { localBranch: "main", remoteName: "origin", remoteBranch: "main" };

const remoteManagementClient = {
  closeRepo: async () => unimplemented(),
  listOpenRepos: async () => unimplemented(),
  persistOpenRepos: async () => unimplemented(),
  scanReposInRoot: async () => [],
  listWorkspaces: async () => [],
  saveWorkspace: async () => "workspace-id",
  updateWorkspace: async () => {},
  deleteWorkspace: async () => {},
  listRemotes: async () => [remote],
  listRemoteBranches: async () => unimplemented(),
  getCurrentUpstream: async () => upstream,
  getRemoteUpstreams: async () => [upstream],
  addRemote: async () => unimplemented(),
  renameRemote: async () => unimplemented(),
  updateRemoteUrls: async () => unimplemented(),
  removeRemote: async () => unimplemented(),
  saveHttpsCredential: async () => unimplemented(),
  forgetHttpsCredential: async () => unimplemented(),
  setRemoteAuthMode: async () => unimplemented(),
  setCurrentUpstream: async () => unimplemented(),
  clearCurrentUpstream: async () => unimplemented(),
    fetchRemote: async () => unimplemented(),
    pullCurrentUpstream: async () => unimplemented(),
    listTags: async () => [],
    createTag: async () => unimplemented(),
    deleteTag: async () => unimplemented(),
    pushCurrentBranch: async () => unimplemented(),
    pushTags: async () => unimplemented(),
    subscribeTransferProgress: () => () => {},
  listWorktrees: async () => [],
  createWorktree: async () => unimplemented(),
  removeWorktree: async () => unimplemented(),
  pruneWorktrees: async () => unimplemented(),
  listSubmodules: async () => [],
  initSubmodule: async () => unimplemented(),
  updateSubmodule: async () => unimplemented(),
  listReflogRefs: async () => [],
  getReflog: async () => [],
  restoreReflogEntry: async () => unimplemented(),
  detectForgeRepository: async () => [],
  saveForgeToken: async () => unimplemented(),
  forgetForgeToken: async () => unimplemented(),
  listPullRequests: async () => unimplemented(),
  createPullRequest: async () => unimplemented(),
  openExternalUrl: async () => unimplemented(),
  getGraphBranchSelection: async () => null,
  setGraphBranchSelection: async () => {},
};

function transferClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    ...remoteManagementClient,
    pickRepoFolder: async () => unimplemented(),
    listRecentRepos: async () => unimplemented(),
    getAppVersion: async () => unimplemented(),
    getLastSeenVersion: async () => unimplemented(),
    setLastSeenVersion: async () => unimplemented(),
    openRepo: async () => {},
    getStatus: async () => [],
    getCommitGraph: async () => [],
    getWorkingDiff: async () => unimplemented(),
    getCommitDiff: async () => unimplemented(),
    getCommitFiles: async () => unimplemented(),
    stageFile: async () => unimplemented(),
    unstageFile: async () => unimplemented(),
    stageHunk: async () => unimplemented(),
    unstageHunk: async () => unimplemented(),
    discardHunk: async () => unimplemented(),
    commit: async () => unimplemented(),
    listBranches: async () => [],
    createBranch: async () => unimplemented(),
    switchBranch: async () => unimplemented(),
    deleteBranch: async () => unimplemented(),
    renameBranch: async () => unimplemented(),
    listWorktrees: async () => [],
    createWorktree: async () => unimplemented(),
    removeWorktree: async () => unimplemented(),
    pruneWorktrees: async () => unimplemented(),
    listStashes: async () => [],
    saveStash: async () => unimplemented(),
    applyStash: async () => unimplemented(),
    dropStash: async () => unimplemented(),
    getBlame: async () => unimplemented(),
    mergeBranch: async () => unimplemented(),
    getConflictHunks: async () => unimplemented(),
    resolveConflict: async () => unimplemented(),
    abortMerge: async () => unimplemented(),
    getMergeMessage: async () => null,
    resolveAddDeleteConflict: async () => unimplemented(),
    commitsSince: async () => unimplemented(),
    startRebase: async () => unimplemented(),
    rebaseContinue: async () => unimplemented(),
    abortRebase: async () => unimplemented(),
    getRebaseProgress: async () => null,
    listReflogRefs: async () => [],
    getReflog: async () => [],
    restoreReflogEntry: async () => unimplemented(),
    ...overrides,
  };
}

describe("useAppState", () => {
  it("reports a failed remote-auth mutation to dependent callers", async () => {
    const client = transferClient({
      setRemoteAuthMode: async () => {
        throw new Error("credential keychain failure");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let configured = true;
    await act(async () => {
      configured = await result.current.setRemoteAuthMode("origin", "HttpsToken", "rene");
    });

    expect(configured).toBe(false);
    expect(result.current.state.error).toBe("The operating-system credential store is unavailable. Unlock it and try again.");
  });

  it("forwards a credential token directly to the client without placing it in state", async () => {
    let saved: [string, string, string] | null = null;
    const client = transferClient({
      saveHttpsCredential: async (_repoPath, remoteName, username, token) => {
        saved = [remoteName, username, token];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.saveHttpsCredential("origin", "rene", "token-123"));

    expect(saved).toEqual(["origin", "rene", "token-123"]);
    expect(JSON.stringify(result.current.state)).not.toContain("token-123");
  });

  it("refreshes tags after creating and deleting a tag", async () => {
    const release: TagInfo = {
      name: "v1.0.0",
      targetId: "abc123",
      annotated: true,
      message: "first release",
      taggerName: "Test User",
      timestamp: 0,
    };
    let tags: TagInfo[] = [];
    const client = transferClient({
      listTags: async () => tags,
      createTag: async () => {
        tags = [release];
      },
      deleteTag: async () => {
        tags = [];
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.createTag("v1.0.0", "first release"));
    expect(result.current.state.tags).toEqual([release]);

    await act(() => result.current.deleteTag("v1.0.0"));
    expect(result.current.state.tags).toEqual([]);
  });

  it("refresh loads the saved branch selection and passes it to getCommitGraph", async () => {
    let receivedSelection: string[] | null | undefined;
    const client = transferClient({
      getGraphBranchSelection: async () => ["main"],
      getCommitGraph: async (_repoPath, _limit, selectedBranches) => {
        receivedSelection = selectedBranches;
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(receivedSelection).toEqual(["main"]);
    expect(result.current.state.graphBranchSelection).toEqual(["main"]);
  });

  it("setGraphBranchSelection persists the selection and refreshes the graph with it", async () => {
    let saved: string[] | undefined;
    let selectionForGraph: string[] | null | undefined;
    const client = transferClient({
      setGraphBranchSelection: async (_repoPath, branches) => {
        saved = branches;
      },
      getGraphBranchSelection: async () => saved ?? null,
      getCommitGraph: async (_repoPath, _limit, selectedBranches) => {
        selectionForGraph = selectedBranches;
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.setGraphBranchSelection(["feature"]));

    expect(saved).toEqual(["feature"]);
    expect(selectionForGraph).toEqual(["feature"]);
    expect(result.current.state.graphBranchSelection).toEqual(["feature"]);
  });

  it.each(["create", "remove", "prune"] as const)(
    "refreshes status, graph, branches, and worktrees after the %s worktree operation",
    async (operation) => {
      let statusCalls = 0;
      let graphCalls = 0;
      let branchCalls = 0;
      let worktreeCalls = 0;
      const client = transferClient({
        getStatus: async () => {
          statusCalls += 1;
          return [];
        },
        getCommitGraph: async () => {
          graphCalls += 1;
          return [];
        },
        listBranches: async () => {
          branchCalls += 1;
          return [];
        },
        listWorktrees: async () => {
          worktreeCalls += 1;
          return [];
        },
        createWorktree: async () => {},
        removeWorktree: async () => {},
        pruneWorktrees: async () => {},
      });
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

      await act(() => result.current.refresh());
      await act(async () => {
        if (operation === "create") {
          await result.current.createWorktree("feature", "/repo-feature", "feature", "main");
          return;
        }
        if (operation === "remove") {
          await result.current.removeWorktree("feature");
          return;
        }
        await result.current.pruneWorktrees();
      });

      expect(statusCalls).toBe(2);
      expect(graphCalls).toBe(2);
      expect(branchCalls).toBe(2);
      expect(worktreeCalls).toBe(2);
    },
  );

  it.each(["init", "update"] as const)(
    "refreshes status and submodules after the %s submodule operation",
    async (operation) => {
      let statusCalls = 0;
      let submoduleCalls = 0;
      const submodule = {
        path: "deps/child",
        url: "https://example.com/child.git",
        gitlinkId: "0123456789abcdef",
        initialized: true,
        headId: "fedcba9876543210",
      };
      const client = transferClient({
        getStatus: async () => {
          statusCalls += 1;
          return [];
        },
        listSubmodules: async () => {
          submoduleCalls += 1;
          return [submodule];
        },
        initSubmodule: async () => {},
        updateSubmodule: async () => {},
      });
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

      await act(() => result.current.refresh());
      await act(() =>
        operation === "init"
          ? result.current.initSubmodule("deps/child")
          : result.current.updateSubmodule("deps/child", true),
      );

      expect(statusCalls).toBe(2);
      expect(submoduleCalls).toBe(2);
      expect(result.current.state.submodules).toEqual([submodule]);
    },
  );

  it("refreshes status, graph, branches, and reflog after restoring a reflog entry", async () => {
    let statusCalls = 0;
    let graphCalls = 0;
    let branchCalls = 0;
    let reflogCalls = 0;
    const entry: ReflogEntry = {
      reference: "HEAD",
      oldId: "1111111",
      newId: "2222222",
      committerName: "Test User",
      committerEmail: "test@example.com",
      timestamp: 1_725_000_000,
      message: "commit: second commit",
      summary: "second commit",
    };
    const client = transferClient({
      getStatus: async () => {
        statusCalls += 1;
        return [];
      },
      getCommitGraph: async () => {
        graphCalls += 1;
        return [];
      },
      listBranches: async () => {
        branchCalls += 1;
        return [];
      },
      listReflogRefs: async () => ["HEAD"],
      getReflog: async () => {
        reflogCalls += 1;
        return [entry];
      },
      restoreReflogEntry: async () => {},
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    await act(() => result.current.restoreReflogEntry("HEAD", entry.newId));

    expect(statusCalls).toBe(2);
    expect(graphCalls).toBe(2);
    expect(branchCalls).toBe(2);
    expect(reflogCalls).toBe(1);
    expect(result.current.state.reflog).toEqual([entry]);
  });

  it("keeps the newer reflog selection when an older request rejects late", async () => {
    const headEntry: ReflogEntry = {
      reference: "HEAD",
      oldId: "1111111",
      newId: "2222222",
      committerName: "Test User",
      committerEmail: "test@example.com",
      timestamp: 1_725_000_000,
      message: "commit: head",
      summary: "head",
    };
    const featureEntry: ReflogEntry = {
      reference: "refs/heads/feature",
      oldId: "3333333",
      newId: "4444444",
      committerName: "Test User",
      committerEmail: "test@example.com",
      timestamp: 1_725_000_001,
      message: "commit: feature",
      summary: "feature",
    };
    let resolveFeature!: (entries: ReflogEntry[]) => void;
    let rejectHead!: (error: Error) => void;
    const client = transferClient({
      getReflog: (_repoPath, reference) => new Promise<ReflogEntry[]>((resolve, reject) => {
        if (reference === "HEAD") {
          rejectHead = reject;
        } else {
          resolveFeature = resolve;
        }
      }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let selectHead!: Promise<void>;
    let selectFeature!: Promise<void>;
    act(() => {
      selectHead = result.current.selectReflogReference("HEAD");
      selectFeature = result.current.selectReflogReference("refs/heads/feature");
    });

    await act(async () => {
      resolveFeature([featureEntry]);
      await selectFeature;
    });
    await act(async () => {
      rejectHead(new Error("stale HEAD request"));
      await selectHead;
    });

    expect(result.current.state.selectedReflogReference).toBe("refs/heads/feature");
    expect(result.current.state.reflog).toEqual([featureEntry]);
    expect(result.current.state.error).toBeNull();
    expect(headEntry).not.toEqual(featureEntry);
  });

  it("tracks a current-branch push using its transfer operation ID", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pushCurrentBranch: async () => {
        listener?.({ operationId: "push-42", operation: "PushBranch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        return "push-42";
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pushCurrentBranch("origin"));

    expect(result.current.state.transfer).toMatchObject({ operationId: "push-42", phase: "Starting" });
    expect(result.current.state.pending).toBe(true);
  });

  it("tracks synchronous pull progress and cleans up after completion", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    let resolvePull: ((outcome: import("../ipc/RepoClient").PullOutcome) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pullCurrentUpstream: () => new Promise((resolve) => {
        resolvePull = resolve;
        listener?.({ operationId: "pull-42", operation: "Pull", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        listener?.({ operationId: "pull-42", operation: "Pull", phase: "Receiving", errorKind: null, current: 1, total: 2, receivedBytes: 128, message: null });
      }),
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    let pull: Promise<void> = Promise.resolve();
    act(() => {
      pull = result.current.pullCurrentUpstream();
    });

    expect(result.current.state.transfer).toMatchObject({ operationId: "pull-42", phase: "Receiving" });
    expect(result.current.state.pending).toBe(true);

    await act(async () => {
      listener?.({ operationId: "pull-42", operation: "Pull", phase: "Completed", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
      resolvePull?.({ kind: "UpToDate" });
      await pull;
    });

    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.pending).toBe(false);
  });

  it("accepts a pull start event that arrives after the outcome fallback", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pullCurrentUpstream: async () => ({ kind: "UpToDate" }),
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.pending).toBe(false);

    act(() => {
      listener?.({ operationId: "pull-late", operation: "Pull", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
    });

    expect(result.current.state.transfer?.operationId).toBe("pull-late");
  });

  it.each([
    { kind: "UpToDate" } as const,
    { kind: "FastForwarded", upstreamRef: "refs/remotes/origin/main" } as const,
  ])("refreshes immediately after a $kind pull", async (outcome) => {
    let statusCalls = 0;
    const client = transferClient({
      getStatus: async () => {
        statusCalls += 1;
        return [];
      },
      pullCurrentUpstream: async () => outcome,
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    expect(statusCalls).toBe(2);
    expect(result.current.state.pendingPull).toBeNull();
    expect(result.current.state.pullOutcome).toEqual(outcome);
  });

  it("clears the last pull outcome when switching branches", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => ({ kind: "UpToDate" }),
      switchBranch: async () => {},
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    await act(() => result.current.switchBranch("feature"));

    expect(result.current.state.pullOutcome).toBeNull();
  });

  it("clears the last pull outcome when setting a different upstream", async () => {
    const client = transferClient({
      listBranches: async () => [{ name: "main", isCurrent: true }],
      pullCurrentUpstream: async () => ({ kind: "UpToDate" }),
      setCurrentUpstream: async () => {},
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    await act(() => result.current.setCurrentUpstream("backup", "main"));

    expect(result.current.state.pullOutcome).toBeNull();
  });

  it("clears the last pull outcome when clearing the current upstream", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => ({ kind: "UpToDate" }),
      clearCurrentUpstream: async () => {},
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    await act(() => result.current.clearCurrentUpstream());

    expect(result.current.state.pullOutcome).toBeNull();
  });

  it("setCurrentUpstream updates state before the backend call resolves", async () => {
    const branchA: BranchInfo = { name: "main", isCurrent: true };
    let resolveSet: (() => void) | null = null;
    const client = transferClient({
      listBranches: async () => [branchA],
      getCurrentUpstream: async () => null,
      setCurrentUpstream: async () => new Promise<void>((resolve) => { resolveSet = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let setPromise!: Promise<void>;
    act(() => {
      setPromise = result.current.setCurrentUpstream("origin", "main");
    });

    expect(result.current.state.upstream).toEqual({ localBranch: "main", remoteName: "origin", remoteBranch: "main" });

    await act(async () => {
      resolveSet?.();
      await setPromise;
    });
  });

  it("clearCurrentUpstream clears state before the backend call resolves, restores it on failure", async () => {
    const branchA: BranchInfo = { name: "main", isCurrent: true };
    const client = transferClient({
      listBranches: async () => [branchA],
      getCurrentUpstream: async () => upstream,
      clearCurrentUpstream: async () => {
        throw new Error("clear failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.upstream).toEqual(upstream);

    await act(() => result.current.clearCurrentUpstream());

    expect(result.current.state.upstream).toEqual(upstream);
    expect(result.current.state.error).toBe("clear failed");
  });

  it("records a divergent pull without beginning reconciliation", async () => {
    let mergeCalls = 0;
    let rebaseCalls = 0;
    const client = transferClient({
      pullCurrentUpstream: async () => ({ kind: "Diverged", upstreamRef: "refs/remotes/origin/main" }),
      mergeBranch: async () => {
        mergeCalls += 1;
        return unimplemented();
      },
      startRebase: async () => {
        rebaseCalls += 1;
        return unimplemented();
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    expect(result.current.state.pendingPull).toEqual({ upstreamRef: "refs/remotes/origin/main" });
    expect(result.current.state.rebaseOnto).toBeNull();
    expect(mergeCalls).toBe(0);
    expect(rebaseCalls).toBe(0);
  });

  it("dismisses a divergent pull without changing the chosen reconciliation flow", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => ({ kind: "Diverged", upstreamRef: "refs/remotes/origin/main" }),
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());
    act(() => result.current.clearPendingPull());

    expect(result.current.state.pendingPull).toBeNull();
    expect(result.current.state.rebaseOnto).toBeNull();
  });

  it("opens the rebase planner with a squash preset when squashing a graph selection", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    act(() => result.current.openSquashPlanner("aaa", ["ccc", "bbb"]));

    expect(result.current.state.rebaseOnto).toBe("aaa");
    expect(result.current.state.squashPreset).toEqual(new Set(["ccc", "bbb"]));
  });

  it("clears the squash preset when the rebase planner is closed", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    act(() => result.current.openSquashPlanner("aaa", ["ccc", "bbb"]));

    act(() => result.current.closeRebasePlanner());

    expect(result.current.state.rebaseOnto).toBeNull();
    expect(result.current.state.squashPreset).toBeNull();
  });

  it("reports a dirty pull without leaving reconciliation pending", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => {
        throw new Error("cannot pull with a dirty worktree");
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    expect(result.current.state.pendingPull).toBeNull();
    expect(result.current.state.error).toBe("Commit or stash your changes before pulling.");
  });

  it("uses the terminal missing-credential event after a pull request rejects", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pullCurrentUpstream: async () => {
        listener?.({ operationId: "rejected-pull", operation: "Pull", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        throw new Error("pull failed");
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    act(() => {
      listener?.({ operationId: "rejected-pull", operation: "Pull", phase: "Failed", errorKind: "MissingCredential", current: 0, total: 0, receivedBytes: 0, message: null });
    });

    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.pending).toBe(false);
    expect(result.current.state.error).toBe("Save an HTTPS token for this remote before retrying.");
  });

  it("handles a fast fetch that completes before its operation ID reply", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    let statusCalls = 0;
    const client = transferClient({
      getStatus: async () => {
        statusCalls += 1;
        return [];
      },
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchRemote: async () => {
        listener?.({ operationId: "fast-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        listener?.({ operationId: "fast-fetch", operation: "Fetch", phase: "Completed", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        return "fast-fetch";
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchRemote("origin"));

    expect(statusCalls).toBe(2);
    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.pending).toBe(false);
  });

  it("clears a failed transfer so retry can start while retaining safe error feedback", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    let fetchCalls = 0;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchRemote: async () => {
        fetchCalls += 1;
        const operationId = `fetch-${fetchCalls}`;
        listener?.({ operationId, operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        if (fetchCalls === 1) {
          listener?.({ operationId, operation: "Fetch", phase: "Failed", errorKind: "TransferFailed", current: 0, total: 0, receivedBytes: 0, message: "https://alice:secret@example.test/repo.git" });
        }
        return operationId;
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchRemote("origin"));

    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.error).toBe("Fetch failed");
    expect(result.current.state.pending).toBe(false);

    await act(() => result.current.fetchRemote("origin"));

    expect(fetchCalls).toBe(2);
    expect(result.current.state.transfer?.operationId).toBe("fetch-2");
    expect(result.current.state.pending).toBe(true);
    expect(result.current.state.error).toBeNull();
  });

  it("guides a fetch missing an HTTPS credential toward saving a token", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchRemote: async () => {
        listener?.({ operationId: "credential-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        listener?.({
          operationId: "credential-fetch",
          operation: "Fetch",
          phase: "Failed",
          errorKind: "MissingCredential",
          current: 0,
          total: 0,
          receivedBytes: 0,
          message: null,
        });
        return "credential-fetch";
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchRemote("origin"));

    expect(result.current.state.error).toBe("Save an HTTPS token for this remote before retrying.");
  });

  it("uses the terminal missing-credential event after a direct fetch request rejects", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => { listener = next; return () => {}; },
      fetchRemote: async () => {
        listener?.({ operationId: "rejected-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        throw new Error("Fetch failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchRemote("origin"));
    act(() => listener?.({ operationId: "rejected-fetch", operation: "Fetch", phase: "Failed", errorKind: "MissingCredential", current: 0, total: 0, receivedBytes: 0, message: null }));

    expect(result.current.state.error).toBe("Save an HTTPS token for this remote before retrying.");
  });

  it("renders safe keychain and SSH-agent transfer remediation without provider diagnostics", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => { listener = next; return () => {}; },
      fetchRemote: async () => {
        listener?.({ operationId: "keychain-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        listener?.({ operationId: "keychain-fetch", operation: "Fetch", phase: "Failed", errorKind: "CredentialStoreFailure", current: 0, total: 0, receivedBytes: 0, message: null });
        return "keychain-fetch";
      },
      pushCurrentBranch: async () => {
        listener?.({ operationId: "agent-push", operation: "PushBranch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
        listener?.({ operationId: "agent-push", operation: "PushBranch", phase: "Failed", errorKind: "SshAgentFailure", current: 0, total: 0, receivedBytes: 0, message: null });
        return "agent-push";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchRemote("origin"));
    expect(result.current.state.error).toBe("The operating-system credential store is unavailable. Unlock it and try again.");
    await act(() => result.current.pushCurrentBranch("origin"));
    expect(result.current.state.error).toBe("Load a key into your SSH agent and try again.");
    expect(result.current.state.error).not.toContain("test SSH agent was invoked");
  });

  it("guides a non-fast-forward push toward pull or history reconciliation", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pushCurrentBranch: async () => {
        const operationId = "push-42";
        listener?.({
          operationId,
          operation: "PushBranch",
          phase: "Starting",
          errorKind: null,
          current: 0,
          total: 0,
          receivedBytes: 0,
          message: null,
        });
        listener?.({
          operationId,
          operation: "PushBranch",
          phase: "Failed",
          errorKind: "NonFastForward",
          current: 0,
          total: 0,
          receivedBytes: 0,
          message: null,
        });
        return operationId;
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pushCurrentBranch("origin"));

    expect(result.current.state.error).toBe(
      "Push was rejected because the remote has newer commits. Pull or reconcile history, then try again.",
    );
    expect(result.current.state.error).not.toBe("Fetch failed");
  });

  it("refresh populates repository state including remotes and upstream", async () => {
    const entry: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    const graphCommit: GraphCommit = {
      id: "abc123",
      shortId: "abc123",
      summary: "initial commit",
      authorName: "Author",
      authorEmail: "author@example.com",
      timestamp: 0,
      parentIds: [],
      branchRefs: [],
    };
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [entry],
      getCommitGraph: async () => [graphCommit],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(result.current.state.repoPath).toBe(TEST_REPO_PATH);
    expect(result.current.state.status.length).toBe(1);
    expect(result.current.state.commits.length).toBe(1);
    expect(result.current.state.remotes).toEqual([remote]);
    expect(result.current.state.upstream).toEqual(upstream);
    expect(result.current.state.remoteUpstreams).toEqual({ origin: [upstream] });
    expect(result.current.state.selectedRow).toBe("uncommitted");
  });

  it("selectRow updates selectedRow without refetching", async () => {
    let getStatusCalls = 0;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => {
        getStatusCalls += 1;
        return [];
      },
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(getStatusCalls).toBe(1);

    act(() => result.current.selectRow({ commitId: "abc123" }));

    expect(result.current.state.selectedRow).toEqual({ commitId: "abc123" });
    expect(getStatusCalls).toBe(1);
  });

  it("stageFile calls client.stageFile then refreshes status", async () => {
    const entryA: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    let getStatusCalls = 0;
    let stageFileArg: string | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => {
        getStatusCalls += 1;
        return getStatusCalls === 1 ? [entryA] : [];
      },
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async (_repoPath: string, path: string) => {
        stageFileArg = path;
      },
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(result.current.state.status).toEqual([entryA]);

    await act(() => result.current.stageFile("a.txt"));

    expect(stageFileArg).toBe("a.txt");
    expect(result.current.state.status).toEqual([]);
  });

  it("stageFile shows the file as staged before the backend call resolves, keeps it after success", async () => {
    const entryA: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    let staged = false;
    let resolveStageFile: (() => void) | null = null;
    const client = transferClient({
      getStatus: async () => [{ ...entryA, staged }],
      stageFile: async () => new Promise<void>((resolve) => {
        resolveStageFile = () => {
          staged = true;
          resolve();
        };
      }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let stagePromise!: Promise<void>;
    act(() => {
      stagePromise = result.current.stageFile("a.txt");
    });

    expect(result.current.state.status).toEqual([{ path: "a.txt", staged: true, kind: "Modified" }]);

    await act(async () => {
      resolveStageFile?.();
      await stagePromise;
    });

    expect(result.current.state.status).toEqual([{ path: "a.txt", staged: true, kind: "Modified" }]);
  });

  it("stageFile rolls back the optimistic staged flag if the backend call fails", async () => {
    const entryA: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    const client = transferClient({
      getStatus: async () => [entryA],
      stageFile: async () => {
        throw new Error("stage failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.stageFile("a.txt"));

    expect(result.current.state.status).toEqual([entryA]);
    expect(result.current.state.error).toBe("stage failed");
  });

  it("stageFile merges a partially-staged file's two status rows into one staged row", async () => {
    const unstagedPart: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    const stagedPart: StatusEntry = { path: "a.txt", staged: true, kind: "Modified" };
    const client = transferClient({
      getStatus: async () => [unstagedPart, stagedPart],
      stageFile: async () => new Promise<void>(() => {}),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    act(() => {
      void result.current.stageFile("a.txt");
    });

    expect(result.current.state.status).toEqual([{ path: "a.txt", staged: true, kind: "Modified" }]);
  });

  // DiffPane's "Stage all" used to loop the per-file `stageFile`, and every one of those is its
  // own `runMutation` — so an N-file batch cost N stage calls *plus* N full `refresh()` rounds
  // (~13 IPC reads each, plus one per remote), all serialized through the single per-repo worker
  // thread. The bulk variants still make one IPC call per path (there is no bulk backend op) but
  // do it inside a single mutation, so the whole batch refreshes once.
  it("stageAllFiles stages every path but refreshes only once for the whole batch", async () => {
    const stagedPaths: string[] = [];
    let getStatusCalls = 0;
    const client = transferClient({
      getStatus: async () => {
        getStatusCalls += 1;
        return [];
      },
      stageFile: async (_repoPath: string, path: string) => {
        stagedPaths.push(path);
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    const refreshesBefore = getStatusCalls;

    await act(() => result.current.stageAllFiles(["a.txt", "b.txt", "c.txt"]));

    expect(stagedPaths).toEqual(["a.txt", "b.txt", "c.txt"]);
    expect(getStatusCalls - refreshesBefore).toBe(1);
    expect(result.current.state.pending).toBe(false);
  });

  it("unstageAllFiles unstages every path but refreshes only once for the whole batch", async () => {
    const unstagedPaths: string[] = [];
    let getStatusCalls = 0;
    const client = transferClient({
      getStatus: async () => {
        getStatusCalls += 1;
        return [];
      },
      unstageFile: async (_repoPath: string, path: string) => {
        unstagedPaths.push(path);
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    const refreshesBefore = getStatusCalls;

    await act(() => result.current.unstageAllFiles(["a.txt", "b.txt"]));

    expect(unstagedPaths).toEqual(["a.txt", "b.txt"]);
    expect(getStatusCalls - refreshesBefore).toBe(1);
  });

  // `RemotePanel` renders the failure inline next to the Fetch URL field and keeps the typed
  // values, so it needs a per-call signal. A plain `runMutation` gives it none: it swallows the
  // error into `state.error` and always resolves.
  it("addRemote resolves to null on success and to the failure message on failure", async () => {
    const client = transferClient({
      addRemote: async (_repoPath: string, name: string) => {
        if (name === "bad") throw new Error("invalid fetch URL");
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.addRemote("good", "../good.git", null);
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.addRemote("bad", "not-a-url", null);
    });
    // No leaked `"Error: "` prefix — this string is rendered to the user verbatim.
    expect(outcome).toBe("invalid fetch URL");
    expect(result.current.state.error).toBe("invalid fetch URL");
    expect(result.current.state.pending).toBe(false);
  });

  it("addRemote adds the remote to state before the backend call resolves", async () => {
    let resolveAdd: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [],
      addRemote: async () => new Promise<void>((resolve) => { resolveAdd = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let addPromise!: Promise<string | null>;
    act(() => {
      addPromise = result.current.addRemote("upstream", "../upstream.git", null);
    });

    expect(result.current.state.remotes).toEqual([
      { name: "upstream", fetchUrl: "../upstream.git", pushUrl: null, authMode: null, authUsername: null },
    ]);

    await act(async () => {
      resolveAdd?.();
      await addPromise;
    });
  });

  it("removeRemote removes the remote from state before the backend call resolves, restores it on failure", async () => {
    const remoteA: RemoteInfo = { name: "upstream", fetchUrl: "../upstream.git", pushUrl: null, authMode: null, authUsername: null };
    const client = transferClient({
      listRemotes: async () => [remoteA],
      removeRemote: async () => {
        throw new Error("remove failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.removeRemote("upstream", false));

    expect(result.current.state.remotes).toEqual([remoteA]);
    expect(result.current.state.error).toBe("remove failed");
  });

  it("renameRemote renames the remote in state before the backend call resolves, resolves true on success", async () => {
    const remoteA: RemoteInfo = { name: "old", fetchUrl: "../r.git", pushUrl: null, authMode: null, authUsername: null };
    let resolveRename: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [remoteA],
      renameRemote: async () => new Promise<void>((resolve) => { resolveRename = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let renamePromise!: Promise<boolean>;
    act(() => {
      renamePromise = result.current.renameRemote("old", "new");
    });

    expect(result.current.state.remotes[0].name).toBe("new");

    let succeeded!: boolean;
    await act(async () => {
      resolveRename?.();
      succeeded = await renamePromise;
    });
    expect(succeeded).toBe(true);
  });

  it("renameRemote resolves false and restores the original name on failure", async () => {
    const remoteA: RemoteInfo = { name: "old", fetchUrl: "../r.git", pushUrl: null, authMode: null, authUsername: null };
    const client = transferClient({
      listRemotes: async () => [remoteA],
      renameRemote: async () => {
        throw new Error("rename failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    const succeeded = await act(() => result.current.renameRemote("old", "new"));

    expect(succeeded).toBe(false);
    expect(result.current.state.remotes).toEqual([remoteA]);
  });

  it("updateRemoteUrls updates the URLs in state before the backend call resolves", async () => {
    const remoteA: RemoteInfo = { name: "origin", fetchUrl: "../old.git", pushUrl: null, authMode: null, authUsername: null };
    let resolveUpdate: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [remoteA],
      updateRemoteUrls: async () => new Promise<void>((resolve) => { resolveUpdate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let updatePromise!: Promise<void>;
    act(() => {
      updatePromise = result.current.updateRemoteUrls("origin", "../new.git", "../push.git");
    });

    expect(result.current.state.remotes[0].fetchUrl).toBe("../new.git");
    expect(result.current.state.remotes[0].pushUrl).toBe("../push.git");

    await act(async () => {
      resolveUpdate?.();
      await updatePromise;
    });
  });

  // Same `Promise<string | null>` contract as `addRemote` above, for the same reason:
  // `BranchSwitcher`, `WorktreePanel`, and `TagPanel` each render their create-form's failure
  // inline next to the form rather than routing it through the shared banner (issue #30/UX-002).
  it("createBranch resolves to null on success and to the failure message on failure", async () => {
    const client = transferClient({
      createBranch: async (_repoPath, name) => {
        if (name === "bad") throw new Error("branch already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.createBranch("good", "HEAD");
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.createBranch("bad", "HEAD");
    });
    expect(outcome).toBe("branch already exists");
    expect(result.current.state.error).toBe("branch already exists");
  });

  it("createWorktree resolves to null on success and to the failure message on failure", async () => {
    const client = transferClient({
      createWorktree: async (_repoPath, name) => {
        if (name === "bad") throw new Error("worktree path already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.createWorktree("good", "/repo-good", "good", null);
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.createWorktree("bad", "/repo-bad", "bad", null);
    });
    expect(outcome).toBe("worktree path already exists");
    expect(result.current.state.error).toBe("worktree path already exists");
  });

  it("createWorktree adds the worktree to state before the backend call resolves", async () => {
    let resolveCreate: (() => void) | null = null;
    const client = transferClient({
      listWorktrees: async () => [],
      createWorktree: async () => new Promise<void>((resolve) => { resolveCreate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let createPromise!: Promise<string | null>;
    act(() => {
      createPromise = result.current.createWorktree("feature", "/repo/../feature", "feature-branch", null);
    });

    expect(result.current.state.worktrees).toEqual([
      { name: "feature", path: "/repo/../feature", head: null, isMain: false, isLocked: false, isPrunable: false },
    ]);

    await act(async () => {
      resolveCreate?.();
      await createPromise;
    });
  });

  it("removeWorktree removes the worktree from state before the backend call resolves, restores it on failure", async () => {
    const worktreeA: WorktreeInfo = { name: "feature", path: "/repo/../feature", head: "abc", isMain: false, isLocked: false, isPrunable: false };
    const client = transferClient({
      listWorktrees: async () => [worktreeA],
      removeWorktree: async () => {
        throw new Error("worktree has uncommitted changes");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.removeWorktree("feature"));

    expect(result.current.state.worktrees).toEqual([worktreeA]);
    expect(result.current.state.error).toBe("worktree has uncommitted changes");
  });

  it("createTag resolves to null on success and to the failure message on failure", async () => {
    const client = transferClient({
      createTag: async (_repoPath, name) => {
        if (name === "bad") throw new Error("tag already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: string | null = "not set";
    await act(async () => {
      outcome = await result.current.createTag("good", null);
    });
    expect(outcome).toBeNull();

    await act(async () => {
      outcome = await result.current.createTag("bad", null);
    });
    expect(outcome).toBe("tag already exists");
    expect(result.current.state.error).toBe("tag already exists");
  });

  it("createTag adds the tag to state before the backend call resolves", async () => {
    let resolveCreate: (() => void) | null = null;
    const client = transferClient({
      listTags: async () => [],
      createTag: async () => new Promise<void>((resolve) => { resolveCreate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let createPromise!: Promise<string | null>;
    act(() => {
      createPromise = result.current.createTag("v1.0.0", null);
    });

    expect(result.current.state.tags).toHaveLength(1);
    expect(result.current.state.tags[0].name).toBe("v1.0.0");
    expect(result.current.state.tags[0].annotated).toBe(false);

    await act(async () => {
      resolveCreate?.();
      await createPromise;
    });
  });

  it("deleteTag removes the tag from state before the backend call resolves, restores it on failure", async () => {
    const tagA: TagInfo = { name: "v1.0.0", targetId: "abc", annotated: false, message: null, taggerName: null, timestamp: null };
    const client = transferClient({
      listTags: async () => [tagA],
      deleteTag: async () => {
        throw new Error("delete failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteTag("v1.0.0"));

    expect(result.current.state.tags).toEqual([tagA]);
    expect(result.current.state.error).toBe("delete failed");
  });

  // The global banner's dismiss control (issue #30/UX-002): without it, `state.error` only ever
  // clears on the next successful action of the same kind, so a user who moves on to a different
  // panel is left with a stale-looking banner indefinitely.
  it("dismissError clears state.error without waiting for another action", async () => {
    const client = transferClient({
      createTag: async () => {
        throw new Error("tag already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.createTag("bad", null));
    expect(result.current.state.error).toBe("tag already exists");

    act(() => result.current.dismissError());

    expect(result.current.state.error).toBeNull();
  });

  it("stageHunk calls client.stageHunk then refreshes status", async () => {
    const entryA: StatusEntry = { path: "a.txt", staged: true, kind: "Modified" };
    let getStatusCalls = 0;
    let stageHunkArgs: [string, number, number] | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => {
        getStatusCalls += 1;
        return getStatusCalls === 1 ? [] : [entryA];
      },
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        stageHunkArgs = [path, oldStart, newStart];
      },
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(result.current.state.status).toEqual([]);

    await act(() => result.current.stageHunk("a.txt", 3, 4));

    expect(stageHunkArgs).toEqual(["a.txt", 3, 4]);
    expect(result.current.state.status).toEqual([entryA]);
  });

  it("unstageHunk calls client.unstageHunk with the given path and hunk identity", async () => {
    let unstageHunkArgs: [string, number, number] | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        unstageHunkArgs = [path, oldStart, newStart];
      },
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.unstageHunk("a.txt", 3, 4));

    expect(unstageHunkArgs).toEqual(["a.txt", 3, 4]);
  });

  it("discardHunk calls client.discardHunk with the given path and hunk identity", async () => {
    let discardHunkArgs: [string, number, number] | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        discardHunkArgs = [path, oldStart, newStart];
      },
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.discardHunk("a.txt", 3, 4));

    expect(discardHunkArgs).toEqual(["a.txt", 3, 4]);
  });

  it("refresh also populates branches", async () => {
    const branch: BranchInfo = { name: "main", isCurrent: true };
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [branch],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(result.current.state.branches).toEqual([branch]);
  });

  it("switchBranch calls client.switchBranch then refreshes branches", async () => {
    let switchArg: string | null = null;
    let branchesCallCount = 0;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => {
        branchesCallCount += 1;
        return branchesCallCount === 1
          ? [{ name: "main", isCurrent: true }]
          : [{ name: "feature", isCurrent: true }, { name: "main", isCurrent: false }];
      },
      createBranch: async () => unimplemented(),
      switchBranch: async (_repoPath: string, name: string) => {
        switchArg = name;
      },
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.switchBranch("feature"));

    expect(switchArg).toBe("feature");
    expect(result.current.state.branches).toEqual([
      { name: "feature", isCurrent: true },
      { name: "main", isCurrent: false },
    ]);
  });

  it("switchBranch resets selectedRow to uncommitted", async () => {
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => {},
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "abc123" }));
    expect(result.current.state.selectedRow).toEqual({ commitId: "abc123" });

    await act(() => result.current.switchBranch("feature"));

    expect(result.current.state.selectedRow).toBe("uncommitted");
  });

  it("deleteBranch removes the branch from state before the backend call resolves", async () => {
    const branchA: BranchInfo = { name: "feature", isCurrent: false };
    let branches = [branchA];
    let resolveDelete: (() => void) | null = null;
    const client = transferClient({
      listBranches: async () => branches,
      deleteBranch: async () => new Promise<void>((resolve) => {
        resolveDelete = () => {
          branches = [];
          resolve();
        };
      }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.branches).toEqual([branchA]);

    let deletePromise!: Promise<void>;
    act(() => {
      deletePromise = result.current.deleteBranch("feature", false);
    });

    expect(result.current.state.branches).toEqual([]);

    await act(async () => {
      resolveDelete?.();
      await deletePromise;
    });

    expect(result.current.state.branches).toEqual([]);
  });

  it("deleteBranch restores the branch to state if the backend call fails", async () => {
    const branchA: BranchInfo = { name: "feature", isCurrent: false };
    const client = transferClient({
      listBranches: async () => [branchA],
      deleteBranch: async () => {
        throw new Error("branch has unmerged changes");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteBranch("feature", false));

    expect(result.current.state.branches).toEqual([branchA]);
    expect(result.current.state.error).toBe("branch has unmerged changes");
  });

  it("createBranch adds a not-yet-current branch to state before the backend call resolves", async () => {
    let branches: BranchInfo[] = [];
    let resolveCreate: (() => void) | null = null;
    const client = transferClient({
      listBranches: async () => branches,
      createBranch: async () => new Promise<void>((resolve) => {
        resolveCreate = () => {
          branches = [{ name: "feature", isCurrent: false }];
          resolve();
        };
      }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let createPromise!: Promise<string | null>;
    act(() => {
      createPromise = result.current.createBranch("feature", "main");
    });

    expect(result.current.state.branches).toEqual([{ name: "feature", isCurrent: false }]);

    await act(async () => {
      resolveCreate?.();
      await createPromise;
    });

    expect(result.current.state.branches).toEqual([{ name: "feature", isCurrent: false }]);
  });

  it("createBranch resolves the failure message and removes the optimistic entry on failure", async () => {
    const client = transferClient({
      listBranches: async () => [],
      createBranch: async () => {
        throw new Error("branch already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    const failure = await act(() => result.current.createBranch("feature", "main"));

    expect(failure).toBe("branch already exists");
    expect(result.current.state.branches).toEqual([]);
  });

  it("renameBranch renames the branch in state before the backend call resolves", async () => {
    const branchA: BranchInfo = { name: "old-name", isCurrent: false };
    let resolveRename: (() => void) | null = null;
    const client = transferClient({
      listBranches: async () => [branchA],
      renameBranch: async () => new Promise<void>((resolve) => { resolveRename = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let renamePromise!: Promise<void>;
    act(() => {
      renamePromise = result.current.renameBranch("old-name", "new-name");
    });

    expect(result.current.state.branches).toEqual([{ name: "new-name", isCurrent: false }]);

    await act(async () => {
      resolveRename?.();
      await renamePromise;
    });
  });

  it("createBranch calls client.createBranch and clears the create-branch draft", async () => {
    let createArgs: [string, string] | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async (_repoPath: string, name: string, startPoint: string) => {
        createArgs = [name, startPoint];
      },
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.openCreateBranchDraft("abc123"));
    expect(result.current.state.createBranchDraft).toEqual({ startPoint: "abc123" });

    await act(() => result.current.createBranch("feature", "abc123"));

    expect(createArgs).toEqual(["feature", "abc123"]);
    expect(result.current.state.createBranchDraft).toBeNull();
  });

  it("createBranch resets selectedRow to uncommitted", async () => {
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => {},
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "abc123" }));
    expect(result.current.state.selectedRow).toEqual({ commitId: "abc123" });

    await act(() => result.current.createBranch("feature", "abc123"));

    expect(result.current.state.selectedRow).toBe("uncommitted");
  });

  it("closeCreateBranchDraft clears the draft without calling the client", async () => {
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    act(() => result.current.openCreateBranchDraft("HEAD"));

    act(() => result.current.closeCreateBranchDraft());

    expect(result.current.state.createBranchDraft).toBeNull();
  });

  it("openAddRemoteDraft/closeAddRemoteDraft toggle addRemoteDraftOpen without calling the client", async () => {
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    expect(result.current.state.addRemoteDraftOpen).toBe(false);

    act(() => result.current.openAddRemoteDraft());
    expect(result.current.state.addRemoteDraftOpen).toBe(true);

    act(() => result.current.closeAddRemoteDraft());
    expect(result.current.state.addRemoteDraftOpen).toBe(false);
  });

  it("refresh also populates stashes", async () => {
    const stash: StashEntry = { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1" };
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [stash],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(result.current.state.stashes).toEqual([stash]);
  });

  it("saveStash calls client.saveStash then refreshes stashes", async () => {
    let saveStashCalls = 0;
    let listStashesCalls = 0;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => {
        listStashesCalls += 1;
        return listStashesCalls === 1
          ? []
          : [{ index: 0, message: "WIP on main: abc1234 msg", commitId: "s1" }];
      },
      saveStash: async () => {
        saveStashCalls += 1;
      },
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.saveStash());

    expect(saveStashCalls).toBe(1);
    expect(result.current.state.stashes).toEqual([
      { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1" },
    ]);
  });

  it("applyStash calls client.applyStash with the given index", async () => {
    let applyStashArg: number | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async (_repoPath: string, index: number) => {
        applyStashArg = index;
      },
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.applyStash(0));

    expect(applyStashArg).toBe(0);
  });

  it("dropStash calls client.dropStash with the given index", async () => {
    let dropStashArg: number | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async (_repoPath: string, index: number) => {
        dropStashArg = index;
      },
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.dropStash(0));

    expect(dropStashArg).toBe(0);
  });

  it("dropStash removes the stash from state before the backend call resolves, restores it on failure", async () => {
    const stashA: StashEntry = { index: 0, message: "WIP", commitId: "abc" };
    const client = transferClient({
      listStashes: async () => [stashA],
      dropStash: async () => {
        throw new Error("drop failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.dropStash(0));

    expect(result.current.state.stashes).toEqual([stashA]);
    expect(result.current.state.error).toBe("drop failed");
  });

  it("dropStash clears the selected row optimistically when dropping the currently-selected stash", async () => {
    const stashA: StashEntry = { index: 0, message: "WIP", commitId: "abc" };
    let resolveDrop: (() => void) | null = null;
    const client = transferClient({
      listStashes: async () => [stashA],
      dropStash: async () => new Promise<void>((resolve) => { resolveDrop = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "abc" }));
    expect(result.current.state.selectedRow).toEqual({ commitId: "abc" });

    let dropPromise!: Promise<void>;
    act(() => {
      dropPromise = result.current.dropStash(0);
    });

    expect(result.current.state.stashes).toEqual([]);
    expect(result.current.state.selectedRow).toBe("uncommitted");

    await act(async () => {
      resolveDrop?.();
      await dropPromise;
    });
  });

  it("dropStash resets selectedRow to uncommitted when dropping the currently selected stash", async () => {
    const stash: StashEntry = { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1" };
    let listStashesCalls = 0;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => {
        listStashesCalls += 1;
        return listStashesCalls === 1 ? [stash] : [];
      },
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => {},
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "s1" }));
    expect(result.current.state.selectedRow).toEqual({ commitId: "s1" });

    await act(() => result.current.dropStash(0));

    expect(result.current.state.selectedRow).toBe("uncommitted");
  });

  it("dropStash leaves selectedRow untouched when dropping a stash that isn't selected", async () => {
    const stash: StashEntry = { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1" };
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [stash],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => {},
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "some-other-commit" }));

    await act(() => result.current.dropStash(0));

    expect(result.current.state.selectedRow).toEqual({ commitId: "some-other-commit" });
  });

  it("pending is true only while a mutation is in flight", async () => {
    let resolveStage: (() => void) | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      getAppVersion: async () => unimplemented(),
      getLastSeenVersion: async () => unimplemented(),
      setLastSeenVersion: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async () => unimplemented(),
      switchBranch: async () => unimplemented(),
      deleteBranch: async () => unimplemented(),
      renameBranch: async () => unimplemented(),
      listStashes: async () => [],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getRebaseProgress: async () => null,
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      stageFile: async () =>
        new Promise<void>((resolve) => {
          resolveStage = resolve;
        }),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.pending).toBe(false);

    let mutationPromise!: Promise<void>;
    act(() => {
      mutationPromise = result.current.stageFile("a.txt");
    });
    expect(result.current.state.pending).toBe(true);

    await act(async () => {
      resolveStage?.();
      await mutationPromise;
    });

    expect(result.current.state.pending).toBe(false);
  });
});

describe("useAppState pull requests", () => {
  const remoteAPullRequest: PullRequest = {
    id: "a-1",
    number: 1,
    title: "Remote A's pull request",
    url: "https://github.com/acme/a/pull/1",
    author: "rene",
    sourceBranch: "feature/a",
    targetBranch: "main",
    state: "open",
  };
  const remoteBPullRequest: PullRequest = {
    id: "b-1",
    number: 2,
    title: "Remote B's pull request",
    url: "https://github.com/acme/b/pull/2",
    author: "rene",
    sourceBranch: "feature/b",
    targetBranch: "main",
    state: "open",
  };

  it("creating a pull request on remote B does not touch remote A's already-listed rows", async () => {
    const client = transferClient({
      listPullRequests: async (_repoPath: string, remoteName: string) => ({
        pullRequests: remoteName === "remote-a" ? [remoteAPullRequest] : [],
        truncated: false,
      }),
      createPullRequest: async () => remoteBPullRequest,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.listPullRequests("remote-a", "rene"));
    expect(result.current.state.pullRequests["remote-a"].pullRequests).toEqual([remoteAPullRequest]);

    await act(() =>
      result.current.createPullRequest("remote-b", "rene", {
        title: "Remote B's pull request",
        description: null,
        sourceBranch: "feature/b",
        targetBranch: "main",
      }),
    );

    expect(result.current.state.pullRequests["remote-a"].pullRequests).toEqual([remoteAPullRequest]);
    expect(result.current.state.pullRequests["remote-b"].pullRequests).toEqual([remoteBPullRequest]);
  });

  it("a failed listing on remote B does not show remote A's stale rows, and clears remote B's own entry", async () => {
    let failNextBListing = false;
    const client = transferClient({
      listPullRequests: async (_repoPath: string, remoteName: string) => {
        if (remoteName === "remote-b" && failNextBListing) {
          throw new Error("the provider rejected the request");
        }
        return {
          pullRequests: remoteName === "remote-a" ? [remoteAPullRequest] : [remoteBPullRequest],
          truncated: false,
        };
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.listPullRequests("remote-a", "rene"));
    await act(() => result.current.listPullRequests("remote-b", "rene"));
    expect(result.current.state.pullRequests["remote-a"].pullRequests).toEqual([remoteAPullRequest]);
    expect(result.current.state.pullRequests["remote-b"].pullRequests).toEqual([remoteBPullRequest]);

    failNextBListing = true;
    await act(() => result.current.listPullRequests("remote-b", "rene"));

    // Remote A's rows are untouched by remote B's failure...
    expect(result.current.state.pullRequests["remote-a"].pullRequests).toEqual([remoteAPullRequest]);
    // ...and remote B's own stale rows are cleared rather than left looking like a successful list.
    expect(result.current.state.pullRequests["remote-b"]).toBeUndefined();
    expect(result.current.state.error).toContain("the provider rejected the request");
  });
});
