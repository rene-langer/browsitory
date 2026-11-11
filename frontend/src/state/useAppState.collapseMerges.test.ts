import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit, GraphSelection } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { graphPage } from "../test/fakeRepoClient";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

function commit(id: string, parentIds: string[], extra: Partial<GraphCommit> = {}): GraphCommit {
  return {
    id,
    shortId: id.slice(0, 7),
    summary: id,
    authorName: "a",
    authorEmail: "a@b",
    timestamp: 0,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
    ...extra,
  };
}

const merge = commit("merge", ["base", "side"], { mergedCount: 1 });
const rows = [merge, commit("base", [])];
const side = [commit("side", ["base"])];

/** A client whose saved "Collapse merges" setting `setGraphCollapseMerges` changes. */
function collapsingClient(initial: boolean) {
  let saved = initial;
  const selections: GraphSelection[] = [];
  const getMergedCommits = vi.fn(async (_repoPath: string, _mergeId: string, _limit: number) => side);
  const client = transferClient({
    getGraphCollapseMerges: async () => saved,
    setGraphCollapseMerges: async (_repoPath, collapse) => {
      saved = collapse;
    },
    getCommitGraph: async (_repoPath, _limit, selection) => {
      selections.push(selection);
      return graphPage(selection.firstParent === true ? rows : [{ ...merge, mergedCount: undefined }, ...side, rows[1]]);
    },
    getMergedCommits,
  });
  return { client, selections, getMergedCommits };
}

describe("useAppState collapse merges (#617)", () => {
  it("walks first parents only while the saved setting is on, and sends no flag while it is off", async () => {
    const { client, selections } = collapsingClient(false);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(selections.at(-1)).toEqual({ local: null, remote: [] });
    expect(result.current.state.collapseMerges).toBe(false);

    await act(() => result.current.setCollapseMerges(true));
    expect(selections.at(-1)).toEqual({ local: null, remote: [], firstParent: true });
    expect(result.current.state.collapseMerges).toBe(true);
    expect(result.current.state.commits.map((c) => c.id)).toEqual(["merge", "base"]);
  });

  it("splices an expanded merge's commits in below it, keeps them on refresh and hides them again", async () => {
    const { client, getMergedCommits } = collapsingClient(true);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.expandMerge("merge"));
    expect(getMergedCommits).toHaveBeenLastCalledWith(TEST_REPO_PATH, "merge", 100);
    expect(result.current.state.commits.map((c) => c.id)).toEqual(["merge", "side", "base"]);
    expect(result.current.state.graphRows.map((c) => c.id)).toEqual(["merge", "base"]);
    expect(result.current.state.hasMoreHistory).toBe(false);

    await act(() => result.current.refresh());
    expect(getMergedCommits).toHaveBeenCalledTimes(2);
    expect(result.current.state.commits.map((c) => c.id)).toEqual(["merge", "side", "base"]);

    act(() => result.current.collapseMerge("merge"));
    expect(result.current.state.commits.map((c) => c.id)).toEqual(["merge", "base"]);
    await act(() => result.current.refresh());
    expect(getMergedCommits).toHaveBeenCalledTimes(2);
  });

  it("loads whole pages of a merge, growing it on a second expand", async () => {
    const { client, getMergedCommits } = collapsingClient(true);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.expandMerge("merge", 150));
    expect(getMergedCommits).toHaveBeenLastCalledWith(TEST_REPO_PATH, "merge", 200);
    await act(() => result.current.expandMerge("merge", 1));
    expect(getMergedCommits).toHaveBeenLastCalledWith(TEST_REPO_PATH, "merge", 200);
    expect(result.current.state.expandedMerges.merge.limit).toBe(200);
  });

  it("drops an expansion once its merge is no merge row of the graph", async () => {
    const { client, getMergedCommits } = collapsingClient(true);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.expandMerge("merge"));

    // Turning it off walks every ancestor: the merge row carries no count any more.
    await act(() => result.current.setCollapseMerges(false));

    expect(result.current.state.expandedMerges).toEqual({});
    expect(result.current.state.commits.map((c) => c.id)).toEqual(["merge", "side", "base"]);
    expect(getMergedCommits).toHaveBeenCalledTimes(1);
  });

  it("reports a failed expand and leaves the merge collapsed", async () => {
    const { client } = collapsingClient(true);
    client.getMergedCommits = async () => {
      throw new Error("boom");
    };
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.expandMerge("merge"));

    expect(result.current.state.error).toContain("boom");
    expect(result.current.state.expandedMerges).toEqual({});
  });
});
