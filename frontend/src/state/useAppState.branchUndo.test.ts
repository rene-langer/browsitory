import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { BranchInfo, RepoClient, UndoOutcome } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

/**
 * A client whose branches and saved graph selection change as the calls the tests make change
 * them: `deleteBranch` and a redone delete drop the branch, an undone one brings it back.
 */
function selectionClient(names: string[], selection: string[] | null, overrides: Partial<RepoClient> = {}) {
  const repo = { branches: [...names], saved: selection, writes: 0 };
  const done = (label: string): UndoOutcome => ({ kind: "Done", label });
  const client = transferClient({
    listBranches: async (): Promise<BranchInfo[]> =>
      repo.branches.map((name, index) => ({ name, isCurrent: index === 0 })),
    getGraphBranchSelection: async () => repo.saved,
    setGraphBranchSelection: async (_path: string, local: string[]) => {
      repo.saved = local;
      repo.writes += 1;
    },
    clearGraphBranchSelection: async () => {
      repo.saved = null;
      repo.writes += 1;
    },
    deleteBranch: async (_path: string, name: string) => {
      repo.branches = repo.branches.filter((candidate) => candidate !== name);
    },
    undo: async () => {
      const name = "topic";
      repo.branches = [...repo.branches, name];
      return done(`Delete branch ${name}`);
    },
    redo: async () => {
      repo.branches = repo.branches.filter((candidate) => candidate !== "topic");
      return done("Delete branch topic");
    },
    ...overrides,
  });
  return { client, repo };
}

describe("useAppState — graph selection across undo and redo of a branch delete (#112)", () => {
  it("undoing a branch delete puts the branch back into the saved selection", async () => {
    const { client, repo } = selectionClient(["main", "topic", "other"], ["main", "topic"]);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.deleteBranch("topic", false));
    expect(repo.saved).toEqual(["main"]);

    await act(async () => {
      await result.current.undo();
    });

    expect(repo.saved).toEqual(["main", "topic"]);
    expect(result.current.state.graphSelection.local).toEqual(["main", "topic"]);
  });

  it("undoing the delete of the isolated branch isolates it again", async () => {
    const { client, repo } = selectionClient(["main", "topic"], ["topic"]);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.deleteBranch("topic", false));
    expect(repo.saved).toBeNull();

    await act(async () => {
      await result.current.undo();
    });

    expect(repo.saved).toEqual(["topic"]);
  });

  it("redoing the delete takes the branch out of the selection again", async () => {
    const { client, repo } = selectionClient(["main", "topic", "other"], ["main", "topic"]);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.deleteBranch("topic", false));
    await act(async () => {
      await result.current.undo();
    });

    await act(async () => {
      await result.current.redo();
    });

    expect(repo.saved).toEqual(["main"]);
  });

  it("redoing the delete of the isolated branch shows every branch again", async () => {
    const { client, repo } = selectionClient(["main", "topic"], ["topic"]);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.deleteBranch("topic", false));
    await act(async () => {
      await result.current.undo();
    });

    await act(async () => {
      await result.current.redo();
    });

    expect(repo.saved).toBeNull();
  });

  it("an undo leaves a selection changed since the delete alone", async () => {
    const { client, repo } = selectionClient(["main", "topic", "other"], ["main", "topic"]);
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.deleteBranch("topic", false));
    await act(() => result.current.setGraphBranchSelection(["other"]));

    await act(async () => {
      await result.current.undo();
    });

    expect(repo.saved).toEqual(["other"]);
  });

  it("an undo of another step, or of a delete that left the selection alone, writes nothing", async () => {
    const { client, repo } = selectionClient(["main", "topic"], null, {
      undo: async () => ({ kind: "Done", label: "Commit" }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.deleteBranch("topic", false));

    await act(async () => {
      await result.current.undo();
    });

    expect(repo.writes).toBe(0);
    expect(repo.saved).toBeNull();
  });

  it("a refused undo writes nothing", async () => {
    const { client, repo } = selectionClient(["main", "topic", "other"], ["main", "topic"], {
      undo: async () => ({ kind: "RepositoryChanged", label: "Delete branch topic" }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.deleteBranch("topic", false));
    const writes = repo.writes;

    await act(async () => {
      await result.current.undo();
    });

    expect(repo.writes).toBe(writes);
    expect(repo.saved).toEqual(["main"]);
  });
});
