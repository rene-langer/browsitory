import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CommitLocation, GraphCommit, RepoClient } from "../ipc/RepoClient";
import { useAppState, type SelectedRow } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";
import { useGoTo } from "./useGoTo";

function commit(id: string, extra: Partial<GraphCommit> = {}): GraphCommit {
  return {
    id,
    shortId: id.slice(0, 7),
    summary: `summary of ${id}`,
    authorName: "a",
    authorEmail: "a@b",
    timestamp: 0,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
    ...extra,
  };
}

function setup(overrides: Partial<RepoClient>) {
  const client = transferClient(overrides);
  const reveal = vi.fn<(row: SelectedRow) => void>();
  const { result } = renderHook(() => {
    const appState = useAppState(client, TEST_REPO_PATH);
    const goTo = useGoTo(
      client,
      TEST_REPO_PATH,
      appState,
      (row) => {
        reveal(row);
        appState.selectRow(row);
      },
    );
    return { appState, goTo };
  });
  return { result, reveal };
}

const hidden = (location: Partial<CommitLocation>): CommitLocation => ({
  commitId: "f".repeat(40),
  position: null,
  branches: [],
  remoteBranches: [],
  ...location,
});

describe("useGoTo", () => {
  it("selects a loaded commit without asking the backend", async () => {
    const locateCommit = vi.fn();
    const { result, reveal } = setup({ getCommitGraph: async () => [commit("a".repeat(40))], locateCommit });
    await act(() => result.current.appState.refresh());

    await act(() => result.current.goTo.goToRevision("a".repeat(40)));

    expect(reveal).toHaveBeenCalledWith({ commitId: "a".repeat(40) });
    expect(locateCommit).not.toHaveBeenCalled();
    expect(result.current.goTo.notice).toBeNull();
  });

  it("loads history down to a commit further down the graph", async () => {
    const target = "b".repeat(40);
    const limits: number[] = [];
    const locateCommit = vi.fn(async () => ({ commitId: target, position: 450, branches: [], remoteBranches: [] }));
    const { result, reveal } = setup({
      getCommitGraph: async (_repoPath, limit) => {
        limits.push(limit);
        return limit > 300 ? [commit("a".repeat(40)), commit(target)] : [commit("a".repeat(40))];
      },
      locateCommit,
    });
    await act(() => result.current.appState.refresh());

    await act(() => result.current.goTo.goToRevision("bbbbbbb"));

    expect(locateCommit).toHaveBeenCalledWith(TEST_REPO_PATH, "bbbbbbb", null, []);
    expect(reveal).toHaveBeenCalledWith({ commitId: target });
    expect(limits).toEqual([300, 600]);
    expect(result.current.appState.state.selectedRow).toEqual({ commitId: target });
  });

  it("says when nothing matches", async () => {
    const { result, reveal } = setup({ locateCommit: async () => null });

    await act(() => result.current.goTo.goToRevision("nope"));

    expect(reveal).not.toHaveBeenCalled();
    expect(result.current.goTo.notice).toEqual({ message: "No commit matches “nope”.", commitId: null, action: null });
  });

  it("shows the backend's message for an ambiguous prefix", async () => {
    const { result } = setup({
      locateCommit: async () => {
        throw new Error("\"ab\" matches more than one commit. Type more of the SHA.");
      },
    });

    await act(() => result.current.goTo.goToRevision("ab"));

    expect(result.current.goTo.notice?.message).toBe("\"ab\" matches more than one commit. Type more of the SHA.");
  });

  it("explains a commit only on a hidden branch and offers to show that branch", async () => {
    const target = "f".repeat(40);
    let selection: string[] | null = ["main"];
    const locateCommit = vi.fn(async (_repoPath: string, _revision: string, local: string[] | null) =>
      local !== null && local.includes("feature")
        ? { commitId: target, position: 1, branches: [], remoteBranches: [] }
        : hidden({ branches: ["feature"] }),
    );
    const setGraphBranchSelection = vi.fn(async (_repoPath: string, next: string[]) => {
      selection = next;
    });
    const { result, reveal } = setup({
      listBranches: async () => [
        { name: "main", isCurrent: true },
        { name: "feature", isCurrent: false },
        { name: "other", isCurrent: false },
      ],
      getGraphBranchSelection: async () => selection,
      setGraphBranchSelection,
      getCommitGraph: async () => (selection?.includes("feature") ? [commit("a".repeat(40)), commit(target)] : [commit("a".repeat(40))]),
      locateCommit,
    });
    await act(() => result.current.appState.refresh());

    await act(() => result.current.goTo.goToTag("v1.0", target));

    // Still selected, so its changes show; the graph has no row for it.
    expect(reveal).toHaveBeenCalledWith({ commitId: target });
    const notice = result.current.goTo.notice;
    expect(notice?.message).toBe("v1.0 is not in the graph: it is only on branch feature, hidden in the graph.");
    expect(notice?.action?.label).toBe("Show feature");

    await act(async () => notice?.action?.run());
    await act(async () => {});

    expect(setGraphBranchSelection).toHaveBeenCalledWith(TEST_REPO_PATH, ["main", "feature"]);
    expect(locateCommit).toHaveBeenLastCalledWith(TEST_REPO_PATH, target, ["main", "feature"], []);
    expect(result.current.goTo.notice).toBeNull();
    expect(result.current.appState.state.commits.map((c) => c.id)).toContain(target);
  });

  it("offers to show all branches for a side branch of an isolated branch", async () => {
    const { result } = setup({
      listBranches: async () => [{ name: "main", isCurrent: true }],
      getGraphBranchSelection: async () => ["main"],
      locateCommit: async () => hidden({ branches: ["main"] }),
    });
    await act(() => result.current.appState.refresh());

    await act(() => result.current.goTo.goToRevision("f".repeat(40)));

    expect(result.current.goTo.notice?.message).toBe(
      "fffffff is on a side branch merged into main; the graph follows only the first parents of main.",
    );
    expect(result.current.goTo.notice?.action?.label).toBe("Show all branches");
  });

  it("offers to show a hidden remote branch", async () => {
    const setGraphRemoteBranchSelection = vi.fn(async () => {});
    const { result } = setup({
      listBranches: async () => [{ name: "main", isCurrent: true }],
      locateCommit: async () => hidden({ remoteBranches: ["origin/topic"] }),
      setGraphRemoteBranchSelection,
    });
    await act(() => result.current.appState.refresh());

    await act(() => result.current.goTo.goToBranch("remote", "origin/topic"));
    await act(async () => result.current.goTo.notice?.action?.run());

    expect(setGraphRemoteBranchSelection).toHaveBeenCalledWith(TEST_REPO_PATH, ["origin/topic"]);
  });

  it("says when a commit is on no branch, and drops the notice once another row is selected", async () => {
    const { result } = setup({ locateCommit: async () => hidden({}) });

    await act(() => result.current.goTo.goToRevision("f".repeat(40)));
    expect(result.current.goTo.notice).toEqual({
      message: "fffffff is not on any branch, so the graph does not show it.",
      commitId: "f".repeat(40),
      action: null,
    });

    act(() => result.current.appState.selectRow("uncommitted"));
    expect(result.current.goTo.notice).toBeNull();
  });

  it("goes to a branch tip that is loaded by its badge, else by its ref", async () => {
    const locateCommit = vi.fn(async () => null);
    const { result, reveal } = setup({
      getCommitGraph: async () => [commit("a".repeat(40), { branchRefs: ["main"] })],
      locateCommit,
    });
    await act(() => result.current.appState.refresh());

    await act(() => result.current.goTo.goToBranch("local", "main"));
    await act(() => result.current.goTo.goToBranch("local", "gone"));

    expect(reveal).toHaveBeenCalledWith({ commitId: "a".repeat(40) });
    expect(locateCommit).toHaveBeenCalledWith(TEST_REPO_PATH, "refs/heads/gone", null, []);
    expect(result.current.goTo.notice?.message).toBe("No commit matches “gone”.");
  });

  it("explains a tag of a tree or blob without asking the backend", async () => {
    const locateCommit = vi.fn();
    const { result } = setup({ locateCommit });

    await act(() => result.current.goTo.goToTag("tree-tag", null));

    expect(locateCommit).not.toHaveBeenCalled();
    expect(result.current.goTo.notice?.message).toBe("Tag tree-tag points at a file or folder, not a commit.");
  });
});
