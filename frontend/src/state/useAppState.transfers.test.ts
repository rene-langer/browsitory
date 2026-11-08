import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { BranchInfo } from "../ipc/RepoClient";
import { lastFetchAt } from "../lib/autoFetch";
import { useAppState } from "./useAppState";
import { unimplemented, TEST_REPO_PATH, upstream, transferClient } from "./useAppStateTestClient";

describe("useAppState", () => {
  it("tracks a current-branch push using its transfer operation ID", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pushCurrentBranch: async () => {
        listener?.({ operationId: "push-42", operation: "PushBranch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
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
        listener?.({ operationId: "pull-42", operation: "Pull", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        listener?.({ operationId: "pull-42", operation: "Pull", phase: "Receiving", errorKind: null, current: 1, total: 2, receivedBytes: 128, message: null, hookOutput: null });
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
      listener?.({ operationId: "pull-42", operation: "Pull", phase: "Completed", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
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
      listener?.({ operationId: "pull-late", operation: "Pull", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
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

  it("opens the rebase planner with the given preset when acting on a graph selection", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    act(() =>
      result.current.openRebaseSelection(
        "aaa",
        new Map([
          ["ccc", "Squash"],
          ["bbb", "Squash"],
        ]),
      ),
    );

    expect(result.current.state.rebaseOnto).toBe("aaa");
    expect(result.current.state.rebasePreset).toEqual(
      new Map([
        ["ccc", "Squash"],
        ["bbb", "Squash"],
      ]),
    );
  });

  it("clears the rebase preset when the rebase planner is closed", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    act(() => result.current.openRebaseSelection("aaa", new Map([["ccc", "Drop"]])));

    act(() => result.current.closeRebasePlanner());

    expect(result.current.state.rebaseOnto).toBeNull();
    expect(result.current.state.rebasePreset).toBeNull();
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
        listener?.({ operationId: "rejected-pull", operation: "Pull", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        throw new Error("pull failed");
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.pullCurrentUpstream());

    act(() => {
      listener?.({ operationId: "rejected-pull", operation: "Pull", phase: "Failed", errorKind: "MissingCredential", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
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
        listener?.({ operationId: "fast-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        listener?.({ operationId: "fast-fetch", operation: "Fetch", phase: "Completed", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
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

  it("never adopts a background fetch's events as the transfer the user started (#440)", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const event = (operationId: string, operation: "FetchAll" | "BackgroundFetch", phase: "Starting" | "Completed") => ({
      operationId, operation, phase, errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null,
    });
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchAllRemotes: async () => {
        // A background fetch queued just before still reports while the user's request is pending.
        listener?.(event("background-fetch-1", "BackgroundFetch", "Starting"));
        listener?.(event("fetch-all-2", "FetchAll", "Starting"));
        return "fetch-all-2";
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchAllRemotes());
    expect(result.current.state.transfer).toMatchObject({ operationId: "fetch-all-2", operation: "FetchAll" });

    await act(async () => listener?.(event("background-fetch-1", "BackgroundFetch", "Completed")));
    expect(result.current.state.transfer).toMatchObject({ operationId: "fetch-all-2" });
    expect(result.current.state.pending).toBe(true);

    await act(async () => listener?.(event("fetch-all-2", "FetchAll", "Completed")));
    expect(result.current.state.transfer).toBeNull();
    expect(lastFetchAt(TEST_REPO_PATH)).not.toBeNull();
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
        listener?.({ operationId, operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        if (fetchCalls === 1) {
          listener?.({ operationId, operation: "Fetch", phase: "Failed", errorKind: "TransferFailed", current: 0, total: 0, receivedBytes: 0, message: "https://alice:secret@example.test/repo.git", hookOutput: null });
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

  it("reports a cancelled transfer as cancelled, not as a failure (PERF-002)", async () => {
    // A cancellation arrives on the wire as a `Failed` terminal event, but the user pressed
    // Cancel themselves: "Fetch failed" would be wrong. Reporting it (rather than treating it
    // as a completion) is also what keeps `App`'s success toast from announcing "Fetch
    // complete" for a transfer that never completed.
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchRemote: async () => {
        listener?.({ operationId: "cancelled-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        listener?.({ operationId: "cancelled-fetch", operation: "Fetch", phase: "Failed", errorKind: "Cancelled", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        return "cancelled-fetch";
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchRemote("origin"));

    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.error).toBe("Fetch cancelled.");
    expect(result.current.state.pending).toBe(false);
  });

  it("guides a fetch missing an HTTPS credential toward saving a token", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchRemote: async () => {
        listener?.({ operationId: "credential-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        listener?.({
          operationId: "credential-fetch",
          operation: "Fetch",
          phase: "Failed",
          errorKind: "MissingCredential",
          current: 0,
          total: 0,
          receivedBytes: 0,
          message: null,
          hookOutput: null,
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
        listener?.({ operationId: "rejected-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        throw new Error("Fetch failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.fetchRemote("origin"));
    act(() => listener?.({ operationId: "rejected-fetch", operation: "Fetch", phase: "Failed", errorKind: "MissingCredential", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null }));

    expect(result.current.state.error).toBe("Save an HTTPS token for this remote before retrying.");
  });

  it("renders safe keychain and SSH-agent transfer remediation without provider diagnostics", async () => {
    let listener: ((progress: import("../ipc/RepoClient").TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => { listener = next; return () => {}; },
      fetchRemote: async () => {
        listener?.({ operationId: "keychain-fetch", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        listener?.({ operationId: "keychain-fetch", operation: "Fetch", phase: "Failed", errorKind: "CredentialStoreFailure", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        return "keychain-fetch";
      },
      pushCurrentBranch: async () => {
        listener?.({ operationId: "agent-push", operation: "PushBranch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
        listener?.({ operationId: "agent-push", operation: "PushBranch", phase: "Failed", errorKind: "SshAgentFailure", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null });
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
          hookOutput: null,
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
          hookOutput: null,
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
});
