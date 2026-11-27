import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ReflogEntry } from "../ipc/RepoClient";
import { REFLOG_PAGE_SIZE, useAppState } from "./useAppState";
import { graphPage } from "../test/fakeRepoClient";
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
          return graphPage([]);
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
        return graphPage([]);
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

  // #1161: a reflog can hold hundreds of thousands of entries; the section reads the newest page.
  describe("reading the reflog in pages", () => {
    function reflogEntry(n: number): ReflogEntry {
      return {
        reference: "HEAD",
        oldId: `old${n}`,
        newId: `new${n}`,
        committerName: "Test User",
        committerEmail: "test@example.com",
        timestamp: 1_725_000_000 - n,
        message: `commit: ${n}`,
        summary: `${n}`,
      };
    }
    // A reflog of `total` entries, newest first; each read asks for `limit` of them.
    function pagedReflog(total: number) {
      let refs = "r1";
      const calls: { reference: string; limit: number }[] = [];
      const client = transferClient({
        getChangeStamp: async () => ({ refs, workTree: "w" }),
        listReflogRefs: async () => ["HEAD"],
        getReflog: async (_repoPath, reference, limit) => {
          calls.push({ reference, limit });
          return Array.from({ length: Math.min(limit, total) }, (_, n) => reflogEntry(n));
        },
      });
      return { client, calls, moveRefs: () => (refs = `${refs}+`) };
    }

    it("reads the newest page, and one entry more to know whether older ones follow", async () => {
      const { client, calls } = pagedReflog(REFLOG_PAGE_SIZE * 3);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

      await act(() => result.current.selectReflogReference("HEAD"));

      expect(calls).toEqual([{ reference: "HEAD", limit: REFLOG_PAGE_SIZE + 1 }]);
      expect(result.current.state.reflog).toHaveLength(REFLOG_PAGE_SIZE);
      expect(result.current.state.reflog[0]).toEqual(reflogEntry(0));
      expect(result.current.state.reflogHasMore).toBe(true);
    });

    it("loads the next page on request, and knows when the reflog ends", async () => {
      const { client, calls } = pagedReflog(REFLOG_PAGE_SIZE + 5);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => result.current.selectReflogReference("HEAD"));

      await act(() => result.current.loadMoreReflog());

      expect(calls.at(-1)).toEqual({ reference: "HEAD", limit: REFLOG_PAGE_SIZE * 2 + 1 });
      expect(result.current.state.reflog).toHaveLength(REFLOG_PAGE_SIZE + 5);
      expect(result.current.state.reflogHasMore).toBe(false);
    });

    it("a refresh whose references didn't move keeps the reflog it has", async () => {
      const { client, calls, moveRefs } = pagedReflog(REFLOG_PAGE_SIZE * 3);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => result.current.refresh());
      await act(() => result.current.selectReflogReference("HEAD"));
      await act(() => result.current.loadMoreReflog());
      calls.length = 0;

      // Staging a file, an edit the watcher saw, the fallback poll: no reflog read.
      await act(() => result.current.refresh());
      expect(calls).toEqual([]);
      expect(result.current.state.reflog).toHaveLength(REFLOG_PAGE_SIZE * 2);

      // A reference moved: the pages loaded are read again.
      moveRefs();
      await act(() => result.current.refresh());
      expect(calls).toEqual([{ reference: "HEAD", limit: REFLOG_PAGE_SIZE * 2 + 1 }]);

      // The Refresh command reads it whatever the stamp says.
      calls.length = 0;
      await act(() => result.current.refresh({ full: true }));
      expect(calls).toEqual([{ reference: "HEAD", limit: REFLOG_PAGE_SIZE * 2 + 1 }]);
    });

    it("selecting another reference starts again at the newest page", async () => {
      const { client, calls } = pagedReflog(REFLOG_PAGE_SIZE * 3);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => result.current.selectReflogReference("HEAD"));
      await act(() => result.current.loadMoreReflog());

      await act(() => result.current.selectReflogReference("refs/heads/main"));

      expect(calls.at(-1)).toEqual({ reference: "refs/heads/main", limit: REFLOG_PAGE_SIZE + 1 });
    });
  });
});
