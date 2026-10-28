import { useCallback } from "react";
import type { RepoClient, ResetMode } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunMutationWithOutcome } from "./useMutationRunner";
import { hookFailedMessage } from "../lib/hookFailure";

export interface HistoryActions {
  cherryPick(commitId: string): Promise<void>;
  revertCommit(commitId: string): Promise<void>;
  resetToCommit(commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(commitId: string): Promise<void>;
  // Resolves `true` once amended; see `StagingActions.commit`.
  amendCommit(message: string, noVerify: boolean): Promise<boolean>;
  setAmendMode(on: boolean): void;
}

// History-rewriting actions on a single commit. None are optimistic: each rewrites refs, and the
// conflict case (cherry-pick/revert) only becomes visible through the refresh that `runMutation`
// performs afterwards (MERGE_MSG ⇒ `mergeMessage`, CHERRY_PICK_HEAD ⇒ `headInfo.operation`).
export function useHistoryActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runMutationWithOutcome: RunMutationWithOutcome,
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
    async (message: string, noVerify: boolean) => {
      const ok = await runMutationWithOutcome(async () => {
        const outcome = await client.amendCommit(repoPath, message, noVerify);
        if (outcome.kind === "HookFailed") {
          setState((prev) => ({
            ...prev,
            hookFailure: { hook: outcome.hook, output: outcome.output, retry: { kind: "amend", message }, dialogOpen: true },
          }));
          throw new Error(hookFailedMessage(outcome.hook));
        }
        setState((prev) => ({ ...prev, amendMode: false, skipHooks: false, hookFailure: null }));
      });
      setState((prev) => ({ ...prev, hookRun: null }));
      return ok;
    },
    [client, runMutationWithOutcome, repoPath, setState],
  );
  const setAmendMode = useCallback(
    (on: boolean) => setState((prev) => ({ ...prev, amendMode: on })),
    [setState],
  );
  return { cherryPick, revertCommit, resetToCommit, checkoutCommit, amendCommit, setAmendMode };
}
