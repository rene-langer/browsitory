import { useCallback } from "react";
import type { AmendOptions, PickOutcome, RepoClient, ResetMode, SequenceAbort, SequenceOutcome } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunMutationWithOutcome } from "./useMutationRunner";
import { hookFailedMessage, type HookRetry } from "../lib/hookFailure";

const NO_AMEND_OPTIONS: AmendOptions = { author: null, trailers: [] };

export interface HistoryActions {
  /**
   * Each resolves to the backend's outcome, or `null` when it failed (the error is in
   * `state.error`). `mainline` (1-based parent) is required for merge commits.
   */
  cherryPick(commitId: string, mainline?: number | null): Promise<PickOutcome | null>;
  revertCommit(commitId: string, mainline?: number | null): Promise<PickOutcome | null>;
  /** Ids oldest-first (graph order reversed); a conflict pauses the sequence. */
  cherryPickMany(commitIdsOldestFirst: string[]): Promise<SequenceOutcome | null>;
  revertMany(commitIdsOldestFirst: string[]): Promise<SequenceOutcome | null>;
  /**
   * The step it commits runs the commit hooks unless `noVerify`. A failing hook opens the hook
   * dialog ("Continue without hooks") and resolves `null`, like a failed commit.
   */
  continueSequence(noVerify?: boolean): Promise<SequenceOutcome | null>;
  abortSequence(): Promise<SequenceAbort | null>;
  resetToCommit(commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(commitId: string): Promise<void>;
  // Resolves `true` once amended; see `StagingActions.commit`.
  amendCommit(message: string, noVerify: boolean, options?: AmendOptions): Promise<boolean>;
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
  // Runs `call` as a mutation and hands back its result; `null` when it threw (runMutation has
  // put the error into `state.error`).
  const withOutcome = useCallback(
    async <T,>(call: () => Promise<T>): Promise<T | null> => {
      let result: T | null = null;
      await runMutation(async () => {
        result = await call();
      });
      return result;
    },
    [runMutation],
  );
  // A pick's own hooks (#183): a failing `prepare-commit-msg` opens the hook dialog (output only:
  // that hook can't be skipped) and resolves `null` with the error banner, like a failed commit.
  // The pick stays staged and in progress: the operation strip and the commit box finish it.
  // `retry`: a single pick has nothing to retry; a sequence pauses for Continue.
  const runPick = useCallback(
    async <T extends PickOutcome | SequenceOutcome>(call: () => Promise<T>, retry: HookRetry): Promise<T | null> => {
      const outcome = await withOutcome(async () => {
        const outcome = await call();
        if (outcome.kind === "HookFailed") {
          setState((prev) => ({
            ...prev,
            hookFailure: { hook: outcome.hook, output: outcome.output, retry, dialogOpen: true },
          }));
          throw new Error(hookFailedMessage(outcome.hook));
        }
        return outcome;
      });
      setState((prev) => ({ ...prev, hookRun: null }));
      return outcome;
    },
    [withOutcome, setState],
  );
  const cherryPick = useCallback(
    (commitId: string, mainline?: number | null) =>
      runPick(() => client.cherryPick(repoPath, commitId, mainline ?? null), { kind: "pick" }),
    [client, runPick, repoPath],
  );
  const revertCommit = useCallback(
    (commitId: string, mainline?: number | null) =>
      runPick(() => client.revertCommit(repoPath, commitId, mainline ?? null), { kind: "pick" }),
    [client, runPick, repoPath],
  );
  const cherryPickMany = useCallback(
    (commitIds: string[]) =>
      runPick(() => client.cherryPickMany(repoPath, commitIds), { kind: "continueSequence" }),
    [client, runPick, repoPath],
  );
  const revertMany = useCallback(
    (commitIds: string[]) => runPick(() => client.revertMany(repoPath, commitIds), { kind: "continueSequence" }),
    [client, runPick, repoPath],
  );
  const continueSequence = useCallback(
    async (noVerify = false) => {
      const outcome = await withOutcome(async () => {
        const outcome = await client.continueSequence(repoPath, noVerify);
        if (outcome.kind === "HookFailed") {
          setState((prev) => ({
            ...prev,
            hookFailure: {
              hook: outcome.hook,
              output: outcome.output,
              retry: { kind: "continueSequence" },
              dialogOpen: true,
            },
          }));
          throw new Error(hookFailedMessage(outcome.hook));
        }
        setState((prev) => ({ ...prev, hookFailure: null }));
        return outcome;
      });
      setState((prev) => ({ ...prev, hookRun: null }));
      return outcome;
    },
    [client, withOutcome, repoPath, setState],
  );
  const abortSequence = useCallback(
    () => withOutcome(() => client.abortSequence(repoPath)),
    [client, withOutcome, repoPath],
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
    async (message: string, noVerify: boolean, options: AmendOptions = NO_AMEND_OPTIONS) => {
      const ok = await runMutationWithOutcome(async () => {
        const outcome = await client.amendCommit(repoPath, message, noVerify, options);
        if (outcome.kind === "HookFailed") {
          setState((prev) => ({
            ...prev,
            hookFailure: {
              hook: outcome.hook,
              output: outcome.output,
              retry: { kind: "amend", message, options },
              dialogOpen: true,
            },
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
  return {
    cherryPick,
    revertCommit,
    cherryPickMany,
    revertMany,
    continueSequence,
    abortSequence,
    resetToCommit,
    checkoutCommit,
    amendCommit,
    setAmendMode,
  };
}
