import { useCallback } from "react";
import { errorKind } from "../ipc/RepoClient";
import { credentialRemediation, describeError } from "../lib/errorMessages";
import type { AppState } from "./useAppState";

// Shared by every domain action hook composed inside `useAppState`: wraps a mutation with the
// same pending/refresh/error bookkeeping, in one of three result shapes depending on how the
// caller needs to observe success/failure (see each variant below).
export type RunMutation = (mutate: () => Promise<void>) => Promise<void>;
export type RunMutationWithOutcome = (mutate: () => Promise<void>) => Promise<boolean>;
export type RunMutationWithMessage = (mutate: () => Promise<void>) => Promise<string | null>;
export type OptimisticUpdate = (prev: AppState) => AppState;
export type RunOptimisticMutation = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<void>;
export type RunOptimisticMutationWithMessage = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<string | null>;
export type RunOptimisticMutationWithOutcome = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<boolean>;

export interface MutationRunner {
  runMutation: RunMutation;
  runMutationWithOutcome: RunMutationWithOutcome;
  runMutationWithMessage: RunMutationWithMessage;
  runOptimisticMutation: RunOptimisticMutation;
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage;
  runOptimisticMutationWithOutcome: RunOptimisticMutationWithOutcome;
}

// The message shown for a `WorkerStopped` rejection (#496): the per-repo worker thread has crashed
// and can no longer take requests. A plain retry of the same action can't help here: the worker is
// gone, so every subsequent call on this repo will keep failing the same way until the repository
// is closed and reopened (which spawns a fresh worker). AUD-2026-09-05-CONC-002.
export const REPO_CONNECTION_LOST_MESSAGE =
  "Connection to this repository was lost. Close and reopen the repository to reconnect.";

/**
 * What a failed request shows: a fixed remediation for the kinds that have one (#496), else the
 * backend's message. `Error`s are unwrapped rather than stringified: `String(new Error("x"))` is
 * `"Error: x"`, and `RemotePanel` renders this message inline under the Fetch URL field, not just
 * in the generic error banner.
 */
export function credentialFailureMessage(error: unknown): string {
  switch (errorKind(error)) {
    case "WorkerStopped":
      return REPO_CONNECTION_LOST_MESSAGE;
    case "CredentialStoreFailure":
      return credentialRemediation("CredentialStoreFailure");
    default:
      return error instanceof Error ? error.message : String(error);
  }
}

/**
 * `AppState`'s `error` and `errorHint` for a failed request: `credentialFailureMessage`'s message
 * plus the hint `describeError` gives the error's kind (#573), so the banner explains a failed
 * mutation (a `PermissionDenied` Stage, say) the same way a failed read's inline error does.
 */
export function failureError(error: unknown): { error: string; errorHint: string | null } {
  return { error: credentialFailureMessage(error), errorHint: describeError(error).hint ?? null };
}

/**
 * Drops `errorHint` when an update changes `error` without setting a hint of its own (cleared,
 * dismissed, or replaced by a message set without `failureError`), so a hint never sits under
 * another error's message. A message that still contains the old one (Undo's "Undo failed: …"
 * prefix) is the same error and keeps its hint. `useAppState` runs every update through this.
 */
export function withoutStaleErrorHint(prev: AppState, next: AppState): AppState {
  if (next.errorHint === null || next.errorHint !== prev.errorHint || next.error === prev.error) return next;
  const reworded = next.error !== null && prev.error !== null && next.error.includes(prev.error);
  return reworded ? next : { ...next, errorHint: null };
}

// Builds the `runMutation*` variants that every domain action hook composed inside
// `useAppState` uses to wrap its mutations. All share the same pending/refresh/error bookkeeping
// against the top-level `AppState`; they differ only in what they resolve to (see each type
// above). Every variant clears `error` when it *starts* (#177): an error stays on screen until it
// is dismissed or the next change starts, and the trailing `refresh()` no longer clears it.
export function useMutationRunner(
  refresh: () => Promise<void>,
  setState: (updater: (prev: AppState) => AppState) => void,
): MutationRunner {
  const runMutation = useCallback<RunMutation>(
    async (mutate) => {
      try {
        setState((prev) => ({ ...prev, pending: true, error: null }));
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
      } catch (err) {
        setState((prev) => ({ ...prev, ...failureError(err), pending: false }));
      }
    },
    [refresh, setState],
  );

  const runMutationWithOutcome = useCallback<RunMutationWithOutcome>(
    async (mutate) => {
      try {
        setState((prev) => ({ ...prev, pending: true, error: null }));
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return true;
      } catch (err) {
        setState((prev) => ({ ...prev, ...failureError(err), pending: false }));
        return false;
      }
    },
    [refresh, setState],
  );

  // Same pending/refresh/`state.error` behaviour as `runMutation`, but resolves to the failure
  // message instead of swallowing it — the shared shape behind `addRemote`, `createBranch`,
  // `createWorktree`, and `createTag`'s inline-error results (see each of those in
  // `UseAppStateResult` for why: a create-form has one obvious trigger point to show its own
  // failure next to, per issue #30/UX-002).
  const runMutationWithMessage = useCallback<RunMutationWithMessage>(
    async (mutate) => {
      try {
        setState((prev) => ({ ...prev, pending: true, error: null }));
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return null;
      } catch (err) {
        const failure = failureError(err);
        setState((prev) => ({ ...prev, ...failure, pending: false }));
        return failure.error;
      }
    },
    [refresh, setState],
  );

  // Optimistic counterparts of the three variants above: apply `optimisticUpdate` to `AppState`
  // immediately (before awaiting `mutate`), so the UI reflects the predicted result with no
  // visible round-trip. On failure, restore the *exact* pre-call snapshot rather than computing
  // a per-mutation inverse — cheap (one object) and trivially correct: failure always means
  // "exactly what it looked like before this call touched anything." The `pending`-driven global
  // lock (still set here) already rules out two overlapping optimistic mutations racing each
  // other's snapshot/restore.
  const runOptimisticMutation = useCallback<RunOptimisticMutation>(
    async (optimisticUpdate, mutate) => {
      let snapshot: AppState | null = null;
      setState((prev) => {
        snapshot = prev;
        return { ...optimisticUpdate(prev), pending: true, error: null };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
      } catch (err) {
        setState(() => ({ ...(snapshot as AppState), ...failureError(err), pending: false }));
      }
    },
    [refresh, setState],
  );

  const runOptimisticMutationWithMessage = useCallback<RunOptimisticMutationWithMessage>(
    async (optimisticUpdate, mutate) => {
      let snapshot: AppState | null = null;
      setState((prev) => {
        snapshot = prev;
        return { ...optimisticUpdate(prev), pending: true, error: null };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return null;
      } catch (err) {
        const failure = failureError(err);
        setState(() => ({ ...(snapshot as AppState), ...failure, pending: false }));
        return failure.error;
      }
    },
    [refresh, setState],
  );

  const runOptimisticMutationWithOutcome = useCallback<RunOptimisticMutationWithOutcome>(
    async (optimisticUpdate, mutate) => {
      let snapshot: AppState | null = null;
      setState((prev) => {
        snapshot = prev;
        return { ...optimisticUpdate(prev), pending: true, error: null };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return true;
      } catch (err) {
        setState(() => ({ ...(snapshot as AppState), ...failureError(err), pending: false }));
        return false;
      }
    },
    [refresh, setState],
  );

  return {
    runMutation,
    runMutationWithOutcome,
    runMutationWithMessage,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    runOptimisticMutationWithOutcome,
  };
}
