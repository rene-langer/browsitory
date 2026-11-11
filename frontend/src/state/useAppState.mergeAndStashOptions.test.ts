import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("useAppState merge and stash options", () => {
  const defaultStash = { message: null, keepIndex: false, includeUntracked: true, paths: [] };

  it("merge with options persists them, merges with them and records the outcome", async () => {
    const calls: string[] = [];
    const setMergeOptions = vi.fn(async () => {
      calls.push("set");
    });
    const mergeBranch = vi.fn(async () => {
      calls.push("merge");
      return { kind: "Merged" as const };
    });
    const client = transferClient({ setMergeOptions, mergeBranch });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    const options = { fastForward: "Never", squash: false } as const;

    act(() => result.current.openDialog({ kind: "merge", branchName: "feature" }));
    expect(result.current.state.dialog).toEqual({ kind: "merge", branchName: "feature" });
    await act(() => result.current.mergeBranch("feature", options));

    expect(setMergeOptions).toHaveBeenCalledWith(TEST_REPO_PATH, options);
    expect(mergeBranch).toHaveBeenCalledWith(TEST_REPO_PATH, "feature", options);
    expect(calls).toEqual(["set", "merge"]);
    expect(result.current.state.dialog).toBeNull();
    expect(result.current.state.mergeOutcome).toEqual({ branchName: "feature", outcome: { kind: "Merged" } });
  });

  it("still merges when remembering the options fails", async () => {
    const mergeBranch = vi.fn(async () => ({ kind: "Squashed" as const }));
    const client = transferClient({
      setMergeOptions: async () => {
        throw new Error("disk full");
      },
      mergeBranch,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.mergeBranch("feature", { fastForward: "Allow", squash: true }));

    expect(mergeBranch).toHaveBeenCalled();
    expect(result.current.state.error).toBeNull();
    expect(result.current.state.mergeOutcome?.outcome).toEqual({ kind: "Squashed" });
  });

  it("a failed merge records no outcome and closes the dialog", async () => {
    const client = transferClient({
      setMergeOptions: async () => {},
      mergeBranch: async () => {
        throw new Error("Can't fast-forward: 'feature' has diverged from the current branch.");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    act(() => result.current.openDialog({ kind: "merge", branchName: "feature" }));

    await act(() => result.current.mergeBranch("feature", { fastForward: "Only", squash: false }));

    expect(result.current.state.mergeOutcome).toBeNull();
    expect(result.current.state.dialog).toBeNull();
    expect(result.current.state.error).toBe("Can't fast-forward: 'feature' has diverged from the current branch.");
  });

  it("closeDialog closes the merge dialog and getMergeOptions reads the client", async () => {
    const options = { fastForward: "Only", squash: false } as const;
    const client = transferClient({ getMergeOptions: async () => options });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    act(() => result.current.openDialog({ kind: "merge", branchName: "feature" }));
    act(() => result.current.closeDialog("merge"));

    expect(result.current.state.dialog).toBeNull();
    await expect(result.current.getMergeOptions()).resolves.toEqual(options);
  });

  it("pull merge passes the full upstream ref with Allow", async () => {
    const setMergeOptions = vi.fn(async () => {});
    const mergeBranch = vi.fn(async () => ({ kind: "Merged" as const }));
    const client = transferClient({ setMergeOptions, mergeBranch });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.mergePull("refs/remotes/origin/main"));

    expect(mergeBranch).toHaveBeenCalledWith(TEST_REPO_PATH, "refs/remotes/origin/main", {
      fastForward: "Allow",
      squash: false,
    });
    expect(setMergeOptions).not.toHaveBeenCalled();
  });

  it("a no-ff pull merges with a merge commit, and a fast-forward-only one with Only (#740)", async () => {
    const setMergeOptions = vi.fn(async () => {});
    const mergeBranch = vi.fn(async () => ({ kind: "Merged" as const }));
    const client = transferClient({ setMergeOptions, mergeBranch });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.mergePull("refs/remotes/origin/main", "Never"));
    expect(mergeBranch).toHaveBeenLastCalledWith(TEST_REPO_PATH, "refs/remotes/origin/main", {
      fastForward: "Never",
      squash: false,
    });
    await act(() => result.current.mergePull("refs/remotes/origin/main", "Only"));
    expect(mergeBranch).toHaveBeenLastCalledWith(TEST_REPO_PATH, "refs/remotes/origin/main", {
      fastForward: "Only",
      squash: false,
    });
    expect(setMergeOptions).not.toHaveBeenCalled();
  });

  it("saveStash defaults to stashing everything", async () => {
    const saveStash = vi.fn(async () => {});
    const client = transferClient({ saveStash });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.saveStash());

    expect(saveStash).toHaveBeenCalledWith(TEST_REPO_PATH, defaultStash);
  });

  it("saveStash passes dialog options and closes the dialog", async () => {
    const saveStash = vi.fn(async () => {});
    const client = transferClient({ saveStash });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    const options = { message: "wip", keepIndex: true, includeUntracked: false, paths: ["a.txt"] };

    act(() => result.current.openDialog({ kind: "stash", paths: ["a.txt"] }));
    expect(result.current.state.dialog).toEqual({ kind: "stash", paths: ["a.txt"] });
    await act(() => result.current.saveStash(options));

    expect(saveStash).toHaveBeenCalledWith(TEST_REPO_PATH, options);
    expect(result.current.state.dialog).toBeNull();
    act(() => result.current.openDialog({ kind: "stash", paths: [] }));
    act(() => result.current.closeDialog("stash"));
    expect(result.current.state.dialog).toBeNull();
  });

  it("pop as unstaged passes restoreIndex false and resets a popped selected stash", async () => {
    const stash = { index: 0, message: "On main: wip", commitId: "s0", branch: "main", timestamp: 1700000000, fileCount: 1, hasStagedChanges: false };
    const popStash = vi.fn(async () => ({ kind: "Applied" as const }));
    const client = transferClient({ listStashes: async () => [stash], popStash });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    act(() => result.current.selectRow({ commitId: "s0" }));
    await act(() => result.current.popStash(0, false));

    expect(popStash).toHaveBeenCalledWith(TEST_REPO_PATH, 0, "s0", false);
    expect(result.current.state.selectedRow).toBe("uncommitted");
    expect(result.current.state.stashOutcome).toEqual({ action: "pop", outcome: { kind: "Applied" } });
  });

  it("a conflicted pop keeps the stash selected", async () => {
    const stash = { index: 0, message: "On main: wip", commitId: "s0", branch: "main", timestamp: 1700000000, fileCount: 1, hasStagedChanges: false };
    const client = transferClient({
      listStashes: async () => [stash],
      popStash: async () => ({ kind: "Conflicted" as const, files: ["a.txt"] }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "s0" }));

    await act(() => result.current.popStash(0));

    expect(result.current.state.selectedRow).toEqual({ commitId: "s0" });
    expect(result.current.state.stashOutcome).toEqual({
      action: "pop",
      outcome: { kind: "Conflicted", files: ["a.txt"] },
    });
  });

  it("apply restores staged changes by default and records the outcome", async () => {
    const applyStash = vi.fn(async () => ({ kind: "Applied" as const }));
    const client = transferClient({
      applyStash,
      listStashes: async () => [
        { index: 0, message: "top", commitId: "s0", branch: null, timestamp: 1700000000, fileCount: 1, hasStagedChanges: false },
        { index: 1, message: "older", commitId: "s1", branch: null, timestamp: 1700000000, fileCount: 1, hasStagedChanges: false },
      ],
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.applyStash(1));

    expect(applyStash).toHaveBeenCalledWith(TEST_REPO_PATH, 1, "s1", true);
    expect(result.current.state.stashOutcome).toEqual({ action: "apply", outcome: { kind: "Applied" } });
  });

  it("refuses a stash index missing from the listed stashes without calling the backend", async () => {
    const applyStash = vi.fn(async () => ({ kind: "Applied" as const }));
    const popStash = vi.fn(async () => ({ kind: "Applied" as const }));
    const dropStash = vi.fn(async () => {});
    const client = transferClient({ applyStash, popStash, dropStash, listStashes: async () => [] });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.applyStash(0));
    expect(result.current.state.error).toBe("The stash list changed; refresh and try again.");
    await act(() => result.current.popStash(0));
    await act(() => result.current.dropStash(0));

    expect(applyStash).not.toHaveBeenCalled();
    expect(popStash).not.toHaveBeenCalled();
    expect(dropStash).not.toHaveBeenCalled();
  });
});
