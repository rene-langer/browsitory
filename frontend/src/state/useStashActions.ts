import { useCallback } from "react";
import type { RepoClient, StashEntry, StashOptions } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";
import { withoutDialog } from "./workspaceDialog";

export const STASH_LIST_CHANGED = "The stash list changed; refresh and try again.";

const STASH_EVERYTHING: StashOptions = { message: null, keepIndex: false, includeUntracked: true, paths: [] };

export interface StashActions {
  /**
   * Stashes with `options`; without them, everything (tracked and untracked, no message). Closes
   * the stash dialog (`dialog` kind "stash") if open.
   */
  saveStash(options?: StashOptions): Promise<void>;
  /**
   * Apply/Pop record `state.stashOutcome` for the toast. `restoreIndex` (default on) restores the
   * stash's staged changes as staged; "Apply as unstaged" passes `false` (#657).
   */
  applyStash(index: number, restoreIndex?: boolean): Promise<void>;
  popStash(index: number, restoreIndex?: boolean): Promise<void>;
  dropStash(index: number): Promise<void>;
}

export function useStashActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
  stashes: StashEntry[],
): StashActions {
  const saveStash = useCallback(
    (options: StashOptions = STASH_EVERYTHING) =>
      runMutation(async () => {
        setState((prev) => ({ ...prev, dialog: withoutDialog(prev.dialog, "stash") }));
        await client.saveStash(repoPath, options);
      }),
    [client, runMutation, repoPath, setState],
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
    async (index: number, restoreIndex = true) => {
      const commitId = listedCommitId(index);
      if (commitId === null) return;
      await runMutation(async () => {
        setState((prev) => ({ ...prev, stashOutcome: null }));
        const outcome = await client.applyStash(repoPath, index, commitId, restoreIndex);
        setState((prev) => ({ ...prev, stashOutcome: { action: "apply", outcome } }));
      });
    },
    [client, runMutation, repoPath, setState, listedCommitId],
  );
  const popStash = useCallback(
    async (index: number, restoreIndex = true) => {
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
    [client, runMutation, repoPath, setState, listedCommitId],
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
    applyStash,
    popStash,
    dropStash,
  };
}
