import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { BranchInfo, RepoClient } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { unimplemented, TEST_REPO_PATH, remoteManagementClient, transferClient } from "./useAppStateTestClient";

describe("useAppState", () => {
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getCommitTree: async () => unimplemented(),
      getFileAtCommit: async () => unimplemented(),
      saveFileVersion: async () => unimplemented(),
      restoreFileVersion: async () => unimplemented(),
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getCommitTree: async () => unimplemented(),
      getFileAtCommit: async () => unimplemented(),
      saveFileVersion: async () => unimplemented(),
      restoreFileVersion: async () => unimplemented(),
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getCommitTree: async () => unimplemented(),
      getFileAtCommit: async () => unimplemented(),
      saveFileVersion: async () => unimplemented(),
      restoreFileVersion: async () => unimplemented(),
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getCommitTree: async () => unimplemented(),
      getFileAtCommit: async () => unimplemented(),
      saveFileVersion: async () => unimplemented(),
      restoreFileVersion: async () => unimplemented(),
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getCommitTree: async () => unimplemented(),
      getFileAtCommit: async () => unimplemented(),
      saveFileVersion: async () => unimplemented(),
      restoreFileVersion: async () => unimplemented(),
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getCommitTree: async () => unimplemented(),
      getFileAtCommit: async () => unimplemented(),
      saveFileVersion: async () => unimplemented(),
      restoreFileVersion: async () => unimplemented(),
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
      popStash: async () => unimplemented(),
      getStashFiles: async () => unimplemented(),
      getRangeFiles: async () => unimplemented(),
      getRangeDiff: async () => unimplemented(),
      getWorkingTreeCompareFiles: async () => unimplemented(),
      getWorkingTreeCompareDiff: async () => unimplemented(),
      getCommitTree: async () => unimplemented(),
      getFileAtCommit: async () => unimplemented(),
      saveFileVersion: async () => unimplemented(),
      restoreFileVersion: async () => unimplemented(),
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
    expect(result.current.state.addRemoteDraftOpen).toBe(false);

    act(() => result.current.openAddRemoteDraft());
    expect(result.current.state.addRemoteDraftOpen).toBe(true);

    act(() => result.current.closeAddRemoteDraft());
    expect(result.current.state.addRemoteDraftOpen).toBe(false);
  });
});
