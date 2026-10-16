import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FileHistoryEntry, RepoClient } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import { FILE_HISTORY_PAGE_SIZE, useFileHistory } from "./useFileHistory";

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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function setup(getFileHistory: RepoClient["getFileHistory"], selectedRow: SelectedRow = "uncommitted") {
  const client = { getFileHistory } as unknown as RepoClient;
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
    const getFileHistory = vi.fn(async () => [entry("c2"), entry("c1")]);
    const { result, selectRow } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));

    await waitFor(() => expect(result.current.history?.loading).toBe(false));
    expect(getFileHistory).toHaveBeenCalledWith("/repo", "f.txt", FILE_HISTORY_PAGE_SIZE);
    expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c2", "c1"]);
    expect(result.current.history?.hasMore).toBe(false);
    expect(selectRow).toHaveBeenCalledWith({ commitId: "c2" });
  });

  it("close clears history and restores the row selected before opening", async () => {
    const { result, selectRow } = setup(vi.fn(async () => [entry("c1")]), { commitId: "before" });

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.loading).toBe(false));
    act(() => result.current.close());

    expect(result.current.history).toBeNull();
    expect(selectRow).toHaveBeenLastCalledWith({ commitId: "before" });
  });

  it("loadMore re-requests with a larger limit and reports hasMore", async () => {
    const full = Array.from({ length: FILE_HISTORY_PAGE_SIZE }, (_, i) => entry(`c${i}`));
    const getFileHistory = vi.fn(async () => full);
    const { result } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.hasMore).toBe(true));
    act(() => result.current.loadMore());

    await waitFor(() =>
      expect(getFileHistory).toHaveBeenLastCalledWith("/repo", "f.txt", FILE_HISTORY_PAGE_SIZE * 2),
    );
  });

  it("drops a stale response when another file was opened meanwhile", async () => {
    const forA = deferred<FileHistoryEntry[]>();
    const forB = deferred<FileHistoryEntry[]>();
    const getFileHistory = vi.fn((_repo: string, path: string) => (path === "a.txt" ? forA.promise : forB.promise));
    const { result } = setup(getFileHistory);

    act(() => result.current.open("a.txt"));
    act(() => result.current.open("b.txt"));
    await act(async () => forB.resolve([entry("b1", "b.txt")]));
    await act(async () => forA.resolve([entry("a1", "a.txt")]));

    expect(result.current.history?.path).toBe("b.txt");
    expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["b1"]);
  });

  it("refetches the open history when the repo refreshes", async () => {
    const getFileHistory = vi
      .fn<RepoClient["getFileHistory"]>()
      .mockResolvedValueOnce([entry("c1")])
      .mockResolvedValueOnce([entry("c2"), entry("c1")]);
    const { result, rerender } = setup(getFileHistory);

    act(() => result.current.open("f.txt"));
    await waitFor(() => expect(result.current.history?.entries).toHaveLength(1));
    rerender({ generation: 1, row: { commitId: "c1" } });

    await waitFor(() => expect(result.current.history?.entries.map((e) => e.commit.id)).toEqual(["c2", "c1"]));
  });

  it("does nothing on refresh while closed", () => {
    const getFileHistory = vi.fn(async () => []);
    const { rerender } = setup(getFileHistory);

    rerender({ generation: 1, row: "uncommitted" });

    expect(getFileHistory).not.toHaveBeenCalled();
  });

  it("keeps the error and retries with the same path and limit", async () => {
    const getFileHistory = vi
      .fn<RepoClient["getFileHistory"]>()
      .mockRejectedValueOnce(new Error("Path must be relative to the repository root."))
      .mockResolvedValueOnce([entry("c1")]);
    const { result } = setup(getFileHistory);

    act(() => result.current.open("../x"));
    await waitFor(() => expect(result.current.history?.error?.message).toContain("relative"));
    act(() => result.current.retry());

    await waitFor(() => expect(result.current.history?.error).toBeNull());
    expect(getFileHistory).toHaveBeenLastCalledWith("/repo", "../x", FILE_HISTORY_PAGE_SIZE);
  });
});
