import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import type { AppState } from "./useAppState";
import { RepoError } from "../ipc/RepoClient";
import { credentialRemediation } from "../lib/errorMessages";
import { credentialFailureMessage, failureError, useMutationRunner, withoutStaleErrorHint } from "./useMutationRunner";

const BASE_STATE: AppState = {
  repoPath: "/repo",
  selectedRow: "uncommitted",
  status: [],
  commits: [],
  graphLimit: 300,
  hasMoreHistory: false,
  graphBranchSelection: null,
  graphRemoteBranchSelection: null,
  branches: [],
  worktrees: [],
  submodules: [],
  reflogRefs: [],
  selectedReflogReference: null,
  reflog: [],
  remotes: [],
  tags: [],
  upstream: null,
  remoteUpstreams: {},
  forgeRepositories: [],
  pullRequests: {},
  createBranchDraft: null,
  stashes: [],
  mergeMessage: null,
  rebaseProgress: null,
  rebaseOnto: null,
  rebasePreset: null,
  rewordDialog: null,
  historyEditOutcome: null,
  patchOutcome: null,
  pendingPull: null,
  pullOutcome: null,
  mergeDialog: null,
  mergeOutcome: null,
  stashDialog: null,
  stashRestoreIndex: true,
  stashOutcome: null,
  transfer: null,
  error: null,
  errorHint: null,
  pending: false,
  addRemoteDraftOpen: false,
  remoteDialog: null,
  refreshGeneration: 0,
  headInfo: { headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
  forcePushOffer: null,
  forcePushConfirm: null,
  fetchPrune: null,
  pushDialog: null,
  deleteRemoteBranchDialog: null,
  createTagDialog: null,
  deleteRemoteTagDialog: null,
  amendMode: false,
  skipHooks: false,
  hookRun: null,
  hookFailure: null,
  clearCommitMessage: null,
  undoState: { undo: null, redo: null },
};

function setupRunner(initial: AppState = BASE_STATE) {
  return renderHook(() => {
    const [state, setState] = useState<AppState>(initial);
    const refresh = async () => {};
    const runner = useMutationRunner(refresh, setState);
    return { state, ...runner };
  });
}

describe("credentialFailureMessage", () => {
  // AUD-2026-09-05-CONC-002: `WorkerHandle` methods across `crates/repo-service/src/worker/*`
  // reject every subsequent call on a repo whose worker thread has died, with the `WorkerStopped`
  // kind (#496). It maps to a distinct reconnect message — retrying the same mutation can't help
  // once the worker itself is gone.
  it.each([
    "worker thread stopped",
    "worker thread stopped before replying",
  ])("maps a WorkerStopped %s to the reconnect message instead of surfacing it verbatim", (workerError) => {
    expect(credentialFailureMessage(new RepoError("WorkerStopped", workerError))).toBe(
      "Connection to this repository was lost. Close and reopen the repository to reconnect.",
    );
  });

  it("maps a CredentialStoreFailure to its remediation", () => {
    expect(credentialFailureMessage(new RepoError("CredentialStoreFailure", "credential keychain failure: locked"))).toBe(
      "The operating-system credential store is unavailable. Unlock it and try again.",
    );
  });

  // #496: only the kind counts; the old substring checks matched these texts.
  it("leaves other errors as they are, whatever their text", () => {
    expect(credentialFailureMessage(new Error("some other failure"))).toBe("some other failure");
    expect(credentialFailureMessage(new RepoError("Other", "worker thread stopped"))).toBe("worker thread stopped");
    expect(credentialFailureMessage("missing credential")).toBe("missing credential");
    expect(credentialFailureMessage(new RepoError("Other", "SSH agent failure"))).toBe("SSH agent failure");
  });
});

describe("credentialRemediation", () => {
  it.each([
    ["MissingCredential", "Save an HTTPS token for this remote before retrying."],
    ["CredentialStoreFailure", "The operating-system credential store is unavailable. Unlock it and try again."],
    ["SshAgentFailure", "Load a key into your SSH agent and try again."],
  ] as const)("gives %s its fixed hint", (kind, hint) => {
    expect(credentialRemediation(kind)).toBe(hint);
  });
});

describe("useMutationRunner's optimistic variants", () => {
  it("runOptimisticMutation applies the update before the mutation resolves, keeps it after success", async () => {
    const { result } = setupRunner();
    let resolveMutate: (() => void) | null = null;

    let mutationPromise!: Promise<void>;
    act(() => {
      mutationPromise = result.current.runOptimisticMutation(
        (prev) => ({ ...prev, branches: [{ name: "feature", isCurrent: false }] }),
        () => new Promise<void>((resolve) => { resolveMutate = resolve; }),
      );
    });

    expect(result.current.state.branches).toEqual([{ name: "feature", isCurrent: false }]);
    expect(result.current.state.pending).toBe(true);

    await act(async () => {
      resolveMutate?.();
      await mutationPromise;
    });

    expect(result.current.state.branches).toEqual([{ name: "feature", isCurrent: false }]);
    expect(result.current.state.pending).toBe(false);
  });

  it("runOptimisticMutation rolls back to the pre-call snapshot on failure", async () => {
    const { result } = setupRunner();

    await act(() =>
      result.current.runOptimisticMutation(
        (prev) => ({ ...prev, branches: [{ name: "feature", isCurrent: false }] }),
        async () => {
          throw new Error("boom");
        },
      ),
    );

    expect(result.current.state.branches).toEqual([]);
    expect(result.current.state.error).toBe("boom");
    expect(result.current.state.pending).toBe(false);
  });

  it("runOptimisticMutationWithMessage resolves null on success, restores state and resolves the message on failure", async () => {
    const { result } = setupRunner();

    const successResult = await act(() =>
      result.current.runOptimisticMutationWithMessage(
        (prev) => ({ ...prev, tags: [{ name: "v1", targetId: "", commitId: null, annotated: false, message: null, taggerName: null, timestamp: null }] }),
        async () => {},
      ),
    );
    expect(successResult).toBeNull();
    expect(result.current.state.tags).toHaveLength(1);

    const failureResult = await act(() =>
      result.current.runOptimisticMutationWithMessage(
        (prev) => ({ ...prev, tags: [...prev.tags, { name: "v2", targetId: "", commitId: null, annotated: false, message: null, taggerName: null, timestamp: null }] }),
        async () => {
          throw new Error("tag exists");
        },
      ),
    );
    expect(failureResult).toBe("tag exists");
    expect(result.current.state.tags).toHaveLength(1);
  });

  it("runOptimisticMutationWithOutcome resolves true on success, restores state and resolves false on failure", async () => {
    const { result } = setupRunner();

    const succeeded = await act(() =>
      result.current.runOptimisticMutationWithOutcome(
        (prev) => ({ ...prev, remotes: [{ name: "upstream", fetchUrl: "u", pushUrl: null, authMode: null, authUsername: null }] }),
        async () => {},
      ),
    );
    expect(succeeded).toBe(true);
    expect(result.current.state.remotes).toHaveLength(1);

    const failed = await act(() =>
      result.current.runOptimisticMutationWithOutcome(
        (prev) => ({ ...prev, remotes: prev.remotes.map((r) => ({ ...r, name: "renamed" })) }),
        async () => {
          throw new Error("rename failed");
        },
      ),
    );
    expect(failed).toBe(false);
    expect(result.current.state.remotes[0].name).toBe("upstream");
  });
});

// #177: an error stays on screen until it is dismissed or the next change starts — so every
// variant clears it when it starts, not when its trailing refresh lands.
describe("useMutationRunner clears a previous error when a mutation starts", () => {
  type Run = (runner: ReturnType<typeof setupRunner>["result"]["current"], mutate: () => Promise<void>) => Promise<unknown>;
  const variants: [string, Run][] = [
    ["runMutation", (r, m) => r.runMutation(m)],
    ["runMutationWithOutcome", (r, m) => r.runMutationWithOutcome(m)],
    ["runMutationWithMessage", (r, m) => r.runMutationWithMessage(m)],
    ["runOptimisticMutation", (r, m) => r.runOptimisticMutation((prev) => prev, m)],
    ["runOptimisticMutationWithMessage", (r, m) => r.runOptimisticMutationWithMessage((prev) => prev, m)],
    ["runOptimisticMutationWithOutcome", (r, m) => r.runOptimisticMutationWithOutcome((prev) => prev, m)],
  ];

  it.each(variants)("%s clears a previous error when it starts", async (_name, run) => {
    const { result } = setupRunner({ ...BASE_STATE, error: "old" });
    let resolveMutate: () => void = () => {};
    let done!: Promise<unknown>;
    act(() => {
      done = run(result.current, () => new Promise<void>((resolve) => { resolveMutate = resolve; }));
    });
    expect(result.current.state.error).toBeNull();
    expect(result.current.state.pending).toBe(true);
    await act(async () => {
      resolveMutate();
      await done;
    });
    expect(result.current.state.error).toBeNull();
    expect(result.current.state.pending).toBe(false);
  });

  it("an optimistic failure shows the new error, not the restored one", async () => {
    const { result } = setupRunner({ ...BASE_STATE, error: "old" });
    await act(() =>
      result.current.runOptimisticMutation(
        (prev) => ({ ...prev, branches: [{ name: "feature", isCurrent: false }] }),
        async () => {
          throw new Error("new");
        },
      ),
    );
    expect(result.current.state.error).toBe("new");
    expect(result.current.state.branches).toEqual([]);
  });
});

// #573: a failed request's banner text and the hint for its kind.
describe("failureError", () => {
  it("pairs the message with describeError's hint for the kind", () => {
    expect(failureError(new RepoError("PermissionDenied", "Permission denied"))).toEqual({
      error: "Permission denied",
      errorHint: "File permissions blocked this. Check that you can read and write the file or folder.",
    });
  });

  it("has no hint for a kind without one, or a plain error", () => {
    expect(failureError(new RepoError("CredentialStoreFailure", "locked"))).toEqual({
      error: credentialRemediation("CredentialStoreFailure"),
      errorHint: null,
    });
    expect(failureError(new Error("boom"))).toEqual({ error: "boom", errorHint: null });
  });
});

describe("withoutStaleErrorHint", () => {
  const failed = { ...BASE_STATE, error: "Permission denied", errorHint: "hint" };

  it("drops the hint when error changes without a new hint", () => {
    expect(withoutStaleErrorHint(failed, { ...failed, error: null }).errorHint).toBeNull();
    expect(withoutStaleErrorHint(failed, { ...failed, error: "other" }).errorHint).toBeNull();
  });

  it("keeps a hint set with its error, and one whose error is unchanged", () => {
    expect(withoutStaleErrorHint(BASE_STATE, failed)).toBe(failed);
    const next = { ...failed, pending: true };
    expect(withoutStaleErrorHint(failed, next)).toBe(next);
  });

  it("keeps the hint when the message is only prefixed, as Undo does", () => {
    const prefixed = { ...failed, error: "Undo failed: Permission denied" };
    expect(withoutStaleErrorHint(failed, prefixed)).toBe(prefixed);
  });
});
