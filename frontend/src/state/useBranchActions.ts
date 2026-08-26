import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type {
  RunMutation,
  RunMutationWithMessage,
  RunOptimisticMutation,
  RunOptimisticMutationWithMessage,
} from "./useMutationRunner";

export interface BranchActions {
  // Resolves to `null` on success, or the failure message on failure — mirrors `addRemote`.
  // Naming a new branch is the one create-form action here with an obvious single trigger point
  // (the "New Branch…" draft form), so its failure surfaces next to that form instead of the
  // shared banner (issue #30/UX-002).
  createBranch(name: string, startPoint: string): Promise<string | null>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  setGraphBranchSelection(selectedBranches: string[]): Promise<void>;
}

export function useBranchActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runMutationWithMessage: RunMutationWithMessage,
  _runOptimisticMutation: RunOptimisticMutation,
  _runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  setState: (updater: (prev: AppState) => AppState) => void,
): BranchActions {
  const createBranch = useCallback(
    (name: string, startPoint: string) =>
      runMutationWithMessage(async () => {
        await client.createBranch(repoPath, name, startPoint);
        setState((prev) => ({ ...prev, createBranchDraft: null, selectedRow: "uncommitted" }));
      }),
    [client, runMutationWithMessage, repoPath, setState],
  );
  const switchBranch = useCallback(
    (name: string) =>
      runMutation(async () => {
        await client.switchBranch(repoPath, name);
        setState((prev) => ({ ...prev, selectedRow: "uncommitted", pullOutcome: null }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const deleteBranch = useCallback(
    (name: string, force: boolean) => runMutation(() => client.deleteBranch(repoPath, name, force)),
    [client, runMutation, repoPath],
  );
  const renameBranch = useCallback(
    (oldName: string, newName: string) => runMutation(() => client.renameBranch(repoPath, oldName, newName)),
    [client, runMutation, repoPath],
  );

  const openCreateBranchDraft = useCallback(
    (startPoint: string) => {
      setState((prev) => ({ ...prev, createBranchDraft: { startPoint } }));
    },
    [setState],
  );
  const closeCreateBranchDraft = useCallback(() => {
    setState((prev) => ({ ...prev, createBranchDraft: null }));
  }, [setState]);

  const setGraphBranchSelection = useCallback(
    (selectedBranches: string[]) =>
      runMutation(() => client.setGraphBranchSelection(repoPath, selectedBranches)),
    [client, runMutation, repoPath],
  );

  return {
    createBranch,
    switchBranch,
    deleteBranch,
    renameBranch,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    setGraphBranchSelection,
  };
}
