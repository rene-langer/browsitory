import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type {
  BranchInfo,
  GraphCommit,
  RemoteInfo,
  RepoClient,
  StashEntry,
  StatusEntry,
  TagInfo,
  ReflogEntry,
  UpstreamInfo,
} from "../ipc/RepoClient";
import { useAppState } from "./useAppState";

function unimplemented(): never {
  throw new Error("not implemented in this fake");
}

const remote: RemoteInfo = { name: "origin", fetchUrl: "../origin.git", pushUrl: null, authMode: null, authUsername: null };
const upstream: UpstreamInfo = { localBranch: "main", remoteName: "origin", remoteBranch: "main" };

const remoteManagementClient = {
  listRemotes: async () => [remote],
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
};

function transferClient(overrides: Partial<RepoClient>): RepoClient {
  return {
    ...remoteManagementClient,
    pickRepoFolder: async () => unimplemented(),
    listRecentRepos: async () => unimplemented(),
    openRepo: async () => {},
    getStatus: async () => [],
    getCommitGraph: async () => [],
    getWorkingDiff: async () => unimplemented(),
    getCommitDiff: async () => unimplemented(),
    getCommitFiles: async () => unimplemented(),
    stageFile: async () => unimplemented(),
    unstageFile: async () => unimplemented(),
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
    const { result } = renderHook(() => useAppState(client));

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
      saveHttpsCredential: async (remoteName, username, token) => {
        saved = [remoteName, username, token];
      },
    });
    const { result } = renderHook(() => useAppState(client));

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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    await act(() => result.current.createTag("v1.0.0", "first release"));
    expect(result.current.state.tags).toEqual([release]);

    await act(() => result.current.deleteTag("v1.0.0"));
    expect(result.current.state.tags).toEqual([]);
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
      const { result } = renderHook(() => useAppState(client));

      await act(() => result.current.openRepo("/repo"));
      await act(() => {
        if (operation === "create") {
          return result.current.createWorktree("feature", "/repo-feature", "feature", "main");
        }
        if (operation === "remove") return result.current.removeWorktree("feature");
        return result.current.pruneWorktrees();
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
      const { result } = renderHook(() => useAppState(client));

      await act(() => result.current.openRepo("/repo"));
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
    const { result } = renderHook(() => useAppState(client));

    await act(() => result.current.openRepo("/repo"));
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
      getReflog: (reference) => new Promise<ReflogEntry[]>((resolve, reject) => {
        if (reference === "HEAD") {
          rejectHead = reject;
        } else {
          resolveFeature = resolve;
        }
      }),
    });
    const { result } = renderHook(() => useAppState(client));

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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    await act(() => result.current.pullCurrentUpstream());

    expect(statusCalls).toBe(2);
    expect(result.current.state.pendingPull).toBeNull();
    expect(result.current.state.pullOutcome).toEqual(outcome);
  });

  it("clears the last pull outcome when opening another repository", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => ({ kind: "UpToDate" }),
    });
    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    await act(() => result.current.pullCurrentUpstream());

    await act(() => result.current.openRepo("/other-repo"));

    expect(result.current.state.pullOutcome).toBeNull();
  });

  it("clears the last pull outcome when switching branches", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => ({ kind: "UpToDate" }),
      switchBranch: async () => {},
    });
    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    await act(() => result.current.pullCurrentUpstream());

    await act(() => result.current.switchBranch("feature"));

    expect(result.current.state.pullOutcome).toBeNull();
  });

  it("clears the last pull outcome when setting a different upstream", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => ({ kind: "UpToDate" }),
      setCurrentUpstream: async () => {},
    });
    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    await act(() => result.current.pullCurrentUpstream());

    await act(() => result.current.setCurrentUpstream("backup", "main"));

    expect(result.current.state.pullOutcome).toBeNull();
  });

  it("clears the last pull outcome when clearing the current upstream", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => ({ kind: "UpToDate" }),
      clearCurrentUpstream: async () => {},
    });
    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    await act(() => result.current.pullCurrentUpstream());

    await act(() => result.current.clearCurrentUpstream());

    expect(result.current.state.pullOutcome).toBeNull();
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    await act(() => result.current.pullCurrentUpstream());
    act(() => result.current.clearPendingPull());

    expect(result.current.state.pendingPull).toBeNull();
    expect(result.current.state.rebaseOnto).toBeNull();
  });

  it("reports a dirty pull without leaving reconciliation pending", async () => {
    const client = transferClient({
      pullCurrentUpstream: async () => {
        throw new Error("cannot pull with a dirty worktree");
      },
    });

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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
    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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
    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    await act(() => result.current.pushCurrentBranch("origin"));

    expect(result.current.state.error).toBe(
      "Push was rejected because the remote has newer commits. Pull or reconcile history, then try again.",
    );
    expect(result.current.state.error).not.toBe("Fetch failed");
  });

  it("ignores a former repository's transfer completion after switching repositories", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    let statusCalls = 0;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => {
        statusCalls += 1;
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
      commit: async () => unimplemented(),
      fetchRemote: async () => "op-1",
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo-a"));
    await act(() => result.current.fetchRemote("origin"));

    expect(result.current.state.transfer?.operationId).toBe("op-1");
    expect(result.current.state.pending).toBe(true);

    await act(() => result.current.openRepo("/repo-b"));
    expect(result.current.state.repoPath).toBe("/repo-b");
    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.pending).toBe(false);
    expect(statusCalls).toBe(2);

    await act(async () => {
      listener?.({ operationId: "op-1", operation: "Fetch", phase: "Completed", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
    });

    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.pending).toBe(false);
    expect(statusCalls).toBe(2);
  });

  it("openRepo populates repository state including remotes and upstream", async () => {
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));

    await act(() => result.current.openRepo("/repo"));

    expect(result.current.state.repoPath).toBe("/repo");
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));

    await act(() => result.current.openRepo("/repo"));
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
      stageFile: async (path: string) => {
        stageFileArg = path;
      },
      unstageFile: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));

    await act(() => result.current.openRepo("/repo"));
    expect(result.current.state.status).toEqual([entryA]);

    await act(() => result.current.stageFile("a.txt"));

    expect(stageFileArg).toBe("a.txt");
    expect(result.current.state.status).toEqual([]);
  });

  it("errors surface in state.error without throwing", async () => {
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {
        throw new Error("no such directory");
      },
      getStatus: async () => unimplemented(),
      getCommitGraph: async () => unimplemented(),
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));

    await act(() => result.current.openRepo("/bad"));

    expect(result.current.state.error).toBe("Error: no such directory");
  });

  it("openRepo also populates branches", async () => {
    const branch: BranchInfo = { name: "main", isCurrent: true };
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));

    await act(() => result.current.openRepo("/repo"));

    expect(result.current.state.branches).toEqual([branch]);
  });

  it("switchBranch calls client.switchBranch then refreshes branches", async () => {
    let switchArg: string | null = null;
    let branchesCallCount = 0;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
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
      switchBranch: async (name: string) => {
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));

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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
    act(() => result.current.selectRow({ commitId: "abc123" }));
    expect(result.current.state.selectedRow).toEqual({ commitId: "abc123" });

    await act(() => result.current.switchBranch("feature"));

    expect(result.current.state.selectedRow).toBe("uncommitted");
  });

  it("createBranch calls client.createBranch and clears the create-branch draft", async () => {
    let createArgs: [string, string] | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [],
      getCommitGraph: async () => [],
      listBranches: async () => [],
      createBranch: async (name: string, startPoint: string) => {
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    act(() => result.current.openCreateBranchDraft("HEAD"));

    act(() => result.current.closeCreateBranchDraft());

    expect(result.current.state.createBranchDraft).toBeNull();
  });

  it("openRepo also populates stashes", async () => {
    const stash: StashEntry = { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1" };
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));

    await act(() => result.current.openRepo("/repo"));

    expect(result.current.state.stashes).toEqual([stash]);
  });

  it("saveStash calls client.saveStash then refreshes stashes", async () => {
    let saveStashCalls = 0;
    let listStashesCalls = 0;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));

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
      applyStash: async (index: number) => {
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));

    await act(() => result.current.applyStash(0));

    expect(applyStashArg).toBe(0);
  });

  it("dropStash calls client.dropStash with the given index", async () => {
    let dropStashArg: number | null = null;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
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
      dropStash: async (index: number) => {
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));

    await act(() => result.current.dropStash(0));

    expect(dropStashArg).toBe(0);
  });

  it("dropStash resets selectedRow to uncommitted when dropping the currently selected stash", async () => {
    const stash: StashEntry = { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1" };
    let listStashesCalls = 0;
    const client: RepoClient = {
      ...remoteManagementClient,
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client));
    await act(() => result.current.openRepo("/repo"));
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
