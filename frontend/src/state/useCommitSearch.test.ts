import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { GraphCommit, RepoClient, SearchPage } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import { SEARCH_DEBOUNCE_MS, SEARCH_ROUND_SIZE, useCommitSearch } from "./useCommitSearch";

function commit(id: string): GraphCommit {
  return {
    id, shortId: id.slice(0, 7), summary: `commit ${id}`, authorName: "A", authorEmail: "a@example.com",
    timestamp: 1_700_000_000, parentIds: [], branchRefs: [], remoteBranchRefs: [],
  };
}
function page(ids: string[], nextCursor: string | null, scanned: number): SearchPage {
  return { commits: ids.map(commit), nextCursor, scanned };
}
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

function setup(
  searchCommits: RepoClient["searchCommits"],
  { row = "uncommitted" as SelectedRow, debounceMs = 0 } = {},
) {
  const cancelSearch = vi.fn(async () => {});
  const client = { searchCommits, cancelSearch } as unknown as RepoClient;
  const selectRow = vi.fn();
  const hook = renderHook(
    ({ generation, refs, selected }: { generation: number; refs: string[] | null; selected: SelectedRow }) =>
      useCommitSearch(client, "/repo", generation, refs, selected, selectRow, { debounceMs }),
    { initialProps: { generation: 0, refs: ["main"] as string[] | null, selected: row } },
  );
  return { ...hook, selectRow, cancelSearch };
}

afterEach(() => vi.useRealTimers());

describe("useCommitSearch", () => {
  it("debounces typing and searches once with the graph's branch selection", async () => {
    vi.useFakeTimers();
    const searchCommits = vi.fn(async () => page([], null, 0));
    const { result } = setup(searchCommits, { debounceMs: SEARCH_DEBOUNCE_MS });

    act(() => result.current.setQuery("a"));
    act(() => result.current.setQuery("au"));
    await act(async () => { await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS - 1); });
    expect(searchCommits).not.toHaveBeenCalled();
    expect(result.current.query).toBe("au");
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });

    expect(searchCommits).toHaveBeenCalledTimes(1);
    expect(searchCommits).toHaveBeenCalledWith("/repo", "au", ["main"], null);
  });

  it("keeps paging until a round is full or the walk ends, reporting scanned progress", async () => {
    const second = deferred<SearchPage>();
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page(["c1"], "20000:fp", 20_000))
      .mockReturnValueOnce(second.promise);
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.scanned).toBe(20_000));
    expect(result.current.results?.loading).toBe(true);
    expect(result.current.results?.commits.map((c) => c.id)).toEqual(["c1"]);
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", ["main"], "20000:fp");

    await act(async () => second.resolve(page(["c2"], null, 25_000)));
    expect(result.current.results).toMatchObject({ loading: false, nextCursor: null, scanned: 25_000 });
    expect(result.current.results?.commits.map((c) => c.id)).toEqual(["c1", "c2"]);
  });

  it("stops a round at SEARCH_ROUND_SIZE matches and loadMore continues from nextCursor", async () => {
    const full = Array.from({ length: SEARCH_ROUND_SIZE }, (_, i) => `c${i}`);
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page(full, "100:fp", 500))
      .mockResolvedValueOnce(page(["more"], null, 900));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    expect(searchCommits).toHaveBeenCalledTimes(1);

    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.results?.nextCursor).toBeNull());
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", ["main"], "100:fp");
    expect(result.current.results?.commits).toHaveLength(SEARCH_ROUND_SIZE + 1);
  });

  it("selects the first match of a new query and close restores the previous row and cancels", async () => {
    const pending = deferred<SearchPage>();
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page(["c1"], "1:fp", 20_000))
      .mockReturnValueOnce(pending.promise);
    const { result, selectRow, cancelSearch } = setup(searchCommits, { row: { commitId: "before" } });

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(selectRow).toHaveBeenCalledWith({ commitId: "c1" }));

    act(() => result.current.close());
    expect(result.current.results).toBeNull();
    expect(result.current.query).toBe("");
    expect(selectRow).toHaveBeenLastCalledWith({ commitId: "before" });
    expect(cancelSearch).toHaveBeenCalledWith("/repo");

    await act(async () => pending.resolve(page(["late"], null, 30_000)));
    expect(result.current.results).toBeNull();
  });

  it("drops a stale response after the query changes", async () => {
    const first = deferred<SearchPage>();
    const searchCommits = vi.fn()
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce(page(["new"], null, 2));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("old"));
    await waitFor(() => expect(searchCommits).toHaveBeenCalledTimes(1));
    act(() => result.current.setQuery("new"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    await act(async () => first.resolve(page(["old"], null, 2)));

    expect(result.current.results?.query).toBe("new");
    expect(result.current.results?.commits.map((c) => c.id)).toEqual(["new"]);
  });

  it("shows an error and retry restarts from the first page", async () => {
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page(Array.from({ length: SEARCH_ROUND_SIZE }, (_, i) => `c${i}`), "100:fp", 100))
      .mockRejectedValueOnce(new Error("The history changed since this search started. Search again."))
      .mockResolvedValueOnce(page(["fresh"], null, 5));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.results?.error?.message).toMatch(/history changed/));

    act(() => result.current.retry());
    await waitFor(() => expect(result.current.results?.commits.map((c) => c.id)).toEqual(["fresh"]));
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", ["main"], null);
    expect(result.current.results?.error).toBeNull();
  });

  it("re-runs from the first page on refresh without changing the selection", async () => {
    const searchCommits = vi.fn(async () => page(["c1"], null, 1));
    const { result, rerender, selectRow } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    selectRow.mockClear();
    rerender({ generation: 1, refs: ["main"], selected: { commitId: "c1" } });

    await waitFor(() => expect(searchCommits).toHaveBeenCalledTimes(2));
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", ["main"], null);
    expect(selectRow).not.toHaveBeenCalled();
  });

  it("clearing the field closes the search", async () => {
    const searchCommits = vi.fn(async () => page(["c1"], null, 1));
    const { result, selectRow } = setup(searchCommits, { row: { commitId: "before" } });

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    act(() => result.current.setQuery("   "));

    expect(result.current.results).toBeNull();
    expect(selectRow).toHaveBeenLastCalledWith({ commitId: "before" });
  });
});
