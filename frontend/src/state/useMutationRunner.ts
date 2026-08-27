import { useCallback } from "react";
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

export function credentialFailureMessage(error: unknown): string {
  // `Error`s are unwrapped rather than stringified: `String(new Error("x"))` is `"Error: x"`, and
  // that literal prefix is now user-visible — `RemotePanel` renders this message inline under the
  // Fetch URL field, not just in the generic error banner. Tauri's `invoke` rejects with a bare
  // string, so the `String` branch stays the common production path.
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("missing credential")) return "Save an HTTPS token for this remote before retrying.";
  if (message.includes("credential keychain failure")) return "The operating-system credential store is unavailable. Unlock it and try again.";
  if (message.includes("SSH agent failure")) return "Load a key into your SSH agent and try again.";
  return message;
}

// Builds the three `runMutation*` variants that every domain action hook composed inside
// `useAppState` uses to wrap its mutations. All three share the same pending/refresh/error
// bookkeeping against the top-level `AppState`; they differ only in what they resolve to (see
// each type above).
export function useMutationRunner(
  refresh: () => Promise<void>,
  setState: (updater: (prev: AppState) => AppState) => void,
): MutationRunner {
  const runMutation = useCallback<RunMutation>(
    async (mutate) => {
      try {
        setState((prev) => ({ ...prev, pending: true }));
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
      } catch (err) {
        setState((prev) => ({ ...prev, error: credentialFailureMessage(err), pending: false }));
      }
    },
    [refresh, setState],
  );

  const runMutationWithOutcome = useCallback<RunMutationWithOutcome>(
    async (mutate) => {
      try {
        setState((prev) => ({ ...prev, pending: true }));
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return true;
      } catch (err) {
        setState((prev) => ({ ...prev, error: credentialFailureMessage(err), pending: false }));
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
        setState((prev) => ({ ...prev, pending: true }));
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return null;
      } catch (err) {
        const message = credentialFailureMessage(err);
        setState((prev) => ({ ...prev, error: message, pending: false }));
        return message;
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
        return { ...optimisticUpdate(prev), pending: true };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
      } catch (err) {
        setState(() => ({ ...(snapshot as AppState), error: credentialFailureMessage(err), pending: false }));
      }
    },
    [refresh, setState],
  );

  const runOptimisticMutationWithMessage = useCallback<RunOptimisticMutationWithMessage>(
    async (optimisticUpdate, mutate) => {
      let snapshot: AppState | null = null;
      setState((prev) => {
        snapshot = prev;
        return { ...optimisticUpdate(prev), pending: true };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return null;
      } catch (err) {
        const message = credentialFailureMessage(err);
        setState(() => ({ ...(snapshot as AppState), error: message, pending: false }));
        return message;
      }
    },
    [refresh, setState],
  );

  const runOptimisticMutationWithOutcome = useCallback<RunOptimisticMutationWithOutcome>(
    async (optimisticUpdate, mutate) => {
      let snapshot: AppState | null = null;
      setState((prev) => {
        snapshot = prev;
        return { ...optimisticUpdate(prev), pending: true };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return true;
      } catch (err) {
        setState(() => ({ ...(snapshot as AppState), error: credentialFailureMessage(err), pending: false }));
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
