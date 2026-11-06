import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { GraphCommit, RepoClient, StatusEntry, TagInfo } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { unimplemented, TEST_REPO_PATH, remote, upstream, remoteManagementClient, transferClient } from "./useAppStateTestClient";

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

  // AUD-2026-09-05-CONC-002: the per-repo worker thread can die mid-session, and every
  // `WorkerHandle` method in `crates/repo-service/src/worker/*` then rejects every subsequent
  // call on that repo with one of two verbatim strings ("worker thread stopped" / "worker thread
  // stopped before replying" — see `branch.rs`, `status.rs`, etc.). Retrying the same action can
  // never help there (the worker is gone for good), so both should surface as a distinct
  // reconnect message instead of the raw string riding the generic error path unchanged.
  it.each([
    "worker thread stopped",
    "worker thread stopped before replying",
  ])("reports a %s failure as a reconnect message, not the raw worker error", async (workerError) => {
    const client = transferClient({
      setRemoteAuthMode: async () => {
        throw new Error(workerError);
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let configured = true;
    await act(async () => {
      configured = await result.current.setRemoteAuthMode("origin", "HttpsToken", "rene");
    });

    expect(configured).toBe(false);
    expect(result.current.state.error).toBe(
      "Connection to this repository was lost. Close and reopen the repository to reconnect.",
    );
    expect(result.current.state.error).not.toBe(workerError);
  });

  it("reports a worker-death failure hit during refresh as the same reconnect message", async () => {
    const client = transferClient({
      getStatus: async () => {
        throw new Error("worker thread stopped before replying");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(result.current.state.error).toBe(
      "Connection to this repository was lost. Close and reopen the repository to reconnect.",
    );
  });

  it("loads the undo state on refresh", async () => {
    const client = transferClient({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }) });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.undoState).toEqual({ undo: "Stage a.txt", redo: null });
  });

  it("advances refreshGeneration on every refresh, successful or not", async () => {
    let fail = false;
    const client = transferClient({
      getStatus: async () => {
        if (fail) throw new Error("boom");
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    expect(result.current.state.refreshGeneration).toBe(0);

    await act(() => result.current.refresh());
    expect(result.current.state.refreshGeneration).toBe(1);

    fail = true;
    await act(() => result.current.refresh());
    expect(result.current.state.refreshGeneration).toBe(2);
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
      commitId: "abc123",
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

  it("loadMoreHistory refetches the graph with a larger limit", async () => {
    const limits: number[] = [];
    const client = transferClient({
      getCommitGraph: async (_repoPath, limit) => {
        limits.push(limit);
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    await act(() => result.current.loadMoreHistory());

    expect(limits).toEqual([300, 600]);
    expect(result.current.state.graphLimit).toBe(600);
  });

  it("reports more history only when the graph filled its limit", async () => {
    const commitFor = (n: number) => ({
      id: `id${n}`,
      shortId: `id${n}`,
      summary: `c${n}`,
      authorName: "a",
      authorEmail: "a@b",
      timestamp: n,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    });
    const full = Array.from({ length: 300 }, (_, n) => commitFor(n));
    const client = transferClient({ getCommitGraph: async (_repoPath, limit) => full.slice(0, limit) });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(result.current.state.hasMoreHistory).toBe(true);

    const short = transferClient({ getCommitGraph: async () => full.slice(0, 10) });
    const { result: shortResult } = renderHook(() => useAppState(short, TEST_REPO_PATH));
    await act(() => shortResult.current.refresh());
    expect(shortResult.current.state.hasMoreHistory).toBe(false);
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

  it("setGraphRemoteBranchSelection persists the selection and reloads it on refresh", async () => {
    let saved: string[] | undefined;
    const client = transferClient({
      setGraphRemoteBranchSelection: async (_repoPath, branches) => {
        saved = branches;
      },
      getGraphRemoteBranchSelection: async () => saved ?? null,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.setGraphRemoteBranchSelection(["origin/feature"]));

    expect(saved).toEqual(["origin/feature"]);
    expect(result.current.state.graphRemoteBranchSelection).toEqual(["origin/feature"]);
  });

  it("setGraphRemoteBranchSelection leaves getCommitGraph's local-selection arg untouched", async () => {
    const selectedBranchesSeen: (string[] | null | undefined)[] = [];
    const client = transferClient({
      getGraphBranchSelection: async () => ["main"],
      setGraphRemoteBranchSelection: async () => {},
      getCommitGraph: async (_repoPath, _limit, selectedBranches) => {
        selectedBranchesSeen.push(selectedBranches);
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    await act(() => result.current.setGraphRemoteBranchSelection(["origin/feature"]));

    // The remote-branch selection is a separate axis from the local `selectedBranches` arg
    // `getCommitGraph` is called with — the two refreshes above must pass the identical local
    // selection regardless of the remote-selection mutation in between.
    expect(selectedBranchesSeen).toEqual([["main"], ["main"]]);
  });

  it("refresh populates repository state including remotes and upstream", async () => {
    const entry: StatusEntry = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" };
    const graphCommit: GraphCommit = {
      id: "abc123",
      shortId: "abc123",
      summary: "initial commit",
      authorName: "Author",
      authorEmail: "author@example.com",
      timestamp: 0,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      getMergeOptions: async () => unimplemented(),
      setMergeOptions: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      getHeadInfo: async () => ({ headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
      amendCommit: async () => { throw new Error("unimplemented"); },
      getUndoState: async () => ({ undo: null, redo: null }),
      undo: async () => ({ kind: "Nothing" as const }),
      redo: async () => ({ kind: "Nothing" as const }),
      cherryPick: async () => { throw new Error("unimplemented"); },
      revertCommit: async () => { throw new Error("unimplemented"); },
      resetToCommit: async () => { throw new Error("unimplemented"); },
      checkoutCommit: async () => { throw new Error("unimplemented"); },
      forcePushCurrentBranch: async () => { throw new Error("unimplemented"); },
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getFileHistory: async () => unimplemented(),
      searchCommits: async () => ({ commits: [], nextCursor: null, scanned: 0 }),
      cancelSearch: async () => {},
      getCommitRenameDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      getCommitMessage: async () => unimplemented(),
      getCommitSignature: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      stageLines: async () => unimplemented(),
      unstageLines: async () => unimplemented(),
      discardLines: async () => unimplemented(),
      discardFile: async () => unimplemented(),
      discardAllUnstaged: async () => unimplemented(),
      ignorePath: async () => unimplemented(),
      getDiffSettings: async () => unimplemented(),
      setDiffSettings: async () => unimplemented(),
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      getMergeOptions: async () => unimplemented(),
      setMergeOptions: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      getHeadInfo: async () => ({ headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
      amendCommit: async () => { throw new Error("unimplemented"); },
      getUndoState: async () => ({ undo: null, redo: null }),
      undo: async () => ({ kind: "Nothing" as const }),
      redo: async () => ({ kind: "Nothing" as const }),
      cherryPick: async () => { throw new Error("unimplemented"); },
      revertCommit: async () => { throw new Error("unimplemented"); },
      resetToCommit: async () => { throw new Error("unimplemented"); },
      checkoutCommit: async () => { throw new Error("unimplemented"); },
      forcePushCurrentBranch: async () => { throw new Error("unimplemented"); },
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getFileHistory: async () => unimplemented(),
      searchCommits: async () => ({ commits: [], nextCursor: null, scanned: 0 }),
      cancelSearch: async () => {},
      getCommitRenameDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      getCommitMessage: async () => unimplemented(),
      getCommitSignature: async () => unimplemented(),
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      stageLines: async () => unimplemented(),
      unstageLines: async () => unimplemented(),
      discardLines: async () => unimplemented(),
      discardFile: async () => unimplemented(),
      discardAllUnstaged: async () => unimplemented(),
      ignorePath: async () => unimplemented(),
      getDiffSettings: async () => unimplemented(),
      setDiffSettings: async () => unimplemented(),
      commit: async () => unimplemented(),
    };

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(getStatusCalls).toBe(1);

    act(() => result.current.selectRow({ commitId: "abc123" }));

    expect(result.current.state.selectedRow).toEqual({ commitId: "abc123" });
    expect(getStatusCalls).toBe(1);
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      getMergeOptions: async () => unimplemented(),
      setMergeOptions: async () => unimplemented(),
      dropStash: async () => unimplemented(),
      getBlame: async () => unimplemented(),
      mergeBranch: async () => unimplemented(),
      getConflictHunks: async () => unimplemented(),
      resolveConflict: async () => unimplemented(),
      abortMerge: async () => unimplemented(),
      getMergeMessage: async () => null,
      getHeadInfo: async () => ({ headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null }),
      amendCommit: async () => { throw new Error("unimplemented"); },
      getUndoState: async () => ({ undo: null, redo: null }),
      undo: async () => ({ kind: "Nothing" as const }),
      redo: async () => ({ kind: "Nothing" as const }),
      cherryPick: async () => { throw new Error("unimplemented"); },
      revertCommit: async () => { throw new Error("unimplemented"); },
      resetToCommit: async () => { throw new Error("unimplemented"); },
      checkoutCommit: async () => { throw new Error("unimplemented"); },
      forcePushCurrentBranch: async () => { throw new Error("unimplemented"); },
      resolveAddDeleteConflict: async () => unimplemented(),
      commitsSince: async () => unimplemented(),
      startRebase: async () => unimplemented(),
      rebaseContinue: async () => unimplemented(),
      abortRebase: async () => unimplemented(),
      getWorkingDiff: async () => unimplemented(),
      getCommitDiff: async () => unimplemented(),
      getFileHistory: async () => unimplemented(),
      searchCommits: async () => ({ commits: [], nextCursor: null, scanned: 0 }),
      cancelSearch: async () => {},
      getCommitRenameDiff: async () => unimplemented(),
      getCommitFiles: async () => unimplemented(),
      getCommitMessage: async () => unimplemented(),
      getCommitSignature: async () => unimplemented(),
      stageFile: async () =>
        new Promise<void>((resolve) => {
          resolveStage = resolve;
        }),
      unstageFile: async () => unimplemented(),
      stageHunk: async () => unimplemented(),
      unstageHunk: async () => unimplemented(),
      discardHunk: async () => unimplemented(),
      stageLines: async () => unimplemented(),
      unstageLines: async () => unimplemented(),
      discardLines: async () => unimplemented(),
      discardFile: async () => unimplemented(),
      discardAllUnstaged: async () => unimplemented(),
      ignorePath: async () => unimplemented(),
      getDiffSettings: async () => unimplemented(),
      setDiffSettings: async () => unimplemented(),
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
