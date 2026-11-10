import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CommitStats, CommitStatsEntry, RepoClient } from "../ipc/RepoClient";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { LINE_STATS_BATCH, useCommitLineStats } from "./useCommitLineStats";

const stats = (insertions: number): CommitStats => ({ filesChanged: 1, insertions, deletions: 0, files: [] });
const ids = (count: number, prefix = "c") => Array.from({ length: count }, (_, index) => `${prefix}${index}`);
const answerAll = async (_repoPath: string, commitIds: string[]): Promise<CommitStatsEntry[]> =>
  commitIds.map((commitId, index) => ({ commitId, stats: stats(index + 1) }));

function setup(getCommitStats: RepoClient["getCommitStats"], repoPath = "/repo") {
  const client = fakeRepoClient({ getCommitStats });
  return renderHook(({ path }) => useCommitLineStats(client, path), { initialProps: { path: repoPath } });
}

describe("useCommitLineStats (#620)", () => {
  it("counts the visible commits a batch at a time and keeps the numbers", async () => {
    const getCommitStats = vi.fn(answerAll);
    const { result } = setup(getCommitStats);
    const visible = ids(LINE_STATS_BATCH + 10);

    act(() => result.current.request(visible));

    await waitFor(() => expect(result.current.stats.size).toBe(visible.length));
    expect(getCommitStats).toHaveBeenCalledTimes(2);
    expect(getCommitStats.mock.calls[0]).toEqual(["/repo", visible.slice(0, LINE_STATS_BATCH), false]);
    expect(getCommitStats.mock.calls[1]).toEqual(["/repo", visible.slice(LINE_STATS_BATCH), false]);
    expect(result.current.stats.get("c0")).toEqual(stats(1));

    // Commit contents never change: scrolling back asks for nothing.
    act(() => result.current.request(visible.slice(0, 5)));
    expect(getCommitStats).toHaveBeenCalledTimes(2);
  });

  it("asks again for the commits the backend's time budget left out", async () => {
    const getCommitStats = vi.fn(async (_repoPath: string, commitIds: string[]) =>
      commitIds.slice(0, 2).map((commitId) => ({ commitId, stats: stats(1) })),
    );
    const { result } = setup(getCommitStats);

    act(() => result.current.request(["a", "b", "c"]));

    await waitFor(() => expect(result.current.stats.has("c")).toBe(true));
    expect(getCommitStats.mock.calls.map((call) => call[1])).toEqual([["a", "b", "c"], ["c"]]);
  });

  it("keeps one call in flight and skips commits that scrolled away meanwhile", async () => {
    let release: () => void = () => {};
    const getCommitStats = vi.fn(async (repoPath: string, commitIds: string[]) => {
      if (getCommitStats.mock.calls.length === 1) await new Promise<void>((resolve) => (release = resolve));
      return answerAll(repoPath, commitIds);
    });
    const { result } = setup(getCommitStats);
    const first = ids(LINE_STATS_BATCH + 10);

    act(() => result.current.request(first));
    await waitFor(() => expect(getCommitStats).toHaveBeenCalledTimes(1));
    act(() => result.current.request(["elsewhere"]));
    expect(getCommitStats).toHaveBeenCalledTimes(1);
    await act(async () => release());

    await waitFor(() => expect(result.current.stats.has("elsewhere")).toBe(true));
    expect(getCommitStats.mock.calls.map((call) => call[1])).toEqual([first.slice(0, LINE_STATS_BATCH), ["elsewhere"]]);
  });

  it("gives a commit over the cap, or a failed batch, no numbers and doesn't retry", async () => {
    const getCommitStats = vi.fn<RepoClient["getCommitStats"]>(async () => [{ commitId: "big", stats: null }]);
    getCommitStats.mockRejectedValueOnce(new Error("worker stopped"));
    const { result } = setup(getCommitStats);

    act(() => result.current.request(["lost"]));
    await waitFor(() => expect(result.current.stats.has("lost")).toBe(true));
    act(() => result.current.request(["lost", "big"]));
    await waitFor(() => expect(result.current.stats.has("big")).toBe(true));

    expect(result.current.stats.get("lost")).toBeNull();
    expect(result.current.stats.get("big")).toBeNull();
    expect(getCommitStats).toHaveBeenCalledTimes(2);
  });

  it("starts over for another repository", async () => {
    const getCommitStats = vi.fn(answerAll);
    const { result, rerender } = setup(getCommitStats);
    act(() => result.current.request(["a"]));
    await waitFor(() => expect(result.current.stats.has("a")).toBe(true));

    rerender({ path: "/other" });
    expect(result.current.stats.size).toBe(0);
    act(() => result.current.request(["a"]));

    await waitFor(() => expect(result.current.stats.has("a")).toBe(true));
    expect(getCommitStats).toHaveBeenLastCalledWith("/other", ["a"], false);
  });
});
