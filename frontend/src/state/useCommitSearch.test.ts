import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RepoError, type GraphCommit, type RepoClient, type SearchPage } from "../ipc/RepoClient";
import { RECENT_SEARCH_LIMIT } from "../lib/recentSearches";
import type { SelectedRow } from "./useAppState";
import { SEARCH_DEBOUNCE_MS, SEARCH_ROUND_SIZE, useCommitSearch } from "./useCommitSearch";
import { fakeRepoClient } from "../test/fakeRepoClient";

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

interface SearchProps {
  generation: number;
  local: string[] | null;
  remote?: string[] | null;
  selected: SelectedRow;
}

function setup(
  searchCommits: RepoClient["searchCommits"],
  { row = "uncommitted" as SelectedRow, debounceMs = 0 } = {},
) {
  const cancelSearch = vi.fn(async () => {});
  const client = fakeRepoClient({ searchCommits, cancelSearch });
  const selectRow = vi.fn();
  const hook = renderHook(
    ({
      generation,
      local,
      remote = null,
      selected,
    }: SearchProps) => useCommitSearch(client, "/repo", generation, { local, remote }, selected, selectRow, { debounceMs }),
    { initialProps: { generation: 0, local: ["main"], selected: row } as SearchProps },
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
    expect(searchCommits).toHaveBeenCalledWith("/repo", "au", { local: ["main"], remote: [] }, null);
  });

  it("searches the graph's remote selection too (#385)", async () => {
    const searchCommits = vi.fn(async () => page([], null, 0));
    const { result, rerender } = setup(searchCommits);
    rerender({ generation: 0, local: [], remote: ["origin/feature"], selected: "uncommitted" });

    act(() => result.current.setQuery("x"));

    await waitFor(() => expect(searchCommits).toHaveBeenCalledTimes(1));
    expect(searchCommits).toHaveBeenCalledWith("/repo", "x", { local: [], remote: ["origin/feature"] }, null);
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
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", { local: ["main"], remote: [] }, "20000:fp");

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
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", { local: ["main"], remote: [] }, "100:fp");
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
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", { local: ["main"], remote: [] }, null);
    expect(result.current.results?.error).toBeNull();
  });

  it("keeps an invalid query's message for the search field, apart from search errors", async () => {
    const searchCommits = vi.fn()
      .mockRejectedValueOnce(new RepoError("InvalidSearchQuery", "Invalid search: Missing closing parenthesis."))
      .mockResolvedValueOnce(page(["c1"], null, 1));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("(a"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    expect(result.current.results?.queryError).toBe("Invalid search: Missing closing parenthesis.");
    expect(result.current.results?.error).toBeNull();

    act(() => result.current.setQuery("(a)"));
    await waitFor(() => expect(result.current.results?.commits.map((c) => c.id)).toEqual(["c1"]));
    expect(result.current.results?.queryError).toBeNull();
  });

  it("re-runs from the first page on refresh without changing the selection", async () => {
    const searchCommits = vi.fn(async () => page(["c1"], null, 1));
    const { result, rerender, selectRow } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    selectRow.mockClear();
    rerender({ generation: 1, local: ["main"], selected: { commitId: "c1" } });

    await waitFor(() => expect(searchCommits).toHaveBeenCalledTimes(2));
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", { local: ["main"], remote: [] }, null);
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
  it("does not auto-select the first match once the user changed the selection mid-round", async () => {
    const second = deferred<SearchPage>();
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page([], "250:fp", 250))
      .mockReturnValueOnce(second.promise);
    const { result, rerender, selectRow } = setup(searchCommits, { row: { commitId: "before" } });

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(searchCommits).toHaveBeenCalledTimes(2));
    // The user clicks another row (or opens file history, which selects its first entry).
    rerender({ generation: 0, local: ["main"], selected: { commitId: "picked" } });

    await act(async () => second.resolve(page(["c1"], null, 500)));
    expect(result.current.results?.commits.map((c) => c.id)).toEqual(["c1"]);
    expect(selectRow).not.toHaveBeenCalled();
  });
});

describe("useCommitSearch recent searches (#540)", () => {
  beforeEach(() => localStorage.clear());

  it("remembers the last query that ran per search, newest first, per repository", async () => {
    localStorage.setItem("commit-searches:/repo", JSON.stringify(["older"]));
    localStorage.setItem("commit-searches:/other", JSON.stringify(["elsewhere"]));
    const searchCommits = vi.fn(async () => page(["c1"], null, 1));
    const { result } = setup(searchCommits);
    expect(result.current.recentSearches).toEqual(["older"]);

    // Two queries in one search (typing on): one entry, the later query.
    act(() => result.current.setQuery("auth"));
    await waitFor(() => expect(result.current.recentSearches).toEqual(["auth", "older"]));
    act(() => result.current.setQuery("author:ada"));
    await waitFor(() => expect(result.current.recentSearches).toEqual(["author:ada", "older"]));

    // A new search after clearing the field adds an entry.
    act(() => result.current.setQuery(""));
    act(() => result.current.setQuery("  fix  "));
    await waitFor(() => expect(result.current.recentSearches).toEqual(["fix", "author:ada", "older"]));
    expect(JSON.parse(localStorage.getItem("commit-searches:/repo") ?? "[]")).toEqual(["fix", "author:ada", "older"]);
    expect(JSON.parse(localStorage.getItem("commit-searches:/other") ?? "[]")).toEqual(["elsewhere"]);
  });

  it("moves a repeated search to the front and keeps RECENT_SEARCH_LIMIT", async () => {
    const stored = Array.from({ length: RECENT_SEARCH_LIMIT }, (_, i) => `q${i}`);
    localStorage.setItem("commit-searches:/repo", JSON.stringify(stored));
    const searchCommits = vi.fn(async () => page([], null, 1));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("q5"));
    await waitFor(() => expect(result.current.recentSearches[0]).toBe("q5"));
    expect(result.current.recentSearches).toEqual(["q5", ...stored.filter((q) => q !== "q5")]);

    act(() => result.current.close());
    act(() => result.current.setQuery("new"));
    await waitFor(() => expect(result.current.recentSearches[0]).toBe("new"));
    expect(result.current.recentSearches).toHaveLength(RECENT_SEARCH_LIMIT);
    expect(result.current.recentSearches).not.toContain("q9");
  });

  it("does not remember an invalid query", async () => {
    const searchCommits = vi.fn().mockRejectedValue(new RepoError("InvalidSearchQuery", "Invalid search: oops."));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("(a"));
    await waitFor(() => expect(result.current.results?.queryError).toBe("Invalid search: oops."));
    expect(result.current.recentSearches).toEqual([]);
    expect(localStorage.getItem("commit-searches:/repo")).toBeNull();
  });

  it("removes one search", () => {
    localStorage.setItem("commit-searches:/repo", JSON.stringify(["a", "b"]));
    const { result } = setup(vi.fn(async () => page([], null, 0)));

    act(() => result.current.removeRecentSearch("a"));
    expect(result.current.recentSearches).toEqual(["b"]);
    expect(JSON.parse(localStorage.getItem("commit-searches:/repo") ?? "[]")).toEqual(["b"]);
  });
});
