import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { TransferProgress } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient, progress } from "./useAppStateTestClient";

describe("useAppState — refs and remotes (4b)", () => {
  it("opens Push to… instead of pushing when push.default does not choose a target", async () => {
    const pushCurrentBranch = vi.fn(async () => "push-1");
    const client = transferClient({
      listBranches: async () => [{ name: "fx", isCurrent: true }],
      getCurrentUpstream: async () => ({ localBranch: "fx", remoteName: "origin", remoteBranch: "feature-x", pushBranch: null }),
      pushCurrentBranch,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushCurrentBranch("origin"));

    expect(pushCurrentBranch).not.toHaveBeenCalled();
    expect(result.current.state.pushDialog).toEqual({ localBranch: "fx" });
  });

  it("treats an upstream without pushBranch like push.default=simple", async () => {
    const pushCurrentBranch = vi.fn(async () => "push-1");
    const client = transferClient({
      listBranches: async () => [{ name: "fx", isCurrent: true }],
      getCurrentUpstream: async () => ({ localBranch: "fx", remoteName: "origin", remoteBranch: "feature-x" }),
      pushCurrentBranch,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushCurrentBranch("origin"));

    expect(pushCurrentBranch).not.toHaveBeenCalled();
    expect(result.current.state.pushDialog).toEqual({ localBranch: "fx" });
  });

  it("pushes when push.default resolves the differently named upstream", async () => {
    const pushCurrentBranch = vi.fn(async () => "push-1");
    const client = transferClient({
      listBranches: async () => [{ name: "fx", isCurrent: true }],
      getCurrentUpstream: async () => ({
        localBranch: "fx",
        remoteName: "origin",
        remoteBranch: "feature-x",
        pushBranch: "feature-x",
      }),
      pushCurrentBranch,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushCurrentBranch("origin"));

    expect(pushCurrentBranch).toHaveBeenCalledWith(TEST_REPO_PATH, "origin", false);
    expect(result.current.state.pushDialog).toBeNull();
  });

  it("offers no force push for a rejected push to an upstream push.default does not resolve", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      listBranches: async () => [{ name: "fx", isCurrent: true }],
      getCurrentUpstream: async () => ({ localBranch: "fx", remoteName: "origin", remoteBranch: "feature-x", pushBranch: null }),
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pushBranch: async () => {
        listener?.(progress("push-12", "PushBranch", "Starting"));
        listener?.(progress("push-12", "PushBranch", "Failed", { errorKind: "NonFastForward" }));
        return "push-12";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushBranch("origin", "fx", "feature-x", false));

    expect(result.current.state.forcePushOffer).toBeNull();
  });

  it("names the failed remotes after a partial fetch all, once the refresh lands", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchAllRemotes: async () => {
        listener?.(progress("fetch-all-1", "FetchAll", "Starting"));
        listener?.(progress("fetch-all-1", "FetchAll", "RemoteFailed", { errorKind: "TransferFailed", remoteName: "backup" }));
        listener?.(progress("fetch-all-1", "FetchAll", "Completed"));
        return "fetch-all-1";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.fetchAllRemotes());
    await act(async () => {});

    expect(result.current.state.error).toBe("Could not fetch from backup. The other remotes were fetched.");
    expect(result.current.state.transfer).toBeNull();
    expect(result.current.state.pending).toBe(false);
  });

  it("reports a fetch all where every remote failed", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchAllRemotes: async () => {
        listener?.(progress("fetch-all-2", "FetchAll", "Starting"));
        listener?.(progress("fetch-all-2", "FetchAll", "RemoteFailed", { errorKind: "TransferFailed", remoteName: "a" }));
        listener?.(progress("fetch-all-2", "FetchAll", "RemoteFailed", { errorKind: "MissingCredential", remoteName: "b" }));
        listener?.(progress("fetch-all-2", "FetchAll", "Failed", { errorKind: "TransferFailed" }));
        return "fetch-all-2";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.fetchAllRemotes());

    expect(result.current.state.error).toBe(
      "Could not fetch from a, b. b: Save an HTTPS token for this remote before retrying.",
    );
  });

  it("adds the credential hint per remote after a partial fetch all", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchAllRemotes: async () => {
        listener?.(progress("fetch-all-4", "FetchAll", "Starting"));
        listener?.(progress("fetch-all-4", "FetchAll", "RemoteFailed", { errorKind: "SshAgentFailure", remoteName: "ssh" }));
        listener?.(progress("fetch-all-4", "FetchAll", "RemoteFailed", { errorKind: "CredentialStoreFailure", remoteName: "https" }));
        listener?.(progress("fetch-all-4", "FetchAll", "RemoteFailed", { errorKind: "TransferFailed", remoteName: "dead" }));
        listener?.(progress("fetch-all-4", "FetchAll", "Completed"));
        return "fetch-all-4";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.fetchAllRemotes());
    await act(async () => {});

    expect(result.current.state.error).toBe(
      "Could not fetch from ssh, https, dead. The other remotes were fetched. " +
        "ssh: Load a key into your SSH agent and try again. " +
        "https: The operating-system credential store is unavailable. Unlock it and try again.",
    );
  });

  it("reports a cancelled fetch all as cancelled", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      fetchAllRemotes: async () => {
        listener?.(progress("fetch-all-3", "FetchAll", "Starting"));
        listener?.(progress("fetch-all-3", "FetchAll", "RemoteFailed", { errorKind: "TransferFailed", remoteName: "a" }));
        listener?.(progress("fetch-all-3", "FetchAll", "Failed", { errorKind: "Cancelled" }));
        return "fetch-all-3";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.fetchAllRemotes());

    expect(result.current.state.error).toBe("Fetch cancelled.");
  });

  it("explains a refused or cancelled remote branch delete", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    let errorKind: TransferProgress["errorKind"] = "RejectedRemoteRef";
    const client = transferClient({
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      deleteRemoteBranch: async () => {
        listener?.(progress("delete-remote-1", "DeleteRemoteBranch", "Starting"));
        listener?.(progress("delete-remote-1", "DeleteRemoteBranch", "Failed", { errorKind }));
        return "delete-remote-1";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteRemoteBranch("origin", "topic"));
    expect(result.current.state.error).toBe("The remote refused to delete the branch.");
    errorKind = "Cancelled";
    await act(() => result.current.deleteRemoteBranch("origin", "topic"));
    expect(result.current.state.error).toBe("Delete cancelled.");
    errorKind = "TransferFailed";
    await act(() => result.current.deleteRemoteBranch("origin", "topic"));
    expect(result.current.state.error).toBe("Deleting the remote branch failed");
  });

  it("passes the prune preference to every fetch once it is set, and null before", async () => {
    localStorage.clear();
    const fetchRemote = vi.fn(async () => "fetch-1");
    const fetchAllRemotes = vi.fn(async () => "fetch-all-1");
    const client = transferClient({ fetchRemote, fetchAllRemotes });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    expect(result.current.state.fetchPrune).toBeNull();
    await act(() => result.current.fetchRemote("origin"));
    await act(() => result.current.fetchAllRemotes());
    expect(fetchRemote).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", null);
    expect(fetchAllRemotes).toHaveBeenLastCalledWith(TEST_REPO_PATH, null);

    act(() => result.current.setFetchPrune(true));
    await act(() => result.current.fetchRemote("origin"));
    await act(() => result.current.fetchAllRemotes());
    expect(fetchRemote).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", true);
    expect(fetchAllRemotes).toHaveBeenLastCalledWith(TEST_REPO_PATH, true);
    expect(result.current.state.fetchPrune).toBe(true);
    expect(localStorage.getItem(`fetch-prune:${TEST_REPO_PATH}`)).toBe("on");

    act(() => result.current.setFetchPrune(false));
    await act(() => result.current.fetchRemote("origin"));
    expect(fetchRemote).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", false);
    localStorage.clear();
  });

  it("starts from the persisted prune preference for this repo", async () => {
    localStorage.setItem(`fetch-prune:${TEST_REPO_PATH}`, "off");
    const fetchRemote = vi.fn(async () => "fetch-1");
    const { result } = renderHook(() => useAppState(transferClient({ fetchRemote }), TEST_REPO_PATH));
    await act(() => result.current.refresh());

    expect(result.current.state.fetchPrune).toBe(false);
    await act(() => result.current.fetchRemote("origin"));
    expect(fetchRemote).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", false);
    localStorage.clear();
  });

  it("offers no force push after a rejected push of another branch", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      listBranches: async () => [
        { name: "main", isCurrent: true },
        { name: "topic", isCurrent: false },
      ],
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pushBranch: async () => {
        listener?.(progress("push-9", "PushBranch", "Starting"));
        listener?.(progress("push-9", "PushBranch", "Failed", { errorKind: "NonFastForward" }));
        return "push-9";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushBranch("origin", "topic", "topic", false));

    expect(result.current.state.forcePushOffer).toBeNull();
    expect(result.current.state.error).toMatch(/Push was rejected/);
  });

  it("offers no force push after a rejected push of the current branch to another remote branch name", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      // Default upstream: main tracks origin/main, so force push would target origin/main.
      listBranches: async () => [{ name: "main", isCurrent: true }],
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pushBranch: async () => {
        listener?.(progress("push-11", "PushBranch", "Starting"));
        listener?.(progress("push-11", "PushBranch", "Failed", { errorKind: "NonFastForward" }));
        return "push-11";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushBranch("origin", "main", "elsewhere", false));

    expect(result.current.state.forcePushOffer).toBeNull();
  });

  it("offers force push after a rejected same-name push of the current branch", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      listBranches: async () => [{ name: "main", isCurrent: true }],
      getCurrentUpstream: async () => null,
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      pushBranch: async () => {
        listener?.(progress("push-10", "PushBranch", "Starting"));
        listener?.(progress("push-10", "PushBranch", "Failed", { errorKind: "NonFastForward" }));
        return "push-10";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.pushBranch("origin", "main", "main", true));

    expect(result.current.state.forcePushOffer).toEqual({ remoteName: "origin" });
  });

  it("names the leased upstream branch when a force push with a renamed upstream is stale", async () => {
    let listener: ((p: TransferProgress) => void) | null = null;
    const client = transferClient({
      listBranches: async () => [{ name: "fx", isCurrent: true }],
      getCurrentUpstream: async () => ({
        localBranch: "fx",
        remoteName: "origin",
        remoteBranch: "feature-x",
        pushBranch: "feature-x",
      }),
      subscribeTransferProgress: (next) => {
        listener = next;
        return () => {};
      },
      forcePushCurrentBranch: async () => {
        listener?.(progress("force-push-1", "ForcePushBranch", "Starting"));
        listener?.(progress("force-push-1", "ForcePushBranch", "Failed", { errorKind: "StaleLease" }));
        return "force-push-1";
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.forcePushCurrentBranch("origin"));

    expect(result.current.state.error).toMatch(/^origin\/feature-x changed since your last fetch/);
  });

  it("closes the push and delete dialogs when their transfer starts", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    act(() => result.current.openPushDialog("topic"));
    expect(result.current.state.pushDialog).toEqual({ localBranch: "topic" });
    act(() => result.current.closePushDialog());
    expect(result.current.state.pushDialog).toBeNull();
    act(() => result.current.openPushDialog("topic"));
    await act(() => result.current.pushBranch("origin", "topic", "topic", true));
    expect(result.current.state.pushDialog).toBeNull();

    act(() => result.current.openDeleteRemoteBranchDialog("origin", "topic"));
    expect(result.current.state.deleteRemoteBranchDialog).toEqual({ remoteName: "origin", branchName: "topic" });
    act(() => result.current.closeDeleteRemoteBranchDialog());
    expect(result.current.state.deleteRemoteBranchDialog).toBeNull();
    act(() => result.current.openDeleteRemoteBranchDialog(null, null));
    await act(() => result.current.deleteRemoteBranch("origin", "topic"));
    expect(result.current.state.deleteRemoteBranchDialog).toBeNull();
  });

  it("openRemoteDialog/closeRemoteDialog toggle remoteDialog without calling the client", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    const calls = Object.values(client).filter((fn) => vi.isMockFunction(fn)).map((fn) => fn.mock.calls.length);
    expect(result.current.state.remoteDialog).toBeNull();
    act(() => result.current.openRemoteDialog("credentials", "origin"));
    expect(result.current.state.remoteDialog).toEqual({ kind: "credentials", remoteName: "origin" });
    act(() => result.current.closeRemoteDialog());
    expect(result.current.state.remoteDialog).toBeNull();
    expect(Object.values(client).filter((fn) => vi.isMockFunction(fn)).map((fn) => fn.mock.calls.length)).toEqual(calls);
  });

  it("removes a deleted branch from the saved graph selection", async () => {
    const setGraphBranchSelection = vi.fn(async () => {});
    const client = transferClient({
      listBranches: async () => [
        { name: "main", isCurrent: true },
        { name: "topic", isCurrent: false },
      ],
      getGraphBranchSelection: async () => ["main", "topic"],
      deleteBranch: async () => {},
      setGraphBranchSelection,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteBranch("topic", false));

    expect(setGraphBranchSelection).toHaveBeenCalledWith(TEST_REPO_PATH, ["main"]);
  });

  it("deleting the only selected branch shows all remaining branches again", async () => {
    const setGraphBranchSelection = vi.fn(async () => {});
    const client = transferClient({
      listBranches: async () => [
        { name: "main", isCurrent: true },
        { name: "topic", isCurrent: false },
        { name: "other", isCurrent: false },
      ],
      getGraphBranchSelection: async () => ["topic"],
      deleteBranch: async () => {},
      setGraphBranchSelection,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteBranch("topic", false));

    expect(setGraphBranchSelection).toHaveBeenCalledWith(TEST_REPO_PATH, ["main", "other"]);
  });

  it("leaves the selection alone when the delete fails", async () => {
    const setGraphBranchSelection = vi.fn(async () => {});
    const client = transferClient({
      listBranches: async () => [
        { name: "main", isCurrent: true },
        { name: "topic", isCurrent: false },
      ],
      getGraphBranchSelection: async () => ["topic"],
      deleteBranch: async () => {
        throw new Error("branch 'topic' has unmerged commits");
      },
      setGraphBranchSelection,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteBranch("topic", false));

    expect(setGraphBranchSelection).not.toHaveBeenCalled();
  });

  it("does not report a successful rename or delete as failed when the selection update fails", async () => {
    const logFrontendError = vi.fn(async () => {});
    const client = transferClient({
      listBranches: async () => [
        { name: "main", isCurrent: true },
        { name: "topic", isCurrent: false },
        { name: "old", isCurrent: false },
      ],
      getGraphBranchSelection: async () => ["topic", "old"],
      renameBranch: async () => {},
      deleteBranch: async () => {},
      setGraphBranchSelection: async () => {
        throw new Error("config locked");
      },
      logFrontendError,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.renameBranch("topic", "feature"));
    expect(result.current.state.error).toBeNull();
    await act(() => result.current.deleteBranch("old", false));
    expect(result.current.state.error).toBeNull();

    expect(logFrontendError).toHaveBeenCalledTimes(2);
    expect(logFrontendError).toHaveBeenCalledWith(expect.stringContaining("graph branch selection"), expect.any(Error));
  });

  it("renames a branch inside the saved graph selection", async () => {
    const setGraphBranchSelection = vi.fn(async () => {});
    const client = transferClient({
      listBranches: async () => [
        { name: "main", isCurrent: true },
        { name: "topic", isCurrent: false },
      ],
      getGraphBranchSelection: async () => ["topic"],
      renameBranch: async () => {},
      setGraphBranchSelection,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.renameBranch("topic", "feature"));

    expect(setGraphBranchSelection).toHaveBeenCalledWith(TEST_REPO_PATH, ["feature"]);
  });
});
