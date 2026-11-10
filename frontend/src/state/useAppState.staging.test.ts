import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RepoClient, StatusEntry } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("useAppState", () => {
  it("stageFile calls client.stageFile then refreshes status", async () => {
    const entryA: StatusEntry = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" };
    let getStatusCalls = 0;
    let stageFileArg: string | null = null;
    const client: RepoClient = transferClient({
      getStatus: async () => {
        getStatusCalls += 1;
        return getStatusCalls === 1 ? [entryA] : [];
      },
      stageFile: async (_repoPath: string, path: string) => {
        stageFileArg = path;
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(result.current.state.status).toEqual([entryA]);

    await act(() => result.current.stageFile("a.txt"));

    expect(stageFileArg).toBe("a.txt");
    expect(result.current.state.status).toEqual([]);
  });

  it("stageFile shows the file as staged before the backend call resolves, keeps it after success", async () => {
    const entryA: StatusEntry = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" };
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

    expect(result.current.state.status).toEqual([{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }]);

    await act(async () => {
      resolveStageFile?.();
      await stagePromise;
    });

    expect(result.current.state.status).toEqual([{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }]);
  });

  it("stageFile rolls back the optimistic staged flag if the backend call fails", async () => {
    const entryA: StatusEntry = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" };
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
    const unstagedPart: StatusEntry = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" };
    const stagedPart: StatusEntry = { path: "a.txt", oldPath: null, staged: true, kind: "Modified" };
    const client = transferClient({
      getStatus: async () => [unstagedPart, stagedPart],
      stageFile: async () => new Promise<void>(() => {}),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    act(() => {
      void result.current.stageFile("a.txt");
    });

    expect(result.current.state.status).toEqual([{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }]);
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

  it("ignorePaths adds every pattern in order but refreshes only once (#449)", async () => {
    const patterns: string[] = [];
    let getStatusCalls = 0;
    const client = transferClient({
      getStatus: async () => {
        getStatusCalls += 1;
        return [];
      },
      ignorePath: async (_repoPath: string, pattern: string) => {
        patterns.push(pattern);
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    const refreshesBefore = getStatusCalls;

    await act(() => result.current.ignorePaths(["/a.log", "/b.log"]));

    expect(patterns).toEqual(["/a.log", "/b.log"]);
    expect(getStatusCalls - refreshesBefore).toBe(1);
  });

  it("stageHunk calls client.stageHunk then refreshes status", async () => {
    const entryA: StatusEntry = { path: "a.txt", oldPath: null, staged: true, kind: "Modified" };
    let getStatusCalls = 0;
    let stageHunkArgs: [string, number, number] | null = null;
    const client: RepoClient = transferClient({
      getStatus: async () => {
        getStatusCalls += 1;
        return getStatusCalls === 1 ? [] : [entryA];
      },
      stageHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        stageHunkArgs = [path, oldStart, newStart];
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(result.current.state.status).toEqual([]);

    await act(() => result.current.stageHunk("a.txt", 3, 4, { contextLines: 3, algorithm: "Default" }));

    expect(stageHunkArgs).toEqual(["a.txt", 3, 4]);
    expect(result.current.state.status).toEqual([entryA]);
  });

  it("unstageHunk calls client.unstageHunk with the given path and hunk identity", async () => {
    let unstageHunkArgs: [string, number, number] | null = null;
    const client: RepoClient = transferClient({
      unstageHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        unstageHunkArgs = [path, oldStart, newStart];
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.unstageHunk("a.txt", 3, 4, { contextLines: 3, algorithm: "Default" }));

    expect(unstageHunkArgs).toEqual(["a.txt", 3, 4]);
  });

  it("discardHunk calls client.discardHunk with the given path and hunk identity", async () => {
    let discardHunkArgs: [string, number, number] | null = null;
    const client: RepoClient = transferClient({
      discardHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        discardHunkArgs = [path, oldStart, newStart];
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.discardHunk("a.txt", 3, 4, { contextLines: 3, algorithm: "Default" }));

    expect(discardHunkArgs).toEqual(["a.txt", 3, 4]);
  });
});
