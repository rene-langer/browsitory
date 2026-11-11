import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RepoClient, StashEntry } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("useAppState", () => {
  it("refresh also populates stashes", async () => {
    const stash: StashEntry = { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1", branch: "main", timestamp: 1700000000, fileCount: 1, hasStagedChanges: false };
    const client: RepoClient = transferClient({
      listStashes: async () => [stash],
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(result.current.state.stashes).toEqual([stash]);
  });

  it("saveStash calls client.saveStash then refreshes stashes", async () => {
    let saveStashCalls = 0;
    let listStashesCalls = 0;
    const client: RepoClient = transferClient({
      listStashes: async () => {
        listStashesCalls += 1;
        return listStashesCalls === 1
          ? []
          : [{ index: 0, message: "WIP on main: abc1234 msg", commitId: "s1", branch: "main", timestamp: 1700000000, fileCount: 1, hasStagedChanges: false }];
      },
      saveStash: async () => {
        saveStashCalls += 1;
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.saveStash());

    expect(saveStashCalls).toBe(1);
    expect(result.current.state.stashes).toEqual([
      { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1", branch: "main", timestamp: 1700000000, fileCount: 1, hasStagedChanges: false },
    ]);
  });

  it("applyStash calls client.applyStash with the given index", async () => {
    let applyStashArg: number | null = null;
    const client: RepoClient = transferClient({
      listStashes: async () => [{ index: 0, message: "WIP", commitId: "s0", branch: null, timestamp: 1700000000, fileCount: 1, hasStagedChanges: false }],
      applyStash: async (_repoPath: string, index: number, commitId: string) => {
        applyStashArg = index;
        expect(commitId).toBe("s0");
        return { kind: "Applied" as const };
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.applyStash(0));

    expect(applyStashArg).toBe(0);
  });

  it("dropStash calls client.dropStash with the given index", async () => {
    let dropStashArg: number | null = null;
    const client: RepoClient = transferClient({
      listStashes: async () => [{ index: 0, message: "WIP", commitId: "s0", branch: null, timestamp: 1700000000, fileCount: 1, hasStagedChanges: false }],
      dropStash: async (_repoPath: string, index: number, commitId: string) => {
        dropStashArg = index;
        expect(commitId).toBe("s0");
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.dropStash(0));

    expect(dropStashArg).toBe(0);
  });

  it("dropStash removes the stash from state before the backend call resolves, restores it on failure", async () => {
    const stashA: StashEntry = { index: 0, message: "WIP", commitId: "abc", branch: null, timestamp: 1700000000, fileCount: 1, hasStagedChanges: false };
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
    const stashA: StashEntry = { index: 0, message: "WIP", commitId: "abc", branch: null, timestamp: 1700000000, fileCount: 1, hasStagedChanges: false };
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
    const stash: StashEntry = { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1", branch: "main", timestamp: 1700000000, fileCount: 1, hasStagedChanges: false };
    let listStashesCalls = 0;
    const client: RepoClient = transferClient({
      listStashes: async () => {
        listStashesCalls += 1;
        return listStashesCalls === 1 ? [stash] : [];
      },
      dropStash: async () => {},
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "s1" }));
    expect(result.current.state.selectedRow).toEqual({ commitId: "s1" });

    await act(() => result.current.dropStash(0));

    expect(result.current.state.selectedRow).toBe("uncommitted");
  });

  it("dropStash leaves selectedRow untouched when dropping a stash that isn't selected", async () => {
    const stash: StashEntry = { index: 0, message: "WIP on main: abc1234 msg", commitId: "s1", branch: "main", timestamp: 1700000000, fileCount: 1, hasStagedChanges: false };
    const client: RepoClient = transferClient({
      listStashes: async () => [stash],
      dropStash: async () => {},
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "some-other-commit" }));

    await act(() => result.current.dropStash(0));

    expect(result.current.state.selectedRow).toEqual({ commitId: "some-other-commit" });
  });
});
