import { useCallback } from "react";
import type { RepoClient, StashEntry, StashOptions } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";

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
  const applyStash = useCallback(
    (index: number) =>
      runMutation(async () => {
        setState((prev) => ({ ...prev, stashOutcome: null }));
        const outcome = await client.applyStash(repoPath, index, restoreIndex);
        setState((prev) => ({ ...prev, stashOutcome: { action: "apply", outcome } }));
      }),
    [client, runMutation, repoPath, setState, restoreIndex],
  );
  const popStash = useCallback(
    (index: number) =>
      runMutation(async () => {
        setState((prev) => ({ ...prev, stashOutcome: null }));
        const poppedCommitId = stashes[index]?.commitId;
        const outcome = await client.popStash(repoPath, index, restoreIndex);
        setState((prev) => {
          // Only a clean pop drops the stash; a conflicted one keeps it (and its selection).
          const popsSelectedStash =
            outcome.kind === "Applied" &&
            poppedCommitId !== undefined &&
            typeof prev.selectedRow === "object" &&
            prev.selectedRow.commitId === poppedCommitId;
          return {
            ...prev,
            selectedRow: popsSelectedStash ? "uncommitted" : prev.selectedRow,
            stashOutcome: { action: "pop", outcome },
          };
        });
      }),
    [client, runMutation, repoPath, setState, stashes, restoreIndex],
  );
  const dropStash = useCallback(
    (index: number) =>
      runOptimisticMutation(
        (prev) => {
          const droppedCommitId = prev.stashes[index]?.commitId;
          const dropsSelectedStash =
            droppedCommitId !== undefined &&
            typeof prev.selectedRow === "object" &&
            prev.selectedRow.commitId === droppedCommitId;
          return {
            ...prev,
            stashes: prev.stashes.filter((_, stashIndex) => stashIndex !== index),
            selectedRow: dropsSelectedStash ? "uncommitted" : prev.selectedRow,
          };
        },
        () => client.dropStash(repoPath, index),
      ),
    [client, runOptimisticMutation, repoPath],
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
