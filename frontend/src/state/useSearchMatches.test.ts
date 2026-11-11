import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import type { CommitSearchResultsState } from "./useCommitSearch";
import { matchCountText, useSearchMatches } from "./useSearchMatches";

const commit = (id: string): GraphCommit => ({
  id,
  shortId: id,
  summary: id,
  authorName: "a",
  authorEmail: "a@x",
  timestamp: 0,
  parentIds: [],
  branchRefs: [],
  remoteBranchRefs: [],
});

function results(ids: string[], extra: Partial<CommitSearchResultsState> = {}): CommitSearchResultsState {
  return {
    query: "q",
    commits: ids.map(commit),
    nextCursor: null,
    scanned: 10,
    loading: false,
    error: null,
    queryError: null,
    ...extra,
  };
}

// The graph: a, b, c, d, e, f; the matches: b, d, f.
const graph = ["a", "b", "c", "d", "e", "f"].map(commit);

function setup(selected: SelectedRow, found: CommitSearchResultsState | null, loadMore = vi.fn(async () => [] as GraphCommit[])) {
  const reveal = vi.fn();
  const hook = renderHook(
    ({ row, res }: { row: SelectedRow; res: CommitSearchResultsState | null }) =>
      useSearchMatches(res, row, graph, loadMore, reveal),
    { initialProps: { row: selected, res: found } },
  );
  return { ...hook, reveal, loadMore };
}

// #687: a count, and previous/next through the matches where they sit in the graph.
describe("useSearchMatches", () => {
  it("counts the matches and says which one is selected", () => {
    expect(matchCountText(results(["b", "d", "f"]), -1)).toBe("3 matches");
    expect(matchCountText(results(["b", "d", "f"]), 1)).toBe("2 of 3");
    expect(matchCountText(results(["b"]), -1)).toBe("1 match");
    expect(matchCountText(results(["b"], { nextCursor: "c" }), -1)).toBe("1+ matches");
    expect(matchCountText(results(["b", "d"], { nextCursor: "c" }), 0)).toBe("1 of 2+");
    expect(matchCountText(results([], { loading: true }), -1)).toBe("Searching…");
    expect(matchCountText(results([]), -1)).toBe("No matches");
    expect(matchCountText(results([], { queryError: "bad" }), -1)).toBe("");
  });

  it("dims nothing and offers nothing without a search", () => {
    const { result } = setup("uncommitted", null);
    expect(result.current.ids).toBeNull();
    expect(result.current.canGoNext).toBe(false);
    expect(result.current.canGoPrevious).toBe(false);
  });

  it("steps from one match to the next and back", async () => {
    const { result, reveal } = setup({ commitId: "d" }, results(["b", "d", "f"]));
    expect(result.current.ids).toEqual(new Set(["b", "d", "f"]));
    expect(result.current.current).toBe(1);
    await act(() => result.current.step(1));
    expect(reveal).toHaveBeenLastCalledWith("f");
    await act(() => result.current.step(-1));
    expect(reveal).toHaveBeenLastCalledWith("b");
  });

  it("from a commit that is no match, goes to the nearest match below or above it", async () => {
    const { result, reveal } = setup({ commitId: "c" }, results(["b", "d", "f"]));
    expect(result.current.current).toBe(-1);
    await act(() => result.current.step(1));
    expect(reveal).toHaveBeenLastCalledWith("d");
    await act(() => result.current.step(-1));
    expect(reveal).toHaveBeenLastCalledWith("b");
  });

  it("starts at the first match from Uncommitted changes", async () => {
    const { result, reveal } = setup("uncommitted", results(["b", "d", "f"]));
    await act(() => result.current.step(1));
    expect(reveal).toHaveBeenLastCalledWith("b");
  });

  it("stops at the ends of the matches", async () => {
    const { result, reveal } = setup({ commitId: "f" }, results(["b", "d", "f"]));
    expect(result.current.canGoNext).toBe(false);
    await act(() => result.current.step(1));
    expect(reveal).not.toHaveBeenCalled();

    const first = setup({ commitId: "b" }, results(["b", "d", "f"]));
    expect(first.result.current.canGoPrevious).toBe(false);
  });

  it("reads the next round past the last loaded match", async () => {
    const loadMore = vi.fn(async () => ["b", "d", "x"].map(commit));
    const { result, reveal } = setup({ commitId: "d" }, results(["b", "d"], { nextCursor: "more" }), loadMore);
    expect(result.current.canGoNext).toBe(true);
    await act(() => result.current.step(1));
    expect(loadMore).toHaveBeenCalledTimes(1);
    expect(reveal).toHaveBeenLastCalledWith("x");
  });
});
