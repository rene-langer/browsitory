import { useCallback, useEffect, useRef } from "react";
import {
  errorKind,
  type AmendOptions,
  type ConfirmedAbort,
  type HeadInfo,
  type PickOutcome,
  type RepoClient,
  type ResetMode,
  type SequenceAbort,
  type SequenceKind,
  type SequenceOutcome,
} from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunMutationWithOutcome } from "./useMutationRunner";
import { hookFailedMessage, type HookRetry } from "../lib/hookFailure";
import { failedOperation, FailureWithNextStep, hookNextStep, type FailedOperation } from "../lib/operationErrors";
import { failureError } from "./useMutationRunner";

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
  /** `confirmed`: for a terminal sequence, what the user confirmed (see `RepoClient`). */
  abortSequence(confirmed?: ConfirmedAbort): Promise<SequenceAbort | null>;
  resetToCommit(commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(commitId: string): Promise<void>;
  // Resolves `true` once amended; see `StagingActions.commit`.
  amendCommit(message: string, noVerify: boolean, options?: AmendOptions): Promise<boolean>;
  /**
   * The Reword dialog on HEAD (#981): replaces its message only, staged changes stay staged.
   * Not an amend: amend mode and Skip hooks stay as they are. Resolves `true` once reworded.
   */
  rewordHead(commitId: string, message: string, noVerify?: boolean): Promise<boolean>;
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
  /** HEAD as last read, to tell whether a failed pick changed anything. */
  headId: string | null,
  /** The stored and the terminal sequence as last read: a failed Abort names its kind (#1205). */
  sequences: Pick<HeadInfo, "sequence" | "terminalSequence">,
): HistoryActions {
  const headBefore = useRef(headId);
  useEffect(() => {
    headBefore.current = headId;
  }, [headId]);
  const sequencesNow = useRef(sequences);
  useEffect(() => {
    sequencesNow.current = sequences;
  }, [sequences]);
  // Runs `call` as a mutation and hands back its result; `null` when it threw (runMutation has
  // put the error into `state.error`, after `operation` when given).
  const withOutcome = useCallback(
    async <T,>(call: () => Promise<T>, operation?: FailedOperation): Promise<T | null> => {
      let result: T | null = null;
      await runMutation(async () => {
        result = await call();
      }, operation);
      return result;
    },
    [runMutation],
  );
  // Picks and Continue may change the repository and still end in a failure: steps committed
  // before a failing hook, a step whose commit stands after Cancel, a pick left staged. So the
  // refresh always runs: the call's failure is held back from `runMutation` (which skips the
  // refresh when its mutation throws) and only shown afterwards (#183).
  //
  // A failing hook (`HookFailed`: a pick's `prepare-commit-msg`, or Continue's commit hooks)
  // opens the hook dialog and resolves `null` with the error banner, like a failed commit. The
  // step stays staged and paused: the operation strip and the commit box finish it. `retry`:
  // a single pick has nothing to retry (`prepare-commit-msg` can't be skipped); a sequence
  // offers Continue.
  const runRepoChanging = useCallback(
    async <T extends PickOutcome | SequenceOutcome>(
      call: () => Promise<T>,
      retry: HookRetry,
      /** The pick a failure names ("Cherry-pick of 4c5beaf didn't start", #997, #1032). */
      pick?: { kind: SequenceKind; commitIds: readonly string[] },
    ): Promise<T | null> => {
      let outcome: T | null = null;
      let failure: unknown = undefined;
      let failed = false;
      const before = headBefore.current;
      await runMutation(async () => {
        try {
          outcome = await call();
        } catch (err) {
          failed = true;
          failure = err;
        }
      });
      setState((prev) => ({ ...prev, hookRun: null }));
      if (failed) {
        setState((prev) => {
          // The refresh has run: a pick that failed after some steps (`SequenceRun::stopped`)
          // left a paused sequence or step, or moved HEAD. It did start, so it couldn't finish.
          const info = prev.headInfo;
          const started =
            info.sequence !== null ||
            info.operation === "CherryPick" ||
            info.operation === "Revert" ||
            info.headId !== before;
          const operation =
            pick === undefined
              ? undefined
              : started
                ? failedOperation.pickStopped(pick.kind, pick.commitIds)
                : failedOperation.pick(pick.kind, pick.commitIds);
          return { ...prev, ...failureError(failure, operation) };
        });
        return null;
      }
      const settled = outcome as T | null;
      if (settled === null) return null;
      if (settled.kind === "HookFailed") {
        setState((prev) => ({
          ...prev,
          error: hookFailedMessage(settled.hook),
          hookFailure: { hook: settled.hook, output: settled.output, retry, dialogOpen: true },
        }));
        return null;
      }
      return settled;
    },
    [runMutation, setState],
  );
  const cherryPick = useCallback(
    (commitId: string, mainline?: number | null) =>
      runRepoChanging(
        () => client.cherryPick(repoPath, commitId, mainline ?? null),
        { kind: "pick" },
        { kind: "CherryPick", commitIds: [commitId] },
      ),
    [client, runRepoChanging, repoPath],
  );
  const revertCommit = useCallback(
    (commitId: string, mainline?: number | null) =>
      runRepoChanging(
        () => client.revertCommit(repoPath, commitId, mainline ?? null),
        { kind: "pick" },
        { kind: "Revert", commitIds: [commitId] },
      ),
    [client, runRepoChanging, repoPath],
  );
  const cherryPickMany = useCallback(
    (commitIds: string[]) =>
      runRepoChanging(
        () => client.cherryPickMany(repoPath, commitIds),
        { kind: "continueSequence" },
        { kind: "CherryPick", commitIds: commitIds },
      ),
    [client, runRepoChanging, repoPath],
  );
  const revertMany = useCallback(
    (commitIds: string[]) =>
      runRepoChanging(
        () => client.revertMany(repoPath, commitIds),
        { kind: "continueSequence" },
        { kind: "Revert", commitIds: commitIds },
      ),
    [client, runRepoChanging, repoPath],
  );
  const continueSequence = useCallback(
    async (noVerify = false) => {
      const outcome = await runRepoChanging(() => client.continueSequence(repoPath, noVerify), {
        kind: "continueSequence",
      });
      if (outcome !== null) setState((prev) => ({ ...prev, hookFailure: null }));
      return outcome;
    },
    [client, runRepoChanging, repoPath, setState],
  );
  const abortSequence = useCallback(
    (confirmed?: ConfirmedAbort) => {
      // `confirmed` comes with a terminal sequence's Abort; the stored one is aborted otherwise.
      const { sequence, terminalSequence } = sequencesNow.current;
      const kind = (confirmed === undefined ? sequence?.kind : terminalSequence?.kind) ?? null;
      return withOutcome(
        () => (confirmed === undefined ? client.abortSequence(repoPath) : client.abortSequence(repoPath, confirmed)),
        failedOperation.abortSequence(kind),
      );
    },
    [client, withOutcome, repoPath],
  );
  const resetToCommit = useCallback(
    (commitId: string, mode: ResetMode) => runMutation(() => client.resetToCommit(repoPath, commitId, mode), failedOperation.reset(commitId)),
    [client, runMutation, repoPath],
  );
  const checkoutCommit = useCallback(
    (commitId: string) => runMutation(() => client.checkoutCommit(repoPath, commitId), failedOperation.switchDetached(commitId)),
    [client, runMutation, repoPath],
  );
  const amendCommit = useCallback(
    async (message: string, noVerify: boolean, options: AmendOptions = NO_AMEND_OPTIONS) => {
      const ok = await runMutationWithOutcome(async () => {
        const outcome = await client.amendCommit(repoPath, message, noVerify, options).catch((error: unknown) => {
          // No `user.name`/`user.email`: the identity dialog offers to set them (#689).
          if (errorKind(error) === "MissingIdentity") setState((prev) => ({ ...prev, identityRequired: "amend" }));
          throw error;
        });
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
          throw new FailureWithNextStep(hookFailedMessage(outcome.hook), hookNextStep("amend again"));
        }
        setState((prev) => ({ ...prev, amendMode: false, skipHooks: false, hookFailure: null }));
      }, failedOperation.amend());
      setState((prev) => ({ ...prev, hookRun: null }));
      return ok;
    },
    [client, runMutationWithOutcome, repoPath, setState],
  );
  const rewordHead = useCallback(
    async (commitId: string, message: string, noVerify = false) => {
      const ok = await runMutationWithOutcome(async () => {
        const options: AmendOptions = { ...NO_AMEND_OPTIONS, messageOnly: true };
        const outcome = await client.amendCommit(repoPath, message, noVerify, options).catch((error: unknown) => {
          // No `user.name`/`user.email`: the identity dialog offers to set them (#689).
          if (errorKind(error) === "MissingIdentity") setState((prev) => ({ ...prev, identityRequired: "amend" }));
          throw error;
        });
        if (outcome.kind === "HookFailed") {
          setState((prev) => ({
            ...prev,
            hookFailure: {
              hook: outcome.hook,
              output: outcome.output,
              retry: { kind: "reword", commitId, message },
              dialogOpen: true,
            },
          }));
          throw new FailureWithNextStep(hookFailedMessage(outcome.hook), hookNextStep("reword it again"));
        }
        setState((prev) => ({ ...prev, hookFailure: null }));
      }, failedOperation.historyEdit("Reword", commitId));
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
    rewordHead,
    setAmendMode,
  };
}
