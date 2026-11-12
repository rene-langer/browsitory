import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FileHistoryEntry, FileHistoryPage, RepoClient } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import { FILE_HISTORY_PAGE_SIZE, useFileHistory } from "./useFileHistory";
import { fakeRepoClient } from "../test/fakeRepoClient";

function entry(id: string, path = "f.txt", oldPath: string | null = null): FileHistoryEntry {
  return {
    commit: {
      id,
      shortId: id.slice(0, 7),
      summary: `commit ${id}`,
      authorName: "A",
      authorEmail: "a@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    },
    path,
    oldPath,
  };
}

/** A `getFileHistory` answer: the whole history unless `complete` says otherwise. */
function page(entries: FileHistoryEntry[], complete = true): FileHistoryPage {
  return { entries, complete };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function setup(getFileHistory: RepoClient["getFileHistory"], selectedRow: SelectedRow = "uncommitted") {
  const client = fakeRepoClient({ getFileHistory });
  const selectRow = vi.fn();
  const hook = renderHook(
    ({ generation, row }: { generation: number; row: SelectedRow }) =>
      useFileHistory(client, "/repo", generation, row, selectRow),
    { initialProps: { generation: 0, row: selectedRow } },
  );
  return { ...hook, selectRow };
}

describe("useFileHistory", () => {
  it("opens with one page and selects the newest entry", async () => {
    const getFileHistory = vi.fn(async () => page([entry("c2"), entry("c1")]));
    const { result, selectRow } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));

    await waitFor(() => expect(result.current.history?.loading).toBe(false));
    expect(getFileHistory).toHaveBeenCalledWith("/repo", "f.txt", FILE_HISTORY_PAGE_SIZE);
    expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c2", "c1"]);
    expect(result.current.history?.hasMore).toBe(false);
    expect(selectRow).toHaveBeenCalledWith({ commitId: "c2" });
  });

  it("close clears history and restores the row selected before opening", async () => {
    const { result, selectRow } = setup(vi.fn(async () => page([entry("c1")])), { commitId: "before" });

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.loading).toBe(false));
    act(() => result.current.close());

    expect(result.current.history).toBeNull();
    expect(selectRow).toHaveBeenLastCalledWith({ commitId: "before" });
  });

  it("loadMore re-requests with a larger limit and reports hasMore", async () => {
    const full = Array.from({ length: FILE_HISTORY_PAGE_SIZE }, (_, i) => entry(`c${i}`));
    // A full page with more to come, then the end of the history.
    const getFileHistory = vi
      .fn<RepoClient["getFileHistory"]>()
      .mockResolvedValueOnce(page(full, false))
      .mockResolvedValue(page(full));
    const { result } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.hasMore).toBe(true));
    act(() => result.current.loadMore());

    await waitFor(() =>
      expect(getFileHistory).toHaveBeenLastCalledWith("/repo", "f.txt", FILE_HISTORY_PAGE_SIZE * 2),
    );
  });

  it("drops a stale response when another file was opened meanwhile", async () => {
    const forA = deferred<FileHistoryPage>();
    const forB = deferred<FileHistoryPage>();
    const getFileHistory = vi.fn((_repo: string, path: string) => (path === "a.txt" ? forA.promise : forB.promise));
    const { result } = setup(getFileHistory);

    act(() => result.current.open("a.txt"));
    act(() => result.current.open("b.txt"));
    await act(async () => forB.resolve(page([entry("b1", "b.txt")])));
    await act(async () => forA.resolve(page([entry("a1", "a.txt")])));

    expect(result.current.history?.path).toBe("b.txt");
    expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["b1"]);
  });

  it("refetches the open history when the repo refreshes", async () => {
    const getFileHistory = vi
      .fn<RepoClient["getFileHistory"]>()
      .mockResolvedValueOnce(page([entry("c1")]))
      .mockResolvedValueOnce(page([entry("c2"), entry("c1")]));
    const { result, rerender } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.entries).toHaveLength(1));
    rerender({ generation: 1, row: { commitId: "c1" } });

    await waitFor(() => expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c2", "c1"]));
  });

  it("does nothing on refresh while closed", () => {
    const getFileHistory = vi.fn(async () => page([]));
    const { rerender } = setup(getFileHistory);

    rerender({ generation: 1, row: "uncommitted" });

    expect(getFileHistory).not.toHaveBeenCalled();
  });

  it("keeps the error and retries with the same path and limit", async () => {
    const getFileHistory = vi
      .fn<RepoClient["getFileHistory"]>()
      .mockRejectedValueOnce(new Error("Path must be relative to the repository root."))
      .mockResolvedValueOnce(page([entry("c1")]));
    const { result } = setup(getFileHistory);

    act(() => result.current.open("../x"));
    await waitFor(() => expect(result.current.history?.error?.message).toContain("relative"));
    act(() => result.current.retry());

    await waitFor(() => expect(result.current.history?.error).toBeNull());
    expect(getFileHistory).toHaveBeenLastCalledWith("/repo", "../x", FILE_HISTORY_PAGE_SIZE);
  });

  describe("paging within the backend's time budget (#860)", () => {
    it("keeps asking until the page is full or the history ends, showing entries as they come", async () => {
      const second = deferred<FileHistoryPage>();
      const getFileHistory = vi
        .fn<RepoClient["getFileHistory"]>()
        .mockResolvedValueOnce(page([entry("c3")], false))
        .mockReturnValueOnce(second.promise)
        .mockResolvedValueOnce(page([entry("c3"), entry("c2"), entry("c1")]));
      const { result, selectRow } = setup(getFileHistory);

      act(() => result.current.open("f.txt"));
      await waitFor(() => expect(getFileHistory).toHaveBeenCalledTimes(2));
      expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c3"]);
      expect(result.current.history?.loading).toBe(true);
      expect(selectRow).toHaveBeenCalledWith({ commitId: "c3" });
      await act(async () => second.resolve(page([entry("c3"), entry("c2")], false)));

      await waitFor(() => expect(result.current.history?.loading).toBe(false));
      expect(getFileHistory).toHaveBeenCalledTimes(3);
      for (const call of getFileHistory.mock.calls) expect(call).toEqual(["/repo", "f.txt", FILE_HISTORY_PAGE_SIZE]);
      expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c3", "c2", "c1"]);
      expect(result.current.history?.hasMore).toBe(false);
      expect(selectRow).toHaveBeenCalledTimes(1);
    });

    it("stops asking once the history is closed", async () => {
      const second = deferred<FileHistoryPage>();
      const getFileHistory = vi
        .fn<RepoClient["getFileHistory"]>()
        .mockResolvedValueOnce(page([entry("c2")], false))
        .mockReturnValueOnce(second.promise)
        .mockResolvedValue(page([entry("c2"), entry("c1")]));
      const { result } = setup(getFileHistory);

      act(() => result.current.open("f.txt"));
      await waitFor(() => expect(getFileHistory).toHaveBeenCalledTimes(2));
      act(() => result.current.close());
      await act(async () => second.resolve(page([entry("c2")], false)));

      expect(getFileHistory).toHaveBeenCalledTimes(2);
      expect(result.current.history).toBeNull();
    });

    it("stops asking for the old file once another one is opened", async () => {
      const second = deferred<FileHistoryPage>();
      const getFileHistory = vi.fn((_repo: string, path: string) =>
        path === "b.txt" ? Promise.resolve(page([entry("b1", "b.txt")])) : getFileHistory.mock.calls.length <= 1
          ? Promise.resolve(page([entry("a2", "a.txt")], false))
          : second.promise,
      );
      const { result } = setup(getFileHistory);

      act(() => result.current.open("a.txt"));
      await waitFor(() => expect(getFileHistory).toHaveBeenCalledTimes(2));
      act(() => result.current.open("b.txt"));
      await waitFor(() => expect(result.current.history?.loading).toBe(false));
      await act(async () => second.resolve(page([entry("a2", "a.txt")], false)));

      expect(getFileHistory.mock.calls.map((call) => call[1])).toEqual(["a.txt", "a.txt", "b.txt"]);
      expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["b1"]);
    });

    it("offers Load more only for a full page of an incomplete history", async () => {
      const full = Array.from({ length: FILE_HISTORY_PAGE_SIZE }, (_, i) => entry(`c${i}`));
      const { result } = setup(vi.fn(async () => page(full, true)));

      act(() => result.current.open("f.txt"));

      await waitFor(() => expect(result.current.history?.loading).toBe(false));
      expect(result.current.history?.entries).toHaveLength(FILE_HISTORY_PAGE_SIZE);
      expect(result.current.history?.hasMore).toBe(false);
    });

    it("continues a line history the same way", async () => {
      const line = (id: string) => ({ ...entry(id), startLine: 1, endLine: 2 });
      const getLineHistory = vi
        .fn<RepoClient["getLineHistory"]>()
        .mockResolvedValueOnce({ entries: [line("c2")], truncated: false, complete: false })
        .mockResolvedValueOnce({ entries: [line("c2"), line("c1")], truncated: false, complete: true });
      const client = fakeRepoClient({ getLineHistory });
      const { result } = renderHook(() => useFileHistory(client, "/repo", 0, "uncommitted", vi.fn()));

      act(() => result.current.openLines("f.txt", "HEAD", 1, 2));

      await waitFor(() => expect(result.current.history?.loading).toBe(false));
      expect(getLineHistory).toHaveBeenCalledTimes(2);
      expect(getLineHistory).toHaveBeenLastCalledWith("/repo", "HEAD", "f.txt", 1, 2, FILE_HISTORY_PAGE_SIZE);
      expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c2", "c1"]);
    });
  });

  describe("line history (#447)", () => {
    function setupLines(getLineHistory: RepoClient["getLineHistory"], getFileHistory: RepoClient["getFileHistory"] = vi.fn()) {
      const client = fakeRepoClient({ getFileHistory, getLineHistory });
      const selectRow = vi.fn();
      const hook = renderHook(
        ({ generation }: { generation: number }) => useFileHistory(client, "/repo", generation, "uncommitted", selectRow),
        { initialProps: { generation: 0 } },
      );
      return { ...hook, selectRow };
    }

    it("loads the lines' history at the revision and selects the newest entry", async () => {
      const getLineHistory = vi.fn(async () => ({
        entries: [{ ...entry("c2"), startLine: 3, endLine: 4 }],
        truncated: false,
        complete: true,
      }));
      const { result, selectRow } = setupLines(getLineHistory);

      act(() => result.current.openLines("f.txt", "abc", 3, 4));

      await waitFor(() => expect(result.current.history?.loading).toBe(false));
      expect(getLineHistory).toHaveBeenCalledWith("/repo", "abc", "f.txt", 3, 4, FILE_HISTORY_PAGE_SIZE);
      expect(result.current.history?.target).toEqual({ kind: "lines", path: "f.txt", revision: "abc", startLine: 3, endLine: 4 });
      expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c2"]);
      expect(selectRow).toHaveBeenCalledWith({ commitId: "c2" });
    });

    it("offers no more pages once the backend stopped at its cap", async () => {
      const full = Array.from({ length: FILE_HISTORY_PAGE_SIZE }, (_, i) => ({ ...entry(`c${i}`), startLine: 1, endLine: 1 }));
      const { result } = setupLines(vi.fn(async () => ({ entries: full, truncated: true, complete: true })));

      act(() => result.current.openLines("f.txt", "HEAD", 1, 1));

      await waitFor(() => expect(result.current.history?.loading).toBe(false));
      expect(result.current.history?.truncated).toBe(true);
      expect(result.current.history?.hasMore).toBe(false);
    });

    it("loads more and reloads on refresh with the same lines", async () => {
      const full = Array.from({ length: FILE_HISTORY_PAGE_SIZE }, (_, i) => ({ ...entry(`c${i}`), startLine: 1, endLine: 1 }));
      // A full page with more to come, then the end of the history.
      const getLineHistory = vi
        .fn<RepoClient["getLineHistory"]>()
        .mockResolvedValueOnce({ entries: full, truncated: false, complete: false })
        .mockResolvedValue({ entries: full, truncated: false, complete: true });
      const { result, rerender } = setupLines(getLineHistory);

      act(() => result.current.openLines("f.txt", "HEAD", 1, 2));
      await waitFor(() => expect(result.current.history?.hasMore).toBe(true));
      act(() => result.current.loadMore());
      await waitFor(() =>
        expect(getLineHistory).toHaveBeenLastCalledWith("/repo", "HEAD", "f.txt", 1, 2, FILE_HISTORY_PAGE_SIZE * 2),
      );
      rerender({ generation: 1 });
      await waitFor(() => expect(getLineHistory).toHaveBeenCalledTimes(3));
      expect(getLineHistory).toHaveBeenLastCalledWith("/repo", "HEAD", "f.txt", 1, 2, FILE_HISTORY_PAGE_SIZE * 2);
    });

    it("switching from a file's history to its lines drops the file's entries", async () => {
      const lines = deferred<{ entries: never[]; truncated: boolean; complete: boolean }>();
      const getFileHistory = vi.fn(async () => page([entry("c1")]));
      const { result } = setupLines(
        vi.fn(() => lines.promise),
        getFileHistory,
      );

      act(() => result.current.open("f.txt"));
      await waitFor(() => expect(result.current.history?.entries).toHaveLength(1));
      act(() => result.current.openLines("f.txt", "HEAD", 1, 1));

      expect(result.current.history?.entries).toEqual([]);
      expect(result.current.history?.loading).toBe(true);
    });
  });
});
