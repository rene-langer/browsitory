import { useCallback, type RefObject } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation } from "./useMutationRunner";

export interface ReflogActions {
  selectReflogReference(reference: string): Promise<void>;
  restoreReflogEntry(reference: string, newId: string): Promise<void>;
}

// `selectedReflogReference`/`reflogRequestGeneration` are owned by `useAppState` (not this hook)
// because `refresh()` also reads/writes `selectedReflogReference.current` to reconcile the
// selection against a freshly-fetched `reflogRefs` list — see `useAppState.ts`'s `refresh`. Both
// that reconciliation and these two actions need to observe the same "what's currently selected,
// and is this the latest request" state, so the refs are passed in rather than duplicated.
export function useReflogActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
  selectedReflogReference: RefObject<string | null>,
  reflogRequestGeneration: RefObject<number>,
): ReflogActions {
  const selectReflogReference = useCallback(
    async (reference: string) => {
      const requestGeneration = ++reflogRequestGeneration.current;
      try {
        selectedReflogReference.current = reference;
        const reflog = await client.getReflog(repoPath, reference);
        if (
          requestGeneration !== reflogRequestGeneration.current ||
          selectedReflogReference.current !== reference
        ) {
          return;
        }
        setState((prev) => ({
          ...prev,
          selectedReflogReference: reference,
          reflog,
          error: null,
        }));
      } catch (err) {
        if (
          requestGeneration === reflogRequestGeneration.current &&
          selectedReflogReference.current === reference
        ) {
          setState((prev) => ({ ...prev, error: String(err) }));
        }
      }
    },
    [client, repoPath, setState, selectedReflogReference, reflogRequestGeneration],
  );

  const restoreReflogEntry = useCallback(
    (reference: string, newId: string) => {
      selectedReflogReference.current = reference;
      reflogRequestGeneration.current += 1;
      setState((prev) => ({ ...prev, selectedReflogReference: reference }));
      return runMutation(() => client.restoreReflogEntry(repoPath, reference, newId));
    },
    [client, runMutation, repoPath, setState, selectedReflogReference, reflogRequestGeneration],
  );

  return { selectReflogReference, restoreReflogEntry };
}
