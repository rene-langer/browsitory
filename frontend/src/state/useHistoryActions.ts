import { useCallback } from "react";
import type { RepoClient, ResetMode } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation } from "./useMutationRunner";

export interface HistoryActions {
  cherryPick(commitId: string): Promise<void>;
  revertCommit(commitId: string): Promise<void>;
  resetToCommit(commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(commitId: string): Promise<void>;
  amendCommit(message: string): Promise<void>;
  setAmendMode(on: boolean): void;
}

// History-rewriting actions on a single commit. None are optimistic: each rewrites refs, and the
// conflict case (cherry-pick/revert) only becomes visible through the refresh that `runMutation`
// performs afterwards (MERGE_MSG ⇒ `mergeMessage`, CHERRY_PICK_HEAD ⇒ `headInfo.operation`).
export function useHistoryActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): HistoryActions {
  const cherryPick = useCallback(
    (commitId: string) => runMutation(async () => void (await client.cherryPick(repoPath, commitId))),
    [client, runMutation, repoPath],
  );
  const revertCommit = useCallback(
    (commitId: string) => runMutation(async () => void (await client.revertCommit(repoPath, commitId))),
    [client, runMutation, repoPath],
  );
  const resetToCommit = useCallback(
    (commitId: string, mode: ResetMode) => runMutation(() => client.resetToCommit(repoPath, commitId, mode)),
    [client, runMutation, repoPath],
  );
  const checkoutCommit = useCallback(
    (commitId: string) => runMutation(() => client.checkoutCommit(repoPath, commitId)),
    [client, runMutation, repoPath],
  );
  const amendCommit = useCallback(
    (message: string) =>
      runMutation(async () => {
        await client.amendCommit(repoPath, message);
        setState((prev) => ({ ...prev, amendMode: false }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const setAmendMode = useCallback(
    (on: boolean) => setState((prev) => ({ ...prev, amendMode: on })),
    [setState],
  );
  return { cherryPick, revertCommit, resetToCommit, checkoutCommit, amendCommit, setAmendMode };
}
