import { useCallback } from "react";
import type {
  RebaseAbort,
  ConflictSide,
  FileConflictChoice,
  MarkResolvedOutcome,
  MergeOptions,
  MergeOutcome,
  PullStrategy,
  RebasePlanEntry,
  RebaseStepResult,
  RepoClient,
} from "../ipc/RepoClient";
import {
  buildHistoryEditPlan,
  HISTORY_CHANGED,
  historyEditDoneMessage,
  squashMessage,
  type HistoryEdit,
  type HistoryEditRequest,
} from "../lib/historyEdits";
import { logFailure } from "../lib/logger";
import { shortRefName } from "../lib/shortRefName";
import type { PresetAction } from "../lib/rebaseSelection";
import type { AppState } from "./useAppState";
import type { RunMutation } from "./useMutationRunner";
import { withoutDialog } from "./workspaceDialog";

export interface MergeRebaseActions {
  /** The dialog's preselection: remembered per repo, else from `merge.ff`. */
  getMergeOptions(): Promise<MergeOptions>;
  /**
   * Remembers `options` for this repo, closes the merge dialog (`dialog` kind "merge") and merges
   * `branchName` with them, recording the outcome in `state.mergeOutcome` for the toast.
   */
  mergeBranch(branchName: string, options: MergeOptions): Promise<void>;
  /** "Pull has diverged → Merge": merges the upstream ref, allowing fast-forward, no dialog. */
  mergePull(upstreamRef: string): Promise<void>;
  /**
   * "Pull has diverged → Rebase" (#652): a plain rebase of the branch's own commits onto the
   * upstream ref, every commit picked, without the planner. A conflict pauses it as any rebase.
   */
  rebasePull(upstreamRef: string): Promise<void>;
  /** The remembered pull strategy, else the git config's, else `null` (ask). */
  getPullStrategy(): Promise<PullStrategy | null>;
  /** Remembers `strategy` for this repository; a failure is logged, never thrown. */
  setPullStrategy(strategy: PullStrategy): Promise<void>;
  resolveConflict(path: string, resolvedContent: string): Promise<void>;
  resolveAddDeleteConflict(path: string, choice: FileConflictChoice): Promise<void>;
  /** Takes one side's whole file for a conflicted path (any conflict shape). */
  resolveWholeFile(path: string, side: ConflictSide): Promise<void>;
  /**
   * Stages a conflicted file as resolved. Without `force` a file that still has conflict markers
   * is left alone ("MarkersRemain"). `null` means it failed; the error is in `state.error`.
   */
  markResolved(path: string, force: boolean): Promise<MarkResolvedOutcome | null>;
  abortMerge(): Promise<void>;
  openRebasePlanner(commitId: string): void;
  openRebaseSelection(onto: string, preset: ReadonlyMap<string, PresetAction>): void;
  closeRebasePlanner(): void;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<void>;
  /**
   * Runs a one-click history edit of `commitId` (#442) as a one-step rebase from `onto` (see
   * `historyEditChecks`): the plan is `commitsSince(onto)` with that one change. Resolves the
   * step result (`Done`, or a pause), or `null` when it failed (the error is in `state.error`).
   */
  editHistory(commitId: string, onto: string, request: HistoryEditRequest): Promise<RebaseStepResult | null>;
  rebaseContinue(): Promise<void>;
  /** The abort's outcome, or `null` when it failed (the error is in `state.error`). */
  abortRebase(): Promise<RebaseAbort | null>;
}

export function useMergeRebaseActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): MergeRebaseActions {
  const getMergeOptions = useCallback(() => client.getMergeOptions(repoPath), [client, repoPath]);
  const runMerge = useCallback(
    (branchName: string, options: MergeOptions): Promise<void> =>
      runMutation(async () => {
        setState((prev) => ({ ...prev, dialog: withoutDialog(prev.dialog, "merge"), mergeOutcome: null }));
        const outcome: MergeOutcome = await client.mergeBranch(repoPath, branchName, options);
        setState((prev) => ({ ...prev, mergeOutcome: { branchName, outcome } }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const mergeBranch = useCallback(
    async (branchName: string, options: MergeOptions): Promise<void> => {
      // Remembered before merging, so a merge that fails still keeps the choice. Failing to
      // remember must not block the merge itself.
      try {
        await client.setMergeOptions(repoPath, options);
      } catch (error) {
        logFailure("Remembering merge options failed", error);
      }
      await runMerge(branchName, options);
    },
    [client, repoPath, runMerge],
  );
  const mergePull = useCallback(
    (upstreamRef: string) => runMerge(upstreamRef, { fastForward: "Allow", squash: false }),
    [runMerge],
  );
  const rebasePull = useCallback(
    (upstreamRef: string): Promise<void> =>
      runMutation(async () => {
        const commits = await client.commitsSince(repoPath, upstreamRef);
        const plan: RebasePlanEntry[] = commits.map((commit) => ({
          commitId: commit.id,
          action: { kind: "Pick" },
          combinedMessage: null,
        }));
        setState((prev) => ({ ...prev, historyEditOutcome: null }));
        const step = await client.startRebase(repoPath, upstreamRef, plan);
        // A pause shows in the rebase progress panel instead.
        if (step.kind === "Done") {
          const message = `Rebased onto ${shortRefName(upstreamRef)}`;
          setState((prev) => ({ ...prev, historyEditOutcome: { message } }));
        }
      }),
    [client, runMutation, repoPath, setState],
  );
  const getPullStrategy = useCallback(() => client.getPullStrategy(repoPath), [client, repoPath]);
  const setPullStrategy = useCallback(
    async (strategy: PullStrategy): Promise<void> => {
      try {
        await client.setPullStrategy(repoPath, strategy);
      } catch (error) {
        logFailure("Remembering the pull strategy failed", error);
      }
    },
    [client, repoPath],
  );
  const resolveConflict = useCallback(
    (path: string, resolvedContent: string) =>
      runMutation(() => client.resolveConflict(repoPath, path, resolvedContent)),
    [client, runMutation, repoPath],
  );
  const resolveAddDeleteConflict = useCallback(
    (path: string, choice: FileConflictChoice) =>
      runMutation(() => client.resolveAddDeleteConflict(repoPath, path, choice)),
    [client, runMutation, repoPath],
  );
  const resolveWholeFile = useCallback(
    (path: string, side: ConflictSide) => runMutation(() => client.resolveWholeFile(repoPath, path, side)),
    [client, runMutation, repoPath],
  );
  const markResolved = useCallback(
    async (path: string, force: boolean): Promise<MarkResolvedOutcome | null> => {
      let outcome: MarkResolvedOutcome | null = null;
      await runMutation(async () => {
        outcome = await client.markResolved(repoPath, path, force);
      });
      return outcome;
    },
    [client, runMutation, repoPath],
  );
  const abortMerge = useCallback(
    () => runMutation(() => client.abortMerge(repoPath)),
    [client, runMutation, repoPath],
  );

  const openRebasePlanner = useCallback(
    (commitId: string) => {
      setState((prev) => ({ ...prev, rebaseOnto: commitId, rebasePreset: null }));
    },
    [setState],
  );
  const openRebaseSelection = useCallback(
    (onto: string, preset: ReadonlyMap<string, PresetAction>) => {
      setState((prev) => ({ ...prev, rebaseOnto: onto, rebasePreset: preset }));
    },
    [setState],
  );
  const closeRebasePlanner = useCallback(() => {
    setState((prev) => ({ ...prev, rebaseOnto: null, rebasePreset: null }));
  }, [setState]);

  const startRebase = useCallback(
    (onto: string, plan: RebasePlanEntry[]): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.startRebase(repoPath, onto, plan);
        void result;
        setState((prev) => ({ ...prev, rebaseOnto: null, rebasePreset: null }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const editHistory = useCallback(
    async (commitId: string, onto: string, request: HistoryEditRequest): Promise<RebaseStepResult | null> => {
      let result: RebaseStepResult | null = null;
      setState((prev) => ({ ...prev, historyEditOutcome: null }));
      await runMutation(async () => {
        const span = await client.commitsSince(repoPath, onto);
        let edit: HistoryEdit;
        if (request.kind === "Squash") {
          // The parent is the span's first commit; `buildHistoryEditPlan` checks the rest.
          if (span[1]?.id !== commitId) throw new Error(HISTORY_CHANGED);
          const [parentMessage, message] = await Promise.all([
            client.getCommitMessage(repoPath, span[0].id),
            client.getCommitMessage(repoPath, commitId),
          ]);
          edit = { kind: "Squash", combinedMessage: squashMessage(parentMessage, message) };
        } else {
          edit = request;
        }
        const plan = buildHistoryEditPlan(edit, commitId, span);
        const step = await client.startRebase(repoPath, onto, plan, { kind: request.kind, commitId });
        result = step;
        // A pause shows in the rebase progress panel instead.
        if (step.kind === "Done") {
          const shortId = span.find((commit) => commit.id === commitId)?.shortId ?? commitId.slice(0, 7);
          const message = historyEditDoneMessage(request.kind, shortId);
          setState((prev) => ({ ...prev, historyEditOutcome: { message } }));
        }
      });
      return result;
    },
    [client, runMutation, repoPath, setState],
  );
  const rebaseContinue = useCallback(
    (): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.rebaseContinue(repoPath);
        void result;
      }),
    [client, runMutation, repoPath],
  );
  const abortRebase = useCallback(
    async (): Promise<RebaseAbort | null> => {
      let abort: RebaseAbort | null = null;
      await runMutation(async () => {
        abort = await client.abortRebase(repoPath);
      });
      return abort;
    },
    [client, runMutation, repoPath],
  );

  return {
    getMergeOptions,
    mergeBranch,
    mergePull,
    rebasePull,
    getPullStrategy,
    setPullStrategy,
    resolveConflict,
    resolveAddDeleteConflict,
    resolveWholeFile,
    markResolved,
    abortMerge,
    openRebasePlanner,
    openRebaseSelection,
    closeRebasePlanner,
    startRebase,
    editHistory,
    rebaseContinue,
    abortRebase,
  };
}
