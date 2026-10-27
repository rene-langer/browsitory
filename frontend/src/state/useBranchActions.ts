import { useCallback, useEffect, useRef } from "react";
import { withRenamedBranch, withoutBranch } from "../lib/graphSelection";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type {
  RunMutation,
  RunOptimisticMutation,
  RunOptimisticMutationWithMessage,
} from "./useMutationRunner";

export interface BranchActions {
  // Resolves to `null` on success, or the failure message on failure — mirrors `addRemote`.
  // Naming a new branch is the one create-form action here with an obvious single trigger point
  // (the "New branch…" draft form), so its failure surfaces next to that form instead of the
  // shared banner (issue #30/UX-002).
  createBranch(name: string, startPoint: string): Promise<string | null>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  setGraphBranchSelection(selectedBranches: string[]): Promise<void>;
  setGraphRemoteBranchSelection(selectedBranches: string[]): Promise<void>;
}

export function useBranchActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  setState: (updater: (prev: AppState) => AppState) => void,
  graphBranchSelection: string[] | null,
  branchNames: string[],
): BranchActions {
  // Read at mutation time, after the backend call succeeded (see deleteBranch/renameBranch).
  const selectionRef = useRef(graphBranchSelection);
  const branchNamesRef = useRef(branchNames);
  useEffect(() => {
    selectionRef.current = graphBranchSelection;
    branchNamesRef.current = branchNames;
  }, [graphBranchSelection, branchNames]);

  // The branch operation already succeeded; a failure to rewrite the saved selection must not
  // report it as failed (the graph skips missing names anyway, #97), so it is only logged.
  const updateSelection = useCallback(
    async (next: string[], after: "delete" | "rename") => {
      try {
        await client.setGraphBranchSelection(repoPath, next);
      } catch (error) {
        void client
          .logFrontendError(`Updating the graph branch selection after a branch ${after} failed`, error)
          .catch(() => {});
      }
    },
    [client, repoPath],
  );

  const createBranch = useCallback(
    (name: string, startPoint: string) =>
      runOptimisticMutationWithMessage(
        (prev) => ({ ...prev, branches: [...prev.branches, { name, isCurrent: false }] }),
        async () => {
          await client.createBranch(repoPath, name, startPoint);
          setState((prev) => ({ ...prev, createBranchDraft: null, selectedRow: "uncommitted" }));
        },
      ),
    [client, runOptimisticMutationWithMessage, repoPath, setState],
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
    (name: string, force: boolean) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, branches: prev.branches.filter((branch) => branch.name !== name) }),
        async () => {
          await client.deleteBranch(repoPath, name, force);
          // A persisted selection naming the deleted branch would otherwise linger (#97).
          const next = withoutBranch(
            selectionRef.current,
            name,
            branchNamesRef.current.filter((candidate) => candidate !== name),
          );
          if (next !== null) await updateSelection(next, "delete");
        },
      ),
    [client, runOptimisticMutation, repoPath, updateSelection],
  );
  const renameBranch = useCallback(
    (oldName: string, newName: string) =>
      runOptimisticMutation(
        (prev) => ({
          ...prev,
          branches: prev.branches.map((branch) => (branch.name === oldName ? { ...branch, name: newName } : branch)),
        }),
        async () => {
          await client.renameBranch(repoPath, oldName, newName);
          const next = withRenamedBranch(selectionRef.current, oldName, newName);
          if (next !== null) await updateSelection(next, "rename");
        },
      ),
    [client, runOptimisticMutation, repoPath, updateSelection],
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

  const setGraphRemoteBranchSelection = useCallback(
    (selectedBranches: string[]) =>
      runMutation(() => client.setGraphRemoteBranchSelection(repoPath, selectedBranches)),
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
    setGraphRemoteBranchSelection,
  };
}
