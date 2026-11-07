import { useCallback, useEffect, useRef } from "react";
import { coversEveryBranch, withRenamedBranch, withoutBranch } from "../lib/graphSelection";
import type { RepoClient } from "../ipc/RepoClient";
import { refNameClash, takenNameMessage } from "../lib/takenNames";
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
  // Forgets the saved local selection: the graph walks every local branch again, also ones
  // created later, and never just one branch's first parents (#402). The remote one stays.
  clearGraphBranchSelection(): Promise<void>;
  // "Show all branches": forgets both saved selections in one mutation, back to the default
  // graph (every local branch, no remote-only commits, every remote badge; #402).
  showAllGraphBranches(): Promise<void>;
  // "Isolate branch": the graph shows only `name`, a local branch or a remote one
  // (`<remote>/<branch>`). Both selections are saved before the one reload, so no other branch,
  // local or remote, joins the walk and the graph follows that branch's first parents (#385).
  isolateGraphBranch(kind: "local" | "remote", name: string): Promise<void>;
}

export function useBranchActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  setState: (updater: (prev: AppState) => AppState) => void,
  graphBranchSelection: string[] | null,
  graphRemoteBranchSelection: string[] | null,
  branchNames: string[],
): BranchActions {
  // Read at mutation time, after the backend call succeeded (see deleteBranch/renameBranch).
  const selectionRef = useRef(graphBranchSelection);
  const remoteSelectionRef = useRef(graphRemoteBranchSelection);
  const branchNamesRef = useRef(branchNames);
  useEffect(() => {
    selectionRef.current = graphBranchSelection;
    remoteSelectionRef.current = graphRemoteBranchSelection;
    branchNamesRef.current = branchNames;
  }, [graphBranchSelection, graphRemoteBranchSelection, branchNames]);

  // The branch operation already succeeded; a failure to rewrite the saved selection must not
  // report it as failed (the graph skips missing names anyway, #97), so it is only logged.
  const updateSelection = useCallback(
    async (next: string[] | "clear", after: "delete" | "rename") => {
      try {
        if (next === "clear") await client.clearGraphBranchSelection(repoPath);
        else await client.setGraphBranchSelection(repoPath, next);
      } catch (error) {
        void client
          .logFrontendError(`Updating the graph branch selection after a branch ${after} failed`, error)
          .catch(() => {});
      }
    },
    [client, repoPath],
  );

  // A name that is taken (or a folder of a taken one) is refused before the optimistic update:
  // `buildBranchTree` would otherwise put the new row in the existing branch's place until the
  // backend refused it (#413). `except` is the branch being renamed.
  const refuseTaken = useCallback(
    (name: string, except: string | null): string | null => {
      const others = branchNamesRef.current.filter((candidate) => candidate !== except);
      const clash = refNameClash(others, name);
      if (clash === null) return null;
      const message = takenNameMessage("branch", name, clash);
      setState((prev) => ({ ...prev, error: message }));
      return message;
    },
    [setState],
  );

  const createBranch = useCallback(
    async (name: string, startPoint: string) => {
      const refused = refuseTaken(name, null);
      if (refused !== null) return refused;
      return runOptimisticMutationWithMessage(
        (prev) =>
          // Never in place of a branch that arrived since the check above.
          refNameClash(prev.branches.map((branch) => branch.name), name) !== null
            ? prev
            : { ...prev, branches: [...prev.branches, { name, isCurrent: false }] },
        async () => {
          await client.createBranch(repoPath, name, startPoint);
          setState((prev) => ({ ...prev, createBranchDraft: null, selectedRow: "uncommitted" }));
        },
      );
    },
    [client, refuseTaken, runOptimisticMutationWithMessage, repoPath, setState],
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
          const remaining = branchNamesRef.current.filter((candidate) => candidate !== name);
          const next = withoutBranch(selectionRef.current, name, remaining);
          // Every branch left shown is the default graph: forget the list (#402).
          if (next !== null) await updateSelection(coversEveryBranch(next, remaining) ? "clear" : next, "delete");
        },
      ),
    [client, runOptimisticMutation, repoPath, updateSelection],
  );
  const renameBranch = useCallback(
    async (oldName: string, newName: string) => {
      if (refuseTaken(newName, oldName) !== null) return;
      await runOptimisticMutation(
        (prev) =>
          refNameClash(
            prev.branches.map((branch) => branch.name).filter((name) => name !== oldName),
            newName,
          ) !== null
            ? prev
            : {
                ...prev,
                branches: prev.branches.map((branch) => (branch.name === oldName ? { ...branch, name: newName } : branch)),
              },
        async () => {
          await client.renameBranch(repoPath, oldName, newName);
          const next = withRenamedBranch(selectionRef.current, oldName, newName);
          if (next !== null) await updateSelection(next, "rename");
        },
      );
    },
    [client, refuseTaken, runOptimisticMutation, repoPath, updateSelection],
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

  const clearGraphBranchSelection = useCallback(
    () => runMutation(() => client.clearGraphBranchSelection(repoPath)),
    [client, runMutation, repoPath],
  );

  const showAllGraphBranches = useCallback(
    () =>
      runMutation(async () => {
        await client.clearGraphBranchSelection(repoPath);
        await client.clearGraphRemoteBranchSelection(repoPath);
      }),
    [client, runMutation, repoPath],
  );

  const isolateGraphBranch = useCallback(
    (kind: "local" | "remote", name: string) =>
      runMutation(async () => {
        if (kind === "local") {
          await client.setGraphBranchSelection(repoPath, [name]);
          // A remote selection that was never saved walks no remote branch and keeps every
          // remote badge; only a saved one with names in it would add commits.
          const remote = remoteSelectionRef.current;
          if (remote !== null && remote.length > 0) await client.setGraphRemoteBranchSelection(repoPath, []);
        } else {
          await client.setGraphBranchSelection(repoPath, []);
          await client.setGraphRemoteBranchSelection(repoPath, [name]);
        }
      }),
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
    clearGraphBranchSelection,
    showAllGraphBranches,
    isolateGraphBranch,
  };
}
