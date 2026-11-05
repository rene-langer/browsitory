import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import type { CommitOutcome, HookProgress, PickOutcome, SequenceOutcome, TransferProgress } from "../ipc/RepoClient";
import { retryLabel } from "../lib/hookFailure";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

describe("git hooks", () => {
  it("stores a commit hook failure with its retry and reports the error", async () => {
    const commit = vi.fn(async (): Promise<CommitOutcome> => ({ kind: "HookFailed", hook: "pre-commit", output: "lint\n" }));
    const { result } = renderHook(() => useAppState(transferClient({ commit }), TEST_REPO_PATH));

    let ok = true;
    await act(async () => {
      ok = await result.current.commit("msg", false);
    });

    expect(ok).toBe(false);
    expect(commit).toHaveBeenCalledWith(TEST_REPO_PATH, "msg", false, []);
    expect(result.current.state.error).toBe("The pre-commit hook failed.");
    expect(result.current.state.hookFailure).toEqual({
      hook: "pre-commit",
      output: "lint\n",
      retry: { kind: "commit", message: "msg", trailers: [] },
      dialogOpen: true,
    });
  });

  it("retries a commit without hooks and asks the CommitBox to clear", async () => {
    const commit = vi
      .fn<RepoClient["commit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "pre-commit", output: "" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const { result } = renderHook(() => useAppState(transferClient({ commit }), TEST_REPO_PATH));
    await act(async () => void (await result.current.commit("msg", false)));

    await act(async () => {
      await result.current.retryWithoutHooks({ kind: "commit", message: "msg", trailers: [] });
    });

    expect(commit).toHaveBeenLastCalledWith(TEST_REPO_PATH, "msg", true, []);
    expect(result.current.state.hookFailure).toBeNull();
    expect(result.current.state.error).toBeNull();
    expect(result.current.state.clearCommitMessage).toBe("msg");
    act(() => result.current.clearCommitMessageHandled());
    expect(result.current.state.clearCommitMessage).toBeNull();
  });

  it("stores an amend hook failure and retries it as an amend", async () => {
    const amendCommit = vi
      .fn<RepoClient["amendCommit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "commit-msg", output: "bad subject\n" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const { result } = renderHook(() => useAppState(transferClient({ amendCommit }), TEST_REPO_PATH));
    act(() => result.current.setAmendMode(true));
    await act(async () => void (await result.current.amendCommit("m", false)));
    expect(result.current.state.hookFailure?.retry).toEqual({ kind: "amend", message: "m", options: { author: null, trailers: [] } });
    expect(result.current.state.amendMode).toBe(true);

    await act(async () => result.current.retryWithoutHooks({ kind: "amend", message: "m", options: { author: null, trailers: [] } }));

    expect(amendCommit).toHaveBeenLastCalledWith(TEST_REPO_PATH, "m", true, { author: null, trailers: [] });
    expect(result.current.state.amendMode).toBe(false);
    expect(result.current.state.clearCommitMessage).toBeNull();
  });

  it("keeps trailers and the amend author through a hook failure and its retry", async () => {
    const trailers = [{ key: "Co-authored-by", value: "Ada <ada@x.org>" }];
    const commit = vi
      .fn<RepoClient["commit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "commit-msg", output: "" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const amendCommit = vi
      .fn<RepoClient["amendCommit"]>()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "commit-msg", output: "" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c2" });
    const { result } = renderHook(() => useAppState(transferClient({ commit, amendCommit }), TEST_REPO_PATH));

    await act(async () => void (await result.current.commit("msg", false, trailers)));
    expect(commit).toHaveBeenLastCalledWith(TEST_REPO_PATH, "msg", false, trailers);
    const commitRetry = result.current.state.hookFailure!.retry;
    expect(commitRetry).toEqual({ kind: "commit", message: "msg", trailers });
    await act(async () => void (await result.current.retryWithoutHooks(commitRetry)));
    expect(commit).toHaveBeenLastCalledWith(TEST_REPO_PATH, "msg", true, trailers);

    const options = { author: { name: "Bob", email: "bob@x.org" }, trailers };
    await act(async () => void (await result.current.amendCommit("m", false, options)));
    expect(amendCommit).toHaveBeenLastCalledWith(TEST_REPO_PATH, "m", false, options);
    const amendRetry = result.current.state.hookFailure!.retry;
    expect(amendRetry).toEqual({ kind: "amend", message: "m", options });
    await act(async () => void (await result.current.retryWithoutHooks(amendRetry)));
    expect(amendCommit).toHaveBeenLastCalledWith(TEST_REPO_PATH, "m", true, options);
  });

  it("skip hooks applies to the next commit only", async () => {
    const commit = vi.fn(async (): Promise<CommitOutcome> => ({ kind: "Committed", commitId: "c" }));
    const { result } = renderHook(() => useAppState(transferClient({ commit }), TEST_REPO_PATH));
    act(() => result.current.setSkipHooks(true));

    await act(async () => void (await result.current.commit("m", result.current.state.skipHooks)));

    expect(commit).toHaveBeenCalledWith(TEST_REPO_PATH, "m", true, []);
    expect(result.current.state.skipHooks).toBe(false);
  });

  it("keeps skip hooks after a failed commit", async () => {
    const commit = vi.fn(async (): Promise<CommitOutcome> => {
      throw new Error("nothing to commit");
    });
    const { result } = renderHook(() => useAppState(transferClient({ commit }), TEST_REPO_PATH));
    act(() => result.current.setSkipHooks(true));

    await act(async () => void (await result.current.commit("m", true)));

    expect(result.current.state.skipHooks).toBe(true);
  });

  it("tracks the running hook for this repo only and clears it when the commit settles", async () => {
    let emit: ((event: HookProgress) => void) | undefined;
    let finish: ((outcome: CommitOutcome) => void) | undefined;
    const client = transferClient({
      subscribeHookProgress: (listener: (event: HookProgress) => void) => {
        emit = listener;
        return () => {};
      },
      commit: () => new Promise<CommitOutcome>((resolve) => (finish = resolve)),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let pending: Promise<boolean> | undefined;
    act(() => {
      pending = result.current.commit("m", false);
    });
    act(() => emit?.({ repoPath: "/other", operationId: "commit-9", hook: "pre-commit", phase: "Started", source: "Commit" }));
    expect(result.current.state.hookRun).toBeNull();
    act(() => emit?.({ repoPath: TEST_REPO_PATH, operationId: "commit-1", hook: "pre-commit", phase: "Started", source: "Commit" }));
    expect(result.current.state.hookRun).toEqual({ operationId: "commit-1", hook: "pre-commit", source: "Commit" });

    await act(async () => {
      finish?.({ kind: "Committed", commitId: "c" });
      await pending;
    });
    expect(result.current.state.hookRun).toBeNull();
  });

  it("keeps a hook's source, so the UI can show it in the right place (#185)", () => {
    let emit: ((event: HookProgress) => void) | undefined;
    const client = transferClient({
      subscribeHookProgress: (listener: (event: HookProgress) => void) => {
        emit = listener;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    act(() =>
      emit?.({ repoPath: TEST_REPO_PATH, operationId: "switch-1", hook: "post-checkout", phase: "Started", source: "Checkout" }),
    );

    expect(result.current.state.hookRun).toEqual({ operationId: "switch-1", hook: "post-checkout", source: "Checkout" });
  });

  it("opens the hook dialog without a retry when a cherry-pick's prepare-commit-msg fails (#183)", async () => {
    const client = transferClient({
      cherryPick: async () => ({ kind: "HookFailed" as const, hook: "prepare-commit-msg", output: "no ticket\n" }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: PickOutcome | null | undefined;
    await act(async () => {
      outcome = await result.current.cherryPick("abc");
    });

    expect(outcome).toBeNull();
    expect(result.current.state.error).toBe("The prepare-commit-msg hook failed.");
    expect(result.current.state.hookFailure).toEqual({
      hook: "prepare-commit-msg",
      output: "no ticket\n",
      retry: { kind: "pick" },
      dialogOpen: true,
    });
    expect(retryLabel(result.current.state.hookFailure!)).toBeNull();
  });

  it("opens the hook dialog when a sequence step's prepare-commit-msg fails (#183)", async () => {
    const client = transferClient({
      cherryPickMany: async () => ({
        kind: "HookFailed" as const,
        hook: "prepare-commit-msg",
        output: "no ticket\n",
        progress: { kind: "CherryPick" as const, currentStep: 2, totalSteps: 3, currentCommit: "b" },
      }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(async () => {
      await result.current.cherryPickMany(["a", "b", "c"]);
    });

    expect(result.current.state.hookFailure?.retry).toEqual({ kind: "continueSequence" });
    expect(result.current.state.hookFailure?.hook).toBe("prepare-commit-msg");
    expect(result.current.state.error).toBe("The prepare-commit-msg hook failed.");
  });

  it("refreshes after a pick whose prepare-commit-msg failed: the pick is staged and in progress (#183)", async () => {
    const getHeadInfo = vi.fn(async () => ({
      headId: null,
      detachedAt: null,
      headPushed: false,
      operation: null,
      operationLeftover: false,
      squashMessage: null,
      commitHooks: [],
      sequence: null,
      rebase: null,
      bisect: null, terminalSequence: null, terminalRebase: null,
    }));
    const getStatus = vi.fn(async () => []);
    const client = transferClient({
      getHeadInfo,
      getStatus,
      cherryPick: async () => ({ kind: "HookFailed" as const, hook: "prepare-commit-msg", output: "no\n" }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {});
    const headInfoCalls = getHeadInfo.mock.calls.length;
    const statusCalls = getStatus.mock.calls.length;

    await act(async () => {
      await result.current.cherryPick("abc");
    });

    expect(getHeadInfo.mock.calls.length).toBeGreaterThan(headInfoCalls);
    expect(getStatus.mock.calls.length).toBeGreaterThan(statusCalls);
    expect(result.current.state.error).toBe("The prepare-commit-msg hook failed.");
    expect(result.current.state.pending).toBe(false);
  });

  it("refreshes after a rejected cherry-pick sequence, which may have committed steps (#183)", async () => {
    const getHeadInfo = vi.fn(async () => ({
      headId: null,
      detachedAt: null,
      headPushed: false,
      operation: null,
      operationLeftover: false,
      squashMessage: null,
      commitHooks: [],
      sequence: null,
      rebase: null,
      bisect: null, terminalSequence: null, terminalRebase: null,
    }));
    const getStatus = vi.fn(async () => []);
    const client = transferClient({
      getHeadInfo,
      getStatus,
      cherryPickMany: async () => {
        throw new Error("could not write CHERRY_PICK_HEAD");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => {});
    const headInfoCalls = getHeadInfo.mock.calls.length;
    const statusCalls = getStatus.mock.calls.length;

    let outcome: SequenceOutcome | null | undefined;
    await act(async () => {
      outcome = await result.current.cherryPickMany(["a", "b", "c"]);
    });

    expect(outcome).toBeNull();
    expect(getHeadInfo.mock.calls.length).toBeGreaterThan(headInfoCalls);
    expect(getStatus.mock.calls.length).toBeGreaterThan(statusCalls);
    expect(result.current.state.error).toBe("could not write CHERRY_PICK_HEAD");
    expect(result.current.state.pending).toBe(false);
  });

  it("resolves a sequence stopped by Cancel as an outcome, not an error (#183)", async () => {
    const stopped = {
      kind: "Stopped" as const,
      kept: "f one",
      progress: { kind: "CherryPick" as const, currentStep: 2, totalSteps: 3, currentCommit: "b" },
    };
    const client = transferClient({ cherryPickMany: async () => stopped });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let outcome: SequenceOutcome | null | undefined;
    await act(async () => {
      outcome = await result.current.cherryPickMany(["a", "b", "c"]);
    });

    expect(outcome).toEqual(stopped);
    expect(result.current.state.error).toBeNull();
  });

  it("cancels the running hook through cancelTransfer", async () => {
    const cancelTransfer = vi.fn(async () => {});
    let emit: ((event: HookProgress) => void) | undefined;
    const client = transferClient({
      cancelTransfer,
      subscribeHookProgress: (listener: (event: HookProgress) => void) => {
        emit = listener;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    act(() => emit?.({ repoPath: TEST_REPO_PATH, operationId: "commit-1", hook: "pre-commit", phase: "Started", source: "Commit" }));

    await act(async () => result.current.cancelHookRun());

    expect(cancelTransfer).toHaveBeenCalledWith(TEST_REPO_PATH, "commit-1");
  });

  // #182: the strip's Cancel is on screen as soon as React commits the Started render, before
  // its passive effects run. A click in that gap must still reach the hook (the Tauri e2e
  // `git-hooks.spec.ts` clicked there, and the cancel was lost while the button latched).
  it("cancels a hook that started but whose render effects have not run yet", async () => {
    const cancelTransfer = vi.fn(async () => {});
    let emit: ((event: HookProgress) => void) | undefined;
    const client = transferClient({
      cancelTransfer,
      subscribeHookProgress: (listener: (event: HookProgress) => void) => {
        emit = listener;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    // One act: React renders and runs effects only when it ends, after the cancel.
    await act(async () => {
      emit?.({ repoPath: TEST_REPO_PATH, operationId: "checkout-1", hook: "post-checkout", phase: "Started", source: "Checkout" });
      await result.current.cancelHookRun();
    });

    expect(cancelTransfer).toHaveBeenCalledWith(TEST_REPO_PATH, "checkout-1");
  });

  // #182 review: a hook that started and finished in one React batch leaves `hookRun` null
  // before and after, so the sync effect doesn't run; Finished itself must clear the target.
  it("cancels nothing after a hook that started and finished in one batch", async () => {
    const cancelTransfer = vi.fn(async () => {});
    let emit: ((event: HookProgress) => void) | undefined;
    const client = transferClient({
      cancelTransfer,
      subscribeHookProgress: (listener: (event: HookProgress) => void) => {
        emit = listener;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    const event = { repoPath: TEST_REPO_PATH, operationId: "checkout-1", hook: "post-checkout", source: "Checkout" as const };

    act(() => {
      emit?.({ ...event, phase: "Started" });
      emit?.({ ...event, phase: "Finished" });
    });
    await act(async () => result.current.cancelHookRun());

    expect(cancelTransfer).not.toHaveBeenCalled();
  });

  it("turns a HookFailed push into a retryable hook failure", async () => {
    let listener: ((progress: TransferProgress) => void) | undefined;
    const pushCurrentBranch = vi.fn(async () => "push-1");
    const client = transferClient({
      pushCurrentBranch,
      subscribeTransferProgress: (next: (progress: TransferProgress) => void) => {
        listener = next;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => result.current.pushCurrentBranch("origin"));
    expect(pushCurrentBranch).toHaveBeenCalledWith(TEST_REPO_PATH, "origin", false);

    act(() =>
      listener?.({
        operationId: "push-1",
        operation: "PushBranch",
        phase: "Failed",
        errorKind: "HookFailed",
        current: 0,
        total: 0,
        receivedBytes: 0,
        message: null,
        hookOutput: "tests failed\n",
      }),
    );

    expect(result.current.state.error).toBe("The pre-push hook failed.");
    expect(result.current.state.hookFailure).toEqual({
      hook: "pre-push",
      output: "tests failed\n",
      retry: { kind: "push", push: { operation: "PushBranch", remoteName: "origin" } },
      dialogOpen: true,
    });

    await act(async () => result.current.retryWithoutHooks(result.current.state.hookFailure!.retry));
    expect(pushCurrentBranch).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", true);
  });

  it("retries a Push to… and a remote branch delete with the same arguments", async () => {
    const pushBranch = vi.fn(async () => "push-2");
    const deleteRemoteBranch = vi.fn(async () => "delete-1");
    const { result } = renderHook(() => useAppState(transferClient({ pushBranch, deleteRemoteBranch }), TEST_REPO_PATH));

    await act(async () =>
      result.current.retryWithoutHooks({
        kind: "push",
        push: { operation: "PushBranchTo", remoteName: "origin", localBranch: "topic", remoteBranch: "pub", setUpstream: true },
      }),
    );
    expect(pushBranch).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", "topic", "pub", true, true);

    await act(async () =>
      result.current.retryWithoutHooks({
        kind: "push",
        push: { operation: "DeleteRemoteBranch", remoteName: "origin", branchName: "old" },
      }),
    );
    expect(deleteRemoteBranch).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", "old", true);
  });

  it("dismissing the error forgets the hook failure; closing the dialog keeps it", async () => {
    const commit = vi.fn(async (): Promise<CommitOutcome> => ({ kind: "HookFailed", hook: "pre-commit", output: "" }));
    const { result } = renderHook(() => useAppState(transferClient({ commit }), TEST_REPO_PATH));
    await act(async () => void (await result.current.commit("m", false)));

    act(() => result.current.closeHookFailure());
    expect(result.current.state.hookFailure?.dialogOpen).toBe(false);
    act(() => result.current.openHookFailure());
    expect(result.current.state.hookFailure?.dialogOpen).toBe(true);
    act(() => result.current.dismissError());
    expect(result.current.state.hookFailure).toBeNull();
  });
});
