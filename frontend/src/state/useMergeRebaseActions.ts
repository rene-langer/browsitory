import { useCallback } from "react";
import type {
  RebaseAbort,
  ConflictSide,
  FileConflictChoice,
  MarkResolvedOutcome,
  MergeOptions,
  MergeOutcome,
  RebasePlanEntry,
  RebaseStepResult,
  RepoClient,
} from "../ipc/RepoClient";
import { logFailure } from "../lib/logger";
import type { PresetAction } from "../lib/rebaseSelection";
import type { AppState } from "./useAppState";
import type { RunMutation } from "./useMutationRunner";

export interface MergeRebaseActions {
  /** Opens the merge options dialog for `branchName` (BranchTree menu, palette). */
  openMergeDialog(branchName: string): void;
  closeMergeDialog(): void;
  /** The dialog's preselection: remembered per repo, else from `merge.ff`. */
  getMergeOptions(): Promise<MergeOptions>;
  /**
   * Remembers `options` for this repo, closes the dialog and merges `branchName` with them,
   * recording the outcome in `state.mergeOutcome` for the toast.
   */
  mergeBranch(branchName: string, options: MergeOptions): Promise<void>;
  /** "Pull has diverged → Merge": merges the upstream ref, allowing fast-forward, no dialog. */
  mergePull(upstreamRef: string): Promise<void>;
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
  const openMergeDialog = useCallback(
    (branchName: string) => {
      setState((prev) => ({ ...prev, mergeDialog: { branchName }, mergeOutcome: null }));
    },
    [setState],
  );
  const closeMergeDialog = useCallback(() => {
    setState((prev) => ({ ...prev, mergeDialog: null }));
  }, [setState]);
  const getMergeOptions = useCallback(() => client.getMergeOptions(repoPath), [client, repoPath]);
  const runMerge = useCallback(
    (branchName: string, options: MergeOptions): Promise<void> =>
      runMutation(async () => {
        setState((prev) => ({ ...prev, mergeDialog: null, mergeOutcome: null }));
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
    openMergeDialog,
    closeMergeDialog,
    getMergeOptions,
    mergeBranch,
    mergePull,
    resolveConflict,
    resolveAddDeleteConflict,
    resolveWholeFile,
    markResolved,
    abortMerge,
    openRebasePlanner,
    openRebaseSelection,
    closeRebasePlanner,
    startRebase,
    rebaseContinue,
    abortRebase,
  };
}
