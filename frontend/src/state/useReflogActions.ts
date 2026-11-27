import { failedOperation } from "../lib/operationErrors";
import { useCallback, type RefObject } from "react";
import type { ReflogEntry, RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import { errorAfterOwnSuccess, type OwnError } from "./ownError";
import type { RunMutation } from "./useMutationRunner";

/** How many reflog entries the Reflog section reads at a time (#1161). */
export const REFLOG_PAGE_SIZE = 200;

/** What the reflog on screen was read for: the reference and how many entries were asked for. */
export interface ReflogLoaded {
  reference: string;
  limit: number;
}

/**
 * The newest `limit` entries of `reference`'s reflog, and whether older ones follow: one entry
 * more is read to know (#1161). `HEAD`'s reflog can hold hundreds of thousands of entries.
 */
export async function readReflogPage(
  client: RepoClient,
  repoPath: string,
  reference: string,
  limit: number,
): Promise<{ entries: ReflogEntry[]; hasMore: boolean }> {
  const entries = await client.getReflog(repoPath, reference, limit + 1);
  return { entries: entries.slice(0, limit), hasMore: entries.length > limit };
}

export interface ReflogActions {
  selectReflogReference(reference: string): Promise<void>;
  /** Reads the next `REFLOG_PAGE_SIZE` older entries of the selected reference (#1161). */
  loadMoreReflog(): Promise<void>;
  restoreReflogEntry(reference: string, newId: string): Promise<void>;
}

// `selectedReflogReference`/`reflogRequestGeneration` are owned by `useAppState` (not this hook)
// because `refresh()` also reads/writes `selectedReflogReference.current` to reconcile the
// selection against a freshly-fetched `reflogRefs` list — see `useAppState.ts`'s `refresh`. Both
// that reconciliation and these actions need to observe the same "what's currently selected,
// and is this the latest request" state, so the refs are passed in rather than duplicated. So
// are the number of entries shown (`reflogLimit`) and what was read (`reflogLoaded`), which a
// refresh reads again only when the references moved (#1161).
export function useReflogActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
  selectedReflogReference: RefObject<string | null>,
  reflogRequestGeneration: RefObject<number>,
  reflogLimit: RefObject<number>,
  reflogLoaded: RefObject<ReflogLoaded | null>,
  // The error a failed reflog read set: a later successful read clears that one only, never
  // another action's (#177).
  ownError: OwnError,
): ReflogActions {
  // Reads `limit` entries of `reference`, applied unless a later read or selection superseded it.
  const read = useCallback(
    async (reference: string, limit: number) => {
      const requestGeneration = ++reflogRequestGeneration.current;
      const current = () =>
        requestGeneration === reflogRequestGeneration.current && selectedReflogReference.current === reference;
      try {
        const page = await readReflogPage(client, repoPath, reference, limit);
        if (!current()) return;
        reflogLoaded.current = { reference, limit };
        const own = ownError.take();
        setState((prev) => ({
          ...prev,
          selectedReflogReference: reference,
          reflog: page.entries,
          reflogHasMore: page.hasMore,
          error: errorAfterOwnSuccess(prev.error, own),
        }));
      } catch (err) {
        if (current()) {
          ownError.set(String(err));
          setState((prev) => ({ ...prev, error: String(err), errorSource: { error: String(err), source: "read" } }));
        }
      }
    },
    [client, repoPath, setState, selectedReflogReference, reflogRequestGeneration, reflogLoaded, ownError],
  );

  const selectReflogReference = useCallback(
    (reference: string) => {
      selectedReflogReference.current = reference;
      // Another reference starts again at its newest entries.
      reflogLimit.current = REFLOG_PAGE_SIZE;
      return read(reference, REFLOG_PAGE_SIZE);
    },
    [read, selectedReflogReference, reflogLimit],
  );

  const loadMoreReflog = useCallback(async () => {
    const reference = selectedReflogReference.current;
    if (reference === null) return;
    reflogLimit.current += REFLOG_PAGE_SIZE;
    await read(reference, reflogLimit.current);
  }, [read, selectedReflogReference, reflogLimit]);

  const restoreReflogEntry = useCallback(
    (reference: string, newId: string) => {
      selectedReflogReference.current = reference;
      reflogRequestGeneration.current += 1;
      setState((prev) => ({ ...prev, selectedReflogReference: reference }));
      return runMutation(
        () => client.restoreReflogEntry(repoPath, reference, newId),
        failedOperation.restoreReflog(reference, newId),
      );
    },
    [client, runMutation, repoPath, setState, selectedReflogReference, reflogRequestGeneration],
  );

  return { selectReflogReference, loadMoreReflog, restoreReflogEntry };
}
