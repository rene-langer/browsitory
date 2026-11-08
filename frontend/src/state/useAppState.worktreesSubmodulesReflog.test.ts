import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ReflogEntry } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("useAppState", () => {
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
        workdir: "/repo/deps/child",
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
});
