import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
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
  runOptimisticMutation: RunOptimisticMutation,
): StashActions {
  const saveStash = useCallback(
    () =>
      runMutation(() =>
        client.saveStash(repoPath, { message: null, keepIndex: false, includeUntracked: true, paths: [] }),
      ),
    [client, runMutation, repoPath],
  );
  const applyStash = useCallback(
    (index: number) =>
      runMutation(async () => {
        await client.applyStash(repoPath, index, false);
      }),
    [client, runMutation, repoPath],
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

  return { saveStash, applyStash, dropStash };
}
