import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient, UndoOutcome } from "../ipc/RepoClient";
import { useUndoActions } from "./useUndoActions";
import type { AppState } from "./useAppState";

function setup(outcome: UndoOutcome | Error) {
  const client = {
    undo: vi.fn(async () => {
      if (outcome instanceof Error) throw outcome;
      return outcome;
    }),
    redo: vi.fn(async () => {
      if (outcome instanceof Error) throw outcome;
      return outcome;
    }),
  } as unknown as RepoClient;
  let state = { error: null, undoState: { undo: "Stage a.txt", redo: "Commit “x”" } } as unknown as AppState;
  const setState = vi.fn((updater: (prev: AppState) => AppState) => {
    state = updater(state);
  });
  const runMutationWithOutcome = vi.fn(async (mutate: () => Promise<void>) => {
    try {
      await mutate();
      return true;
    } catch (err) {
      state = { ...state, error: err instanceof Error ? err.message : String(err) };
      return false;
    }
  });
  const { result } = renderHook(() => useUndoActions(client, "/repo", runMutationWithOutcome, setState));
  return { result, client, getState: () => state };
}

describe("useUndoActions", () => {
  it("returns Done and leaves no error", async () => {
    const { result, client, getState } = setup({ kind: "Done", label: "Stage a.txt" });
    let outcome: UndoOutcome | null = null;
    await act(async () => {
      outcome = await result.current.undo();
    });
    expect(client.undo).toHaveBeenCalledWith("/repo");
    expect(outcome).toEqual({ kind: "Done", label: "Stage a.txt" });
    expect(getState().error).toBeNull();
  });

  it("shows the repository-changed message for undo and redo", async () => {
    const { result, getState } = setup({ kind: "RepositoryChanged", label: "Stage a.txt" });
    await act(async () => {
      await result.current.undo();
    });
    expect(getState().error).toBe(
      "Can't undo Stage a.txt: the repository changed since. Use the reflog to recover.",
    );
    await act(async () => {
      await result.current.redo();
    });
    expect(getState().error).toBe(
      "Can't redo Stage a.txt: the repository changed since. Use the reflog to recover.",
    );
  });

  it("asks to finish the merge or rebase first", async () => {
    const { result, getState } = setup({ kind: "OperationInProgress" });
    await act(async () => {
      await result.current.undo();
    });
    expect(getState().error).toBe("Finish or abort the merge or rebase first.");
  });

  it("stays silent when there is nothing to undo", async () => {
    const { result, getState } = setup({ kind: "Nothing" });
    await act(async () => {
      await result.current.undo();
    });
    expect(getState().error).toBeNull();
  });

  it("prefixes real failures and resolves null", async () => {
    const { result, getState } = setup(new Error("io error: permission denied"));
    let outcome: UndoOutcome | null = { kind: "Nothing" };
    await act(async () => {
      outcome = await result.current.undo();
    });
    expect(outcome).toBeNull();
    expect(getState().error).toBe("Undo failed: io error: permission denied");
  });
});
