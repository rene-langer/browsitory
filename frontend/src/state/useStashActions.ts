import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";

export interface StashActions {
  saveStash(): Promise<void>;
  applyStash(index: number): Promise<void>;
  dropStash(index: number): Promise<void>;
}

export function useStashActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  _runOptimisticMutation: RunOptimisticMutation,
  state: AppState,
  setState: (updater: (prev: AppState) => AppState) => void,
): StashActions {
  const saveStash = useCallback(
    () => runMutation(() => client.saveStash(repoPath)),
    [client, runMutation, repoPath],
  );
  const applyStash = useCallback(
    (index: number) => runMutation(() => client.applyStash(repoPath, index)),
    [client, runMutation, repoPath],
  );
  const dropStash = useCallback(
    (index: number) =>
      runMutation(async () => {
        // Read the about-to-be-dropped stash's commitId before calling the client: if it's
        // the one currently selected, `DiffPane` would otherwise keep showing a diff for a
        // commit that's about to become unreachable until GC.
        const droppedCommitId = state.stashes[index]?.commitId;
        const dropsSelectedStash =
          droppedCommitId !== undefined &&
          typeof state.selectedRow === "object" &&
          state.selectedRow.commitId === droppedCommitId;
        await client.dropStash(repoPath, index);
        if (dropsSelectedStash) {
          setState((prev) => ({ ...prev, selectedRow: "uncommitted" }));
        }
      }),
    [client, runMutation, state, repoPath, setState],
  );

  return { saveStash, applyStash, dropStash };
}
