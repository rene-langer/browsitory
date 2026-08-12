import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { CommitInfo, RepoClient, StatusEntry } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";

function unimplemented(): never {
  throw new Error("not implemented in this fake");
}

describe("useAppState", () => {
  it("openRepo populates status and log and sets repoPath", async () => {
    const entry: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    const commit: CommitInfo = {
      id: "abc123",
      shortId: "abc123",
      summary: "initial commit",
      authorName: "Author",
      authorEmail: "author@example.com",
      timestamp: 0,
    };
    const client: RepoClient = {
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => [entry],
      getLog: async () => [commit],
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
    expect(result.current.state.log.length).toBe(1);
    expect(result.current.state.selectedRow).toBe("uncommitted");
  });

  it("selectRow updates selectedRow without refetching", async () => {
    let getStatusCalls = 0;
    const client: RepoClient = {
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => {
        getStatusCalls += 1;
        return [];
      },
      getLog: async () => [],
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
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {},
      getStatus: async () => {
        getStatusCalls += 1;
        return getStatusCalls === 1 ? [entryA] : [];
      },
      getLog: async () => [],
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
      pickRepoFolder: async () => unimplemented(),
      listRecentRepos: async () => unimplemented(),
      openRepo: async () => {
        throw new Error("no such directory");
      },
      getStatus: async () => unimplemented(),
      getLog: async () => unimplemented(),
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
});
