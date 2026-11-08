import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RepoClient, StashEntry } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { unimplemented, TEST_REPO_PATH, remoteManagementClient, transferClient } from "./useAppStateTestClient";

describe("useAppState", () => {
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      locateCommit: async () => unimplemented(),
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      locateCommit: async () => unimplemented(),
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
      listStashes: async () => [{ index: 0, message: "WIP", commitId: "s0" }],
      saveStash: async () => unimplemented(),
      applyStash: async (_repoPath: string, index: number, commitId: string) => {
        applyStashArg = index;
        expect(commitId).toBe("s0");
        return { kind: "Applied" as const };
      },
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      locateCommit: async () => unimplemented(),
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
      listStashes: async () => [{ index: 0, message: "WIP", commitId: "s0" }],
      saveStash: async () => unimplemented(),
      applyStash: async () => unimplemented(),
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      locateCommit: async () => unimplemented(),
      getMergeOptions: async () => unimplemented(),
      setMergeOptions: async () => unimplemented(),
      dropStash: async (_repoPath: string, index: number, commitId: string) => {
        dropStashArg = index;
        expect(commitId).toBe("s0");
      },
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      locateCommit: async () => unimplemented(),
      getMergeOptions: async () => unimplemented(),
      setMergeOptions: async () => unimplemented(),
      dropStash: async () => {},
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getBranchCompareRange: async () => unimplemented(),
      locateCommit: async () => unimplemented(),
      getMergeOptions: async () => unimplemented(),
      setMergeOptions: async () => unimplemented(),
      dropStash: async () => {},
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
    act(() => result.current.selectRow({ commitId: "some-other-commit" }));

    await act(() => result.current.dropStash(0));

    expect(result.current.state.selectedRow).toEqual({ commitId: "some-other-commit" });
  });
});
