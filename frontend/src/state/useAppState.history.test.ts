import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { CommitOutcome, TransferProgress } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("back and forward through the selected rows (#618)", () => {
  const always = () => true;
  const selected = (result: { current: ReturnType<typeof useAppState> }) => result.current.state.selectedRow;

  it("goes back and forward through the selections, merging keyboard moves", () => {
    const { result } = renderHook(() => useAppState(transferClient({}), TEST_REPO_PATH));
    act(() => result.current.selectRow({ commitId: "a" }));
    act(() => result.current.selectRow({ commitId: "b" }, "step"));
    act(() => result.current.selectRow({ commitId: "c" }, "step"));
    act(() => result.current.selectRow({ commitId: "d" }));

    let row: unknown;
    act(() => {
      row = result.current.stepNavigation(-1, always);
    });
    expect(row).toEqual({ commitId: "c" });
    expect(selected(result)).toEqual({ commitId: "c" });
    act(() => {
      result.current.stepNavigation(-1, always);
    });
    expect(selected(result)).toEqual({ commitId: "a" });
    act(() => {
      result.current.stepNavigation(-1, always);
    });
    expect(selected(result)).toBe("uncommitted");
    act(() => {
      row = result.current.stepNavigation(-1, always);
    });
    expect(row).toBeNull();
    act(() => {
      result.current.stepNavigation(1, always);
    });
    act(() => {
      result.current.stepNavigation(1, always);
    });
    expect(selected(result)).toEqual({ commitId: "c" });
    // Selecting the row Back landed on is no new entry: Forward still leads to d.
    act(() => result.current.selectRow({ commitId: "c" }));
    act(() => {
      result.current.stepNavigation(1, always);
    });
    expect(selected(result)).toEqual({ commitId: "d" });
  });

  it("skips a commit that is no longer available", () => {
    const { result } = renderHook(() => useAppState(transferClient({}), TEST_REPO_PATH));
    act(() => result.current.selectRow({ commitId: "a" }));
    act(() => result.current.selectRow({ commitId: "gone" }));
    act(() => result.current.selectRow({ commitId: "c" }));
    act(() => {
      result.current.stepNavigation(-1, (row) => row === "uncommitted" || row.commitId !== "gone");
    });
    expect(selected(result)).toEqual({ commitId: "a" });
  });
});

describe("history actions", () => {
  it("loads headInfo on refresh", async () => {
    const info = { headId: "h1", detachedAt: null, headPushed: true, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null };
    const { result } = renderHook(() => useAppState(transferClient({ getHeadInfo: async () => info }), TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.headInfo).toEqual(info);
  });

  it("derives rebase progress from headInfo.rebase on the first refresh", async () => {
    // What a restarted app sees: the backend restored the paused rebase from disk.
    const rebase = { branch: "main", onto: "o1", currentStep: 1, totalSteps: 2, editing: null, health: { kind: "Paused" as const } };
    const info = { headId: "h1", detachedAt: "h1", headPushed: false, operation: "CherryPick" as const, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase, bisect: null, terminalSequence: null, terminalRebase: null };
    const { result } = renderHook(() => useAppState(transferClient({ getHeadInfo: async () => info }), TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.rebaseProgress).toEqual({ currentStep: 1, totalSteps: 2 });
  });

  it("clears rebase progress when headInfo.rebase is null", async () => {
    const rebase = { branch: "main", onto: "o1", currentStep: 2, totalSteps: 2, editing: null, health: { kind: "Paused" as const } };
    let current: typeof rebase | null = rebase;
    const getHeadInfo = async () => ({ headId: "h1", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: current, bisect: null, terminalSequence: null, terminalRebase: null });
    const { result } = renderHook(() => useAppState(transferClient({ getHeadInfo }), TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.rebaseProgress).toEqual({ currentStep: 2, totalSteps: 2 });
    current = null;
    await act(() => result.current.refresh());
    expect(result.current.state.rebaseProgress).toBeNull();
  });

  describe("one-click history edits (#442)", () => {
    const span = ["p", "c", "d"].map((id) => ({ id, shortId: `${id}123456`, summary: id, authorName: "A", timestamp: 1 }));
    const pick = (commitId: string) => ({ commitId, action: { kind: "Pick" }, combinedMessage: null });

    it("squashes with the parent's and the commit's full messages, then toasts", async () => {
      const commitsSince = vi.fn(async () => span);
      const getCommitMessage = vi.fn(async (_repo: string, id: string) => (id === "p" ? "Parent\n\nbody\n" : "Child\n"));
      const startRebase = vi.fn(async () => ({ kind: "Done" }) as const);
      const { result } = renderHook(() =>
        useAppState(transferClient({ commitsSince, getCommitMessage, startRebase }), TEST_REPO_PATH),
      );
      let outcome: unknown;
      await act(async () => {
        outcome = await result.current.editHistory("c", "g", { kind: "Squash" });
      });
      expect(commitsSince).toHaveBeenCalledWith(TEST_REPO_PATH, "g");
      expect(startRebase).toHaveBeenCalledWith(TEST_REPO_PATH, "g", [
        { commitId: "p", action: { kind: "Pick" }, combinedMessage: "Parent\n\nbody\n\nChild\n" },
        { commitId: "c", action: { kind: "Squash" }, combinedMessage: null },
        pick("d"),
      ], { kind: "Squash", commitId: "c" });
      expect(outcome).toEqual({ kind: "Done" });
      expect(result.current.state.historyEditOutcome).toEqual({ message: "Squashed c123456 into its parent" });
    });

    it("leaves a paused edit to the rebase panel, without a toast", async () => {
      const startRebase = vi.fn(async () => ({ kind: "Conflicted" as const, files: ["f"] }));
      const { result } = renderHook(() =>
        useAppState(transferClient({ commitsSince: async () => span.slice(1), startRebase }), TEST_REPO_PATH),
      );
      await act(async () => {
        await result.current.editHistory("c", "p", { kind: "MoveUp" });
      });
      expect(startRebase).toHaveBeenCalledWith(TEST_REPO_PATH, "p", [pick("d"), pick("c")], {
        kind: "MoveUp",
        commitId: "c",
      });
      expect(result.current.state.historyEditOutcome).toBeNull();
    });

    it("Edit commit… starts a rebase that stops at the commit, named for Undo (#882)", async () => {
      const startRebase = vi.fn(async () => ({ kind: "PausedForEdit" }) as const);
      const { result } = renderHook(() =>
        useAppState(transferClient({ commitsSince: async () => span.slice(1), startRebase }), TEST_REPO_PATH),
      );
      let outcome: unknown;
      await act(async () => {
        outcome = await result.current.editHistory("c", "p", { kind: "Edit" });
      });
      expect(startRebase).toHaveBeenCalledWith(
        TEST_REPO_PATH,
        "p",
        [{ commitId: "c", action: { kind: "Edit" }, combinedMessage: null }, pick("d")],
        { kind: "Edit", commitId: "c" },
      );
      expect(outcome).toEqual({ kind: "PausedForEdit" });
      expect(result.current.state.historyEditOutcome).toBeNull();
    });

    it("continues an edit stop with the edited message, else without one (#882)", async () => {
      const rebaseContinue = vi.fn(async () => ({ kind: "Done" }) as const);
      const { result } = renderHook(() => useAppState(transferClient({ rebaseContinue }), TEST_REPO_PATH));
      act(() => result.current.setRebaseEditMessage("New message\n"));
      await act(() => result.current.rebaseContinue());
      expect(rebaseContinue).toHaveBeenLastCalledWith(TEST_REPO_PATH, "New message\n");
      // Used once: the next Continue (another stop) has no edited message.
      await act(() => result.current.rebaseContinue());
      expect(rebaseContinue).toHaveBeenLastCalledWith(TEST_REPO_PATH);
      act(() => result.current.setRebaseEditMessage("Kept\n"));
      act(() => result.current.setRebaseEditMessage(null));
      await act(() => result.current.rebaseContinue());
      expect(rebaseContinue).toHaveBeenLastCalledWith(TEST_REPO_PATH);
    });

    it("folds staged changes into a commit, toasting when done (#883)", async () => {
      const foldStagedInto = vi.fn(async () => ({ kind: "Done" }) as const);
      const { result } = renderHook(() => useAppState(transferClient({ foldStagedInto }), TEST_REPO_PATH));
      await act(() => result.current.foldStaged("c1234567890"));
      expect(foldStagedInto).toHaveBeenCalledWith(TEST_REPO_PATH, "c1234567890");
      expect(result.current.state.historyEditOutcome).toEqual({ message: "Folded staged changes into c123456" });
    });

    it("leaves a paused fold to the rebase panel, without a toast (#883)", async () => {
      const foldStagedInto = vi.fn(async () => ({ kind: "Conflicted" as const, files: ["f"] }));
      const { result } = renderHook(() => useAppState(transferClient({ foldStagedInto }), TEST_REPO_PATH));
      await act(() => result.current.foldStaged("c1234567890"));
      expect(result.current.state.historyEditOutcome).toBeNull();
    });

    it("refuses without starting when the history changed since the menu opened", async () => {
      const startRebase = vi.fn();
      const { result } = renderHook(() =>
        useAppState(transferClient({ commitsSince: async () => span, startRebase }), TEST_REPO_PATH),
      );
      let outcome: unknown;
      await act(async () => {
        outcome = await result.current.editHistory("d", "p", { kind: "Drop" });
      });
      expect(outcome).toBeNull();
      expect(startRebase).not.toHaveBeenCalled();
      expect(result.current.state.error).toBe("The history changed. Refresh and try again.");
    });

    it("opens and closes the Reword dialog", () => {
      const { result } = renderHook(() => useAppState(transferClient({}), TEST_REPO_PATH));
      act(() => result.current.openDialog({ kind: "reword", commitId: "c", onto: "p" }));
      expect(result.current.state.dialog).toEqual({ kind: "reword", commitId: "c", onto: "p" });
      act(() => result.current.closeDialog("reword"));
      expect(result.current.state.dialog).toBeNull();
    });
  });

  it("cherryPick calls the client then refreshes", async () => {
    const cherryPick = vi.fn(async () => ({ kind: "Committed", commitId: "n1" }) as const);
    const getStatus = vi.fn(async () => []);
    const { result } = renderHook(() => useAppState(transferClient({ cherryPick, getStatus }), TEST_REPO_PATH));
    let outcome: unknown;
    await act(async () => {
      outcome = await result.current.cherryPick("c1");
    });
    expect(cherryPick).toHaveBeenCalledWith(TEST_REPO_PATH, "c1", null);
    expect(outcome).toEqual({ kind: "Committed", commitId: "n1" });
    expect(getStatus).toHaveBeenCalled();
    expect(result.current.state.error).toBeNull();
  });

  it("passes a mainline through and resolves sequences to their outcomes", async () => {
    const revertCommit = vi.fn(async () => ({ kind: "Conflicted" as const, files: ["a"] }));
    const completed = { kind: "Completed" as const, applied: 2, skipped: [] };
    const cherryPickMany = vi.fn(async () => completed);
    const revertMany = vi.fn(async () => completed);
    const continueSequence = vi.fn(async () => completed);
    const abortSequence = vi.fn(async () => ({ rewound: false }));
    const { result } = renderHook(() =>
      useAppState(
        transferClient({ revertCommit, cherryPickMany, revertMany, continueSequence, abortSequence }),
        TEST_REPO_PATH,
      ),
    );
    const outcomes: unknown[] = [];
    await act(async () => {
      outcomes.push(await result.current.revertCommit("m", 2));
      outcomes.push(await result.current.cherryPickMany(["a", "b"]));
      outcomes.push(await result.current.revertMany(["a"]));
      outcomes.push(await result.current.continueSequence());
      outcomes.push(await result.current.abortSequence());
    });
    expect(revertCommit).toHaveBeenCalledWith(TEST_REPO_PATH, "m", 2);
    expect(cherryPickMany).toHaveBeenCalledWith(TEST_REPO_PATH, ["a", "b"]);
    expect(revertMany).toHaveBeenCalledWith(TEST_REPO_PATH, ["a"]);
    expect(outcomes).toEqual([{ kind: "Conflicted", files: ["a"] }, completed, completed, completed, { rewound: false }]);
  });

  it("resolves null and sets state.error when a sequence call fails", async () => {
    const continueSequence = vi.fn(async () => {
      throw new Error("Resolve all conflicts before continuing.");
    });
    const { result } = renderHook(() => useAppState(transferClient({ continueSequence }), TEST_REPO_PATH));
    let outcome: unknown = "unset";
    await act(async () => {
      outcome = await result.current.continueSequence();
    });
    expect(outcome).toBeNull();
    expect(result.current.state.error).toContain("Resolve all conflicts");
  });

  it("surfaces a backend refusal as state.error", async () => {
    const revertCommit = vi.fn(async () => {
      throw new Error("This is a merge commit. Choose which parent to apply it against.");
    });
    const { result } = renderHook(() => useAppState(transferClient({ revertCommit }), TEST_REPO_PATH));
    await act(() => result.current.revertCommit("m1"));
    expect(result.current.state.error).toContain("Choose which parent");
  });

  it("resetToCommit and checkoutCommit pass their arguments through", async () => {
    const resetToCommit = vi.fn(async () => {});
    const checkoutCommit = vi.fn(async () => {});
    const { result } = renderHook(() =>
      useAppState(transferClient({ resetToCommit, checkoutCommit }), TEST_REPO_PATH),
    );
    await act(() => result.current.resetToCommit("c1", "hard"));
    await act(() => result.current.checkoutCommit("c2"));
    expect(resetToCommit).toHaveBeenCalledWith(TEST_REPO_PATH, "c1", "hard");
    expect(checkoutCommit).toHaveBeenCalledWith(TEST_REPO_PATH, "c2");
  });

  it("amendCommit turns amend mode off on success and keeps it on failure", async () => {
    let fail = true;
    const amendCommit = vi.fn(async (): Promise<CommitOutcome> => {
      if (fail) throw new Error("boom");
      return { kind: "Committed", commitId: "c" };
    });
    const { result } = renderHook(() => useAppState(transferClient({ amendCommit }), TEST_REPO_PATH));
    act(() => result.current.setAmendMode(true));
    await act(() => result.current.amendCommit("m", false));
    expect(result.current.state.amendMode).toBe(true);
    fail = false;
    await act(() => result.current.amendCommit("m", false));
    expect(result.current.state.amendMode).toBe(false);
  });

  it("offers a force push after a non-fast-forward branch push and runs it", async () => {
    let listener: ((progress: TransferProgress) => void) | null = null;
    const forcePushCurrentBranch = vi.fn(async () => "force-push-1");
    const client = transferClient({
      pushCurrentBranch: async () => "push-1",
      forcePushCurrentBranch,
      subscribeTransferProgress: (callback) => {
        listener = callback;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.pushCurrentBranch("origin"));
    act(() => listener!({ operationId: "push-1", operation: "PushBranch", phase: "Failed", errorKind: "NonFastForward", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null }));
    expect(result.current.state.forcePushOffer).toEqual({ remoteName: "origin" });

    act(() => result.current.openDialog({ kind: "forcePush", remoteName: "origin" }));
    expect(result.current.state.dialog).toEqual({ kind: "forcePush", remoteName: "origin" });

    await act(() => result.current.forcePushCurrentBranch("origin"));
    expect(forcePushCurrentBranch).toHaveBeenCalledWith(TEST_REPO_PATH, "origin", false);
    expect(result.current.state.forcePushOffer).toBeNull();
    expect(result.current.state.dialog).toBeNull();
  });

  it("cancelForcePush clears both the offer and the confirm", () => {
    const { result } = renderHook(() => useAppState(transferClient({}), TEST_REPO_PATH));
    act(() => result.current.openDialog({ kind: "forcePush", remoteName: "origin" }));
    act(() => result.current.cancelForcePush());
    expect(result.current.state.dialog).toBeNull();
    expect(result.current.state.forcePushOffer).toBeNull();
  });

  it("explains a stale lease", async () => {
    let listener: ((progress: TransferProgress) => void) | null = null;
    const client = transferClient({
      forcePushCurrentBranch: async () => "force-push-1",
      getCurrentUpstream: async () => ({ localBranch: "main", remoteName: "origin", remoteBranch: "main" }),
      subscribeTransferProgress: (callback) => {
        listener = callback;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.forcePushCurrentBranch("origin"));
    act(() => listener!({ operationId: "force-push-1", operation: "ForcePushBranch", phase: "Failed", errorKind: "StaleLease", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null }));
    expect(result.current.state.error).toBe(
      "origin/main changed since your last fetch. Fetch and review before force pushing.",
    );
  });

  it("labels a stale lease with the force-pushed remote and local branch, not the upstream", async () => {
    let listener: ((progress: TransferProgress) => void) | null = null;
    const client = transferClient({
      forcePushCurrentBranch: async () => "force-push-1",
      listBranches: async () => [{ name: "feature", isCurrent: true }],
      getCurrentUpstream: async () => ({ localBranch: "feature", remoteName: "origin", remoteBranch: "other" }),
      subscribeTransferProgress: (callback) => {
        listener = callback;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.forcePushCurrentBranch("fork"));
    act(() => listener!({ operationId: "force-push-1", operation: "ForcePushBranch", phase: "Failed", errorKind: "StaleLease", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null }));
    expect(result.current.state.error).toBe(
      "fork/feature changed since your last fetch. Fetch and review before force pushing.",
    );
  });

  it("gives a rejected force push its own message", async () => {
    let listener: ((progress: TransferProgress) => void) | null = null;
    const client = transferClient({
      forcePushCurrentBranch: async () => "force-push-1",
      subscribeTransferProgress: (callback) => {
        listener = callback;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.forcePushCurrentBranch("origin"));
    act(() => listener!({ operationId: "force-push-1", operation: "ForcePushBranch", phase: "Failed", errorKind: "NonFastForward", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null }));
    expect(result.current.state.error).toBe("The remote refused the force push.");
  });

  it("treats a failed force push as a push failure", async () => {
    let listener: ((progress: TransferProgress) => void) | null = null;
    const client = transferClient({
      forcePushCurrentBranch: async () => "force-push-1",
      subscribeTransferProgress: (callback) => {
        listener = callback;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.forcePushCurrentBranch("origin"));
    act(() => listener!({ operationId: "force-push-1", operation: "ForcePushBranch", phase: "Failed", errorKind: "TransferFailed", current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null }));
    expect(result.current.state.error).toBe("Push failed");
  });

  describe("force push offer lifecycle", () => {
    const failed = (operationId: string, operation: TransferProgress["operation"], errorKind: TransferProgress["errorKind"]): TransferProgress => ({
      operationId, operation, phase: "Failed", errorKind, current: 0, total: 0, receivedBytes: 0, message: null, hookOutput: null,
    });
    async function setup() {
      let listener: ((progress: TransferProgress) => void) | null = null;
      let n = 0;
      const client = transferClient({
        pushCurrentBranch: async () => `push-${++n}`,
        forcePushCurrentBranch: async () => "force-1",
        subscribeTransferProgress: (callback) => {
          listener = callback;
          return () => {};
        },
      });
      const hook = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => hook.result.current.pushCurrentBranch("origin"));
      act(() => listener!(failed("push-1", "PushBranch", "NonFastForward")));
      return { ...hook, emit: (p: TransferProgress) => act(() => listener!(p)) };
    }

    it("is cleared by dismissing the error", async () => {
      const { result } = await setup();
      expect(result.current.state.forcePushOffer).not.toBeNull();
      act(() => result.current.dismissError());
      expect(result.current.state.forcePushOffer).toBeNull();
    });

    it("is cleared when a new push starts", async () => {
      const { result } = await setup();
      await act(() => result.current.pushCurrentBranch("origin"));
      expect(result.current.state.forcePushOffer).toBeNull();
    });

    it("is cleared when a push succeeds", async () => {
      const { result, emit } = await setup();
      await act(() => result.current.pushCurrentBranch("origin"));
      emit({ ...failed("push-2", "PushBranch", null), phase: "Completed" });
      expect(result.current.state.forcePushOffer).toBeNull();
    });

    it("is not set by a force push failing non-fast-forward", async () => {
      const { result, emit } = await setup();
      act(() => result.current.dismissError());
      await act(() => result.current.forcePushCurrentBranch("origin"));
      emit(failed("force-1", "ForcePushBranch", "NonFastForward"));
      expect(result.current.state.forcePushOffer).toBeNull();
    });
  });
});
