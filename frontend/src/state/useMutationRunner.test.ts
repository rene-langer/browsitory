import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import type { AppState } from "./useAppState";
import { credentialFailureMessage, useMutationRunner } from "./useMutationRunner";

const BASE_STATE: AppState = {
  repoPath: "/repo",
  selectedRow: "uncommitted",
  status: [],
  commits: [],
  graphBranchSelection: null,
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
  squashPreset: null,
  pendingPull: null,
  pullOutcome: null,
  transfer: null,
  error: null,
  pending: false,
  addRemoteDraftOpen: false,
};

function setupRunner() {
  return renderHook(() => {
    const [state, setState] = useState<AppState>(BASE_STATE);
    const refresh = async () => {};
    const runner = useMutationRunner(refresh, setState);
    return { state, ...runner };
  });
}

describe("credentialFailureMessage", () => {
  // AUD-2026-09-05-CONC-002: `WorkerHandle` methods across `crates/repo-service/src/worker/*`
  // reject every subsequent call on a repo whose worker thread has died with one of these two
  // verbatim strings. Both should map to a distinct reconnect message — retrying the same
  // mutation can't help once the worker itself is gone.
  it.each([
    "worker thread stopped",
    "worker thread stopped before replying",
  ])("maps %s to the reconnect message instead of surfacing it verbatim", (workerError) => {
    expect(credentialFailureMessage(new Error(workerError))).toBe(
      "Connection to this repository was lost. Close and reopen the repository to reconnect.",
    );
  });

  it("leaves unrelated errors and existing credential remediations untouched", () => {
    expect(credentialFailureMessage(new Error("some other failure"))).toBe("some other failure");
    expect(credentialFailureMessage("missing credential")).toBe(
      "Save an HTTPS token for this remote before retrying.",
    );
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
        (prev) => ({ ...prev, tags: [{ name: "v1", targetId: "", annotated: false, message: null, taggerName: null, timestamp: null }] }),
        async () => {},
      ),
    );
    expect(successResult).toBeNull();
    expect(result.current.state.tags).toHaveLength(1);

    const failureResult = await act(() =>
      result.current.runOptimisticMutationWithMessage(
        (prev) => ({ ...prev, tags: [...prev.tags, { name: "v2", targetId: "", annotated: false, message: null, taggerName: null, timestamp: null }] }),
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
