import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RepoError } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("useAppState git defaults (#652, #653, #656, #689)", () => {
  it("a switch refused by local changes opens the Stash and switch dialog with the files", async () => {
    const client = transferClient({
      switchBranch: async () => {
        throw new RepoError("LocalChanges", "Your local changes would be overwritten by switching to feature.");
      },
      getSwitchConflicts: vi.fn(async () => ["a.txt"]),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.switchBranch("feature"));

    expect(result.current.state.switchConflict).toEqual({ branchName: "feature", files: ["a.txt"] });
    expect(result.current.state.error).toContain("would be overwritten");
    act(() => result.current.closeSwitchConflict());
    expect(result.current.state.switchConflict).toBeNull();
  });

  it("another switch failure opens no dialog", async () => {
    const getSwitchConflicts = vi.fn(async () => []);
    const client = transferClient({
      switchBranch: async () => {
        throw new RepoError("Other", "branch 'feature' is checked out in another worktree");
      },
      getSwitchConflicts,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.switchBranch("feature"));

    expect(result.current.state.switchConflict).toBeNull();
    expect(getSwitchConflicts).not.toHaveBeenCalled();
  });

  it("Stash and switch closes the dialog and records the outcome for its toast", async () => {
    const stashAndSwitch = vi.fn(async () => ({ kind: "Reapplied" as const }));
    const client = transferClient({ stashAndSwitch });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: unknown;
    await act(async () => {
      outcome = await result.current.stashAndSwitch("feature", true);
    });

    expect(stashAndSwitch).toHaveBeenCalledWith(TEST_REPO_PATH, "feature", true);
    expect(outcome).toEqual({ kind: "Reapplied" });
    expect(result.current.state.stashSwitchOutcome).toEqual({ branchName: "feature", outcome: { kind: "Reapplied" } });
    expect(result.current.state.switchConflict).toBeNull();
  });

  it("creates a branch with or without checking it out", async () => {
    const createBranch = vi.fn(async () => {});
    const client = transferClient({ createBranch });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.createBranch("a", "HEAD"));
    await act(() => result.current.createBranch("b", "HEAD", false));

    expect(createBranch).toHaveBeenNthCalledWith(1, TEST_REPO_PATH, "a", "HEAD", true);
    expect(createBranch).toHaveBeenNthCalledWith(2, TEST_REPO_PATH, "b", "HEAD", false);
  });

  it("a commit without identity asks for it, and setting it closes the dialog", async () => {
    const setIdentity = vi.fn(async () => {});
    const client = transferClient({
      commit: async () => {
        throw new RepoError("MissingIdentity", "Set your name and email for git before committing.");
      },
      setIdentity,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(async () => {
      await result.current.commit("message", false);
    });
    expect(result.current.state.identityRequired).toBe("commit");

    let failure: string | null = "unset";
    await act(async () => {
      failure = await result.current.setIdentity("Ada", "ada@example.com", true);
    });
    expect(failure).toBeNull();
    expect(setIdentity).toHaveBeenCalledWith(TEST_REPO_PATH, "Ada", "ada@example.com", true);
    expect(result.current.state.identityRequired).toBeNull();
  });

  it("a diverged pull's rebase picks every commit onto the upstream, without the planner", async () => {
    const startRebase = vi.fn(async () => ({ kind: "Done" as const }));
    const client = transferClient({
      commitsSince: async () => [
        { id: "a".repeat(40), shortId: "aaaaaaa", summary: "one", authorName: "A", timestamp: 0 },
        { id: "b".repeat(40), shortId: "bbbbbbb", summary: "two", authorName: "A", timestamp: 0 },
      ],
      startRebase,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.rebasePull("refs/remotes/origin/main"));

    expect(startRebase).toHaveBeenCalledWith(TEST_REPO_PATH, "refs/remotes/origin/main", [
      { commitId: "a".repeat(40), action: { kind: "Pick" }, combinedMessage: null },
      { commitId: "b".repeat(40), action: { kind: "Pick" }, combinedMessage: null },
    ]);
    expect(result.current.state.rebaseOnto).toBeNull();
    expect(result.current.state.historyEditOutcome).toEqual({ message: "Rebased onto origin/main" });
  });
});
