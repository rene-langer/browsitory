import { useCallback } from "react";
import type {
  FileConflictChoice,
  MergeOutcome,
  RebasePlanEntry,
  RebaseStepResult,
  RepoClient,
} from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation } from "./useMutationRunner";

export interface MergeRebaseActions {
  mergeBranch(branchName: string): Promise<void>;
  resolveConflict(path: string, resolvedContent: string): Promise<void>;
  resolveAddDeleteConflict(path: string, choice: FileConflictChoice): Promise<void>;
  abortMerge(): Promise<void>;
  openRebasePlanner(commitId: string): void;
  openSquashPlanner(ontoId: string, squashIds: string[]): void;
  closeRebasePlanner(): void;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<void>;
  rebaseContinue(): Promise<void>;
  abortRebase(): Promise<void>;
}

export function useMergeRebaseActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): MergeRebaseActions {
  const mergeBranch = useCallback(
    (branchName: string): Promise<void> =>
      runMutation(async () => {
        const outcome: MergeOutcome = await client.mergeBranch(repoPath, branchName);
        void outcome;
      }),
    [client, runMutation, repoPath],
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
  const abortMerge = useCallback(
    () => runMutation(() => client.abortMerge(repoPath)),
    [client, runMutation, repoPath],
  );

  const openRebasePlanner = useCallback(
    (commitId: string) => {
      setState((prev) => ({ ...prev, rebaseOnto: commitId, squashPreset: null }));
    },
    [setState],
  );
  const openSquashPlanner = useCallback(
    (ontoId: string, squashIds: string[]) => {
      setState((prev) => ({ ...prev, rebaseOnto: ontoId, squashPreset: new Set(squashIds) }));
    },
    [setState],
  );
  const closeRebasePlanner = useCallback(() => {
    setState((prev) => ({ ...prev, rebaseOnto: null, squashPreset: null }));
  }, [setState]);

  const startRebase = useCallback(
    (onto: string, plan: RebasePlanEntry[]): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.startRebase(repoPath, onto, plan);
        void result;
        setState((prev) => ({ ...prev, rebaseOnto: null, squashPreset: null }));
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
    () => runMutation(() => client.abortRebase(repoPath)),
    [client, runMutation, repoPath],
  );

  return {
    mergeBranch,
    resolveConflict,
    resolveAddDeleteConflict,
    abortMerge,
    openRebasePlanner,
    openSquashPlanner,
    closeRebasePlanner,
    startRebase,
    rebaseContinue,
    abortRebase,
  };
}
