import { useCallback, useRef } from "react";
import type {
  RebaseAbort,
  ConflictSide,
  FastForwardMode,
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
  /**
   * A pull's merge, no dialog: "Pull has diverged → Merge" allows a fast-forward (the default);
   * a pull git config wants as a merge commit passes `Never`, or `Only` when a remembered rebase
   * or fast-forward-only strategy fast-forwards it instead (#740).
   */
  mergePull(upstreamRef: string, fastForward?: FastForwardMode): Promise<void>;
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
  /**
   * Folds the staged changes into `commitId` (#883); toasts when done, a pause shows in the
   * rebase panel. Resolves `null` when it failed (the error is in `state.error`).
   */
  foldStaged(commitId: string): Promise<RebaseStepResult | null>;
  /**
   * Continues the paused rebase. At an edit stop, an edited message (`setRebaseEditMessage`) is
   * sent along once, as the recommitted commit's message (#882).
   */
  rebaseContinue(): Promise<void>;
  /**
   * The message the commit dock's edit stop holds for the recommit, or `null` when it is the
   * commit's own (unchanged): what the next Continue sends (#882).
   */
  setRebaseEditMessage(message: string | null): void;
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
    (upstreamRef: string, fastForward: FastForwardMode = "Allow") => runMerge(upstreamRef, { fastForward, squash: false }),
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
  const foldStaged = useCallback(
    async (commitId: string): Promise<RebaseStepResult | null> => {
      let result: RebaseStepResult | null = null;
      setState((prev) => ({ ...prev, historyEditOutcome: null }));
      await runMutation(async () => {
        const step = await client.foldStagedInto(repoPath, commitId);
        result = step;
        if (step.kind === "Done") {
          const message = `Folded staged changes into ${commitId.slice(0, 7)}`;
          setState((prev) => ({ ...prev, historyEditOutcome: { message } }));
        }
      });
      return result;
    },
    [client, runMutation, repoPath, setState],
  );
  // Not state: only Continue reads it, and typing in the message box re-renders nothing else.
  const rebaseEditMessage = useRef<string | null>(null);
  const setRebaseEditMessage = useCallback((message: string | null) => {
    rebaseEditMessage.current = message;
  }, []);
  const rebaseContinue = useCallback(
    (): Promise<void> =>
      runMutation(async () => {
        const message = rebaseEditMessage.current;
        const result: RebaseStepResult =
          message === null ? await client.rebaseContinue(repoPath) : await client.rebaseContinue(repoPath, message);
        // Landed: the message belonged to that stop, not to the next one.
        rebaseEditMessage.current = null;
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
    foldStaged,
    rebaseContinue,
    setRebaseEditMessage,
    abortRebase,
  };
}
