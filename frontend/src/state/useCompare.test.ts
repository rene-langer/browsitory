import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CompareRange, RepoClient } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import { useCompare } from "./useCompare";

function setup(getBranchCompareRange: RepoClient["getBranchCompareRange"] = vi.fn()) {
  const client = { getBranchCompareRange } as unknown as RepoClient;
  return renderHook(
    ({ generation, row }: { generation: number; row: SelectedRow }) => useCompare(client, "/repo", generation, row),
    { initialProps: { generation: 0, row: { commitId: "b" } as SelectedRow } },
  );
}

describe("useCompare", () => {
  it("opens a commit compare and closes it", () => {
    const { result } = setup();
    act(() => result.current.compareCommits("a", "b"));
    expect(result.current.compare).toEqual({
      target: { kind: "commits", fromId: "a", toId: "b" },
      range: { fromId: "a", toId: "b" },
      error: null,
    });
    act(() => result.current.close());
    expect(result.current.compare).toBeNull();
  });

  it("swap flips from and to", () => {
    const { result } = setup();
    act(() => result.current.compareCommits("a", "b"));
    act(() => result.current.swap());
    expect(result.current.compare?.range).toEqual({ fromId: "b", toId: "a" });
  });

  it("a working-tree compare has no toId", () => {
    const { result } = setup();
    act(() => result.current.compareWorkingTree("a"));
    expect(result.current.compare?.range).toEqual({ fromId: "a", toId: null });
  });

  it("closes when the selected row changes and does not come back", () => {
    const { result, rerender } = setup();
    act(() => result.current.compareCommits("a", "b"));
    rerender({ generation: 0, row: { commitId: "c" } });
    expect(result.current.compare).toBeNull();
    rerender({ generation: 0, row: { commitId: "b" } });
    expect(result.current.compare).toBeNull();
  });

  it("resolves a branch compare and re-resolves on refresh", async () => {
    const getBranchCompareRange = vi
      .fn<(repoPath: string, refName: string) => Promise<CompareRange>>()
      .mockResolvedValueOnce({ fromId: "base", toId: "tip1" })
      .mockResolvedValueOnce({ fromId: "base", toId: "tip2" });
    const { result, rerender } = setup(getBranchCompareRange);
    act(() => result.current.compareBranch("refs/heads/feature", "feature"));
    expect(result.current.compare?.range).toBeNull();
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "tip1" }));
    expect(getBranchCompareRange).toHaveBeenCalledWith("/repo", "refs/heads/feature");

    rerender({ generation: 1, row: { commitId: "b" } });
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "tip2" }));
  });

  it("drops a stale branch resolution", async () => {
    let resolveFirst: (range: CompareRange) => void = () => {};
    const getBranchCompareRange = vi
      .fn<(repoPath: string, refName: string) => Promise<CompareRange>>()
      .mockImplementationOnce(() => new Promise((resolve) => (resolveFirst = resolve)))
      .mockResolvedValueOnce({ fromId: "base", toId: "other" });
    const { result } = setup(getBranchCompareRange);
    act(() => result.current.compareBranch("refs/heads/one", "one"));
    act(() => result.current.compareBranch("refs/heads/two", "two"));
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "other" }));
    await act(async () => resolveFirst({ fromId: "base", toId: "stale" }));
    expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "other" });
  });

  it("reports a resolve error and retries", async () => {
    const getBranchCompareRange = vi
      .fn<(repoPath: string, refName: string) => Promise<CompareRange>>()
      .mockRejectedValueOnce("orphan and HEAD have no common ancestor.")
      .mockResolvedValueOnce({ fromId: "base", toId: "tip" });
    const { result } = setup(getBranchCompareRange);
    act(() => result.current.compareBranch("refs/heads/orphan", "orphan"));
    await waitFor(() =>
      expect(result.current.compare?.error?.message).toBe("orphan and HEAD have no common ancestor."),
    );
    act(() => result.current.retry());
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "tip" }));
    expect(result.current.compare?.error).toBeNull();
  });

  it("re-opening a closed branch compare does not show the previous result", async () => {
    const getBranchCompareRange = vi
      .fn<(repoPath: string, refName: string) => Promise<CompareRange>>()
      .mockResolvedValueOnce({ fromId: "base", toId: "old-tip" })
      .mockImplementationOnce(() => new Promise(() => {}));
    const { result } = setup(getBranchCompareRange);
    act(() => result.current.compareBranch("refs/heads/feature", "feature"));
    await waitFor(() => expect(result.current.compare?.range).toEqual({ fromId: "base", toId: "old-tip" }));
    act(() => result.current.close());

    act(() => result.current.compareBranch("refs/heads/feature", "feature"));
    expect(result.current.compare?.range).toBeNull();
    await waitFor(() => expect(getBranchCompareRange).toHaveBeenCalledTimes(2));
    expect(result.current.compare?.range).toBeNull();
  });

  it("re-opening a closed branch compare does not show the previous error", async () => {
    const getBranchCompareRange = vi
      .fn<(repoPath: string, refName: string) => Promise<CompareRange>>()
      .mockRejectedValueOnce("boom")
      .mockImplementationOnce(() => new Promise(() => {}));
    const { result } = setup(getBranchCompareRange);
    act(() => result.current.compareBranch("refs/heads/feature", "feature"));
    await waitFor(() => expect(result.current.compare?.error?.message).toBe("boom"));
    act(() => result.current.close());

    act(() => result.current.compareBranch("refs/heads/feature", "feature"));
    expect(result.current.compare?.error).toBeNull();
  });
});
