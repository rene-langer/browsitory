import { useCallback } from "react";
import type { RepoClient, StashEntry, StashOptions } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";

export const STASH_LIST_CHANGED = "The stash list changed; refresh and try again.";

const STASH_EVERYTHING: StashOptions = { message: null, keepIndex: false, includeUntracked: true, paths: [] };

export interface StashActions {
  /** Stashes with `options`; without them, everything (tracked and untracked, no message). */
  saveStash(options?: StashOptions): Promise<void>;
  /** Opens the stash options dialog; `paths` non-empty for "Stash file…". */
  openStashDialog(paths: string[]): void;
  closeStashDialog(): void;
  /** Apply/Pop use `state.stashRestoreIndex` and record `state.stashOutcome` for the toast. */
  applyStash(index: number): Promise<void>;
  popStash(index: number): Promise<void>;
  setStashRestoreIndex(on: boolean): void;
  dropStash(index: number): Promise<void>;
}

export function useStashActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
  stashes: StashEntry[],
  restoreIndex: boolean,
): StashActions {
  const saveStash = useCallback(
    (options: StashOptions = STASH_EVERYTHING) =>
      runMutation(async () => {
        setState((prev) => ({ ...prev, stashDialog: null }));
        await client.saveStash(repoPath, options);
      }),
    [client, runMutation, repoPath, setState],
  );
  const openStashDialog = useCallback(
    (paths: string[]) => setState((prev) => ({ ...prev, stashDialog: { paths } })),
    [setState],
  );
  const closeStashDialog = useCallback(
    () => setState((prev) => ({ ...prev, stashDialog: null })),
    [setState],
  );
  const setStashRestoreIndex = useCallback(
    (on: boolean) => setState((prev) => ({ ...prev, stashRestoreIndex: on })),
    [setState],
  );
  // Apply/Pop/Drop send the listed stash's commit id along with its index; the backend refuses
  // when `stash@{index}` is now another stash. An index this list doesn't know is refused here.
  const listedCommitId = useCallback(
    (index: number): string | null => {
      const commitId = stashes[index]?.commitId;
      if (commitId !== undefined) return commitId;
      setState((prev) => ({ ...prev, error: STASH_LIST_CHANGED }));
      return null;
    },
    [stashes, setState],
  );
  const applyStash = useCallback(
    async (index: number) => {
      const commitId = listedCommitId(index);
      if (commitId === null) return;
      await runMutation(async () => {
        setState((prev) => ({ ...prev, stashOutcome: null }));
        const outcome = await client.applyStash(repoPath, index, commitId, restoreIndex);
        setState((prev) => ({ ...prev, stashOutcome: { action: "apply", outcome } }));
      });
    },
    [client, runMutation, repoPath, setState, restoreIndex, listedCommitId],
  );
  const popStash = useCallback(
    async (index: number) => {
      const poppedCommitId = listedCommitId(index);
      if (poppedCommitId === null) return;
      await runMutation(async () => {
        setState((prev) => ({ ...prev, stashOutcome: null }));
        const outcome = await client.popStash(repoPath, index, poppedCommitId, restoreIndex);
        setState((prev) => {
          // Only a clean pop drops the stash; a conflicted one keeps it (and its selection).
          const popsSelectedStash =
            outcome.kind === "Applied" &&
            typeof prev.selectedRow === "object" &&
            prev.selectedRow.commitId === poppedCommitId;
          return {
            ...prev,
            selectedRow: popsSelectedStash ? "uncommitted" : prev.selectedRow,
            stashOutcome: { action: "pop", outcome },
          };
        });
      });
    },
    [client, runMutation, repoPath, setState, restoreIndex, listedCommitId],
  );
  const dropStash = useCallback(
    async (index: number) => {
      const droppedCommitId = listedCommitId(index);
      if (droppedCommitId === null) return;
      await runOptimisticMutation(
        (prev) => {
          const dropsSelectedStash =
            typeof prev.selectedRow === "object" &&
            prev.selectedRow.commitId === droppedCommitId;
          return {
            ...prev,
            stashes: prev.stashes.filter((_, stashIndex) => stashIndex !== index),
            selectedRow: dropsSelectedStash ? "uncommitted" : prev.selectedRow,
          };
        },
        () => client.dropStash(repoPath, index, droppedCommitId),
      );
    },
    [client, runOptimisticMutation, repoPath, listedCommitId],
  );

  return {
    saveStash,
    openStashDialog,
    closeStashDialog,
    applyStash,
    popStash,
    setStashRestoreIndex,
    dropStash,
  };
}
