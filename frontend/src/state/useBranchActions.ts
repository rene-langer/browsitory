import { useCallback, useEffect, useRef } from "react";
import { coversEveryBranch, withRenamedBranch, withoutBranch } from "../lib/graphSelection";
import { errorKind, type RepoClient, type StashSwitchOutcome, type UndoOutcome } from "../ipc/RepoClient";
import { refNameClash, takenNameMessage } from "../lib/takenNames";
import type { SavedGraphSelection } from "./graphSelection";
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
  // `checkout` (default true): switch to the new branch too, as one undo step (#656).
  createBranch(name: string, startPoint: string, checkout?: boolean): Promise<string | null>;
  // A switch refused by local changes in the way also sets `switchConflict` (#653), so the
  // Stash and switch dialog opens over the error.
  switchBranch(name: string): Promise<void>;
  /**
   * "Stash and switch" (#653): stashes every local change, switches to `name` and, with
   * `reapply`, pops the changes there. Resolves its outcome (also in `stashSwitchOutcome` for the
   * toast), or `null` when it failed (the error is in `state.error`).
   */
  stashAndSwitch(name: string, reapply: boolean): Promise<StashSwitchOutcome | null>;
  closeSwitchConflict(): void;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  setGraphBranchSelection(local: string[]): Promise<void>;
  setGraphRemoteBranchSelection(remote: string[]): Promise<void>;
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

/**
 * What `useBranchActions` gives the undo actions besides `BranchActions`: after an undo or redo
 * step ran, it puts the branch of an undone delete back into the saved graph selection, or takes
 * the branch of a redone one out again (#112).
 */
export type AfterUndoStep = (verb: "undo" | "redo", outcome: UndoOutcome) => Promise<void>;

/** The branch an undo step's label names when the step is a branch delete, else `null`. */
function deletedBranch(label: string): string | null {
  const prefix = "Delete branch ";
  return label.startsWith(prefix) ? label.slice(prefix.length) : null;
}

function sameSelection(a: string[] | null, b: string[] | null): boolean {
  if (a === null || b === null) return a === b;
  return a.length === b.length && a.every((name, index) => name === b[index]);
}

export function useBranchActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  setState: (updater: (prev: AppState) => AppState) => void,
  graphSelection: SavedGraphSelection,
  branchNames: string[],
): BranchActions & { afterUndoStep: AfterUndoStep } {
  // Read at mutation time, after the backend call succeeded (see deleteBranch/renameBranch).
  const selectionRef = useRef(graphSelection.local);
  const remoteSelectionRef = useRef(graphSelection.remote);
  const branchNamesRef = useRef(branchNames);
  useEffect(() => {
    selectionRef.current = graphSelection.local;
    remoteSelectionRef.current = graphSelection.remote;
    branchNamesRef.current = branchNames;
  }, [graphSelection, branchNames]);

  // The branch operation already succeeded; a failure to rewrite the saved selection must not
  // report it as failed (the graph skips missing names anyway, #97), so it is only logged.
  const updateSelection = useCallback(
    async (next: string[] | "clear", after: "a branch delete" | "a branch rename" | "undoing a branch delete") => {
      try {
        if (next === "clear") await client.clearGraphBranchSelection(repoPath);
        else await client.setGraphBranchSelection(repoPath, next);
      } catch (error) {
        void client
          .logFrontendError(`Updating the graph branch selection after ${after} failed`, error)
          .catch(() => {});
      }
    },
    [client, repoPath],
  );

  // The saved selections a branch delete rewrote, by branch name: the one before and the one it
  // left, so undoing the delete can put the branch back (#112). Memory only, like the toast
  // that offers the undo.
  const rewrittenByDelete = useRef(new Map<string, { before: string[]; after: string[] | null }>());

  // After `name` was deleted (or its delete redone): a persisted selection naming it would
  // otherwise linger (#97), and every branch left shown is the default graph, so the list is
  // forgotten (#402).
  const dropFromSelection = useCallback(
    async (name: string) => {
      const before = selectionRef.current;
      const remaining = branchNamesRef.current.filter((candidate) => candidate !== name);
      const next = withoutBranch(before, name, remaining);
      if (next === null || before === null) return;
      const after = coversEveryBranch(next, remaining) ? null : next;
      await updateSelection(after ?? "clear", "a branch delete");
      rewrittenByDelete.current.set(name, { before, after });
    },
    [updateSelection],
  );

  // An undone delete puts back the selection it rewrote, unless it was changed since; a redone
  // one takes the branch out again.
  const afterUndoStep = useCallback<AfterUndoStep>(
    async (verb, outcome) => {
      if (outcome.kind !== "Done") return;
      const name = deletedBranch(outcome.label);
      if (name === null) return;
      if (verb === "redo") {
        await dropFromSelection(name);
        return;
      }
      const rewritten = rewrittenByDelete.current.get(name);
      if (rewritten === undefined) return;
      rewrittenByDelete.current.delete(name);
      if (!sameSelection(selectionRef.current, rewritten.after)) return;
      await updateSelection(rewritten.before, "undoing a branch delete");
    },
    [dropFromSelection, updateSelection],
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
    async (name: string, startPoint: string, checkout = true) => {
      const refused = refuseTaken(name, null);
      if (refused !== null) return refused;
      return runOptimisticMutationWithMessage(
        (prev) =>
          // Never in place of a branch that arrived since the check above.
          refNameClash(prev.branches.map((branch) => branch.name), name) !== null
            ? prev
            : { ...prev, branches: [...prev.branches, { name, isCurrent: false }] },
        async () => {
          await client.createBranch(repoPath, name, startPoint, checkout);
          setState((prev) => ({ ...prev, createBranchDraft: null, selectedRow: "uncommitted" }));
        },
      );
    },
    [client, refuseTaken, runOptimisticMutationWithMessage, repoPath, setState],
  );
  const switchBranch = useCallback(
    (name: string) =>
      runMutation(async () => {
        try {
          await client.switchBranch(repoPath, name);
        } catch (error) {
          if (errorKind(error) === "LocalChanges") {
            // Named in the dialog; an empty list still offers Stash and switch.
            const files = await client.getSwitchConflicts(repoPath, name).catch(() => []);
            setState((prev) => ({ ...prev, switchConflict: { branchName: name, files } }));
          }
          throw error;
        }
        setState((prev) => ({ ...prev, selectedRow: "uncommitted", pullOutcome: null }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const stashAndSwitch = useCallback(
    async (name: string, reapply: boolean): Promise<StashSwitchOutcome | null> => {
      let result: StashSwitchOutcome | null = null;
      setState((prev) => ({ ...prev, switchConflict: null, stashSwitchOutcome: null }));
      await runMutation(async () => {
        const outcome = await client.stashAndSwitch(repoPath, name, reapply);
        result = outcome;
        setState((prev) => ({
          ...prev,
          selectedRow: "uncommitted",
          pullOutcome: null,
          stashSwitchOutcome: { branchName: name, outcome },
        }));
      });
      return result;
    },
    [client, runMutation, repoPath, setState],
  );
  const closeSwitchConflict = useCallback(() => {
    setState((prev) => ({ ...prev, switchConflict: null }));
  }, [setState]);
  const deleteBranch = useCallback(
    (name: string, force: boolean) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, branches: prev.branches.filter((branch) => branch.name !== name) }),
        async () => {
          await client.deleteBranch(repoPath, name, force);
          await dropFromSelection(name);
        },
      ),
    [client, runOptimisticMutation, repoPath, dropFromSelection],
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
          if (next !== null) await updateSelection(next, "a branch rename");
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
    (local: string[]) => runMutation(() => client.setGraphBranchSelection(repoPath, local)),
    [client, runMutation, repoPath],
  );

  const setGraphRemoteBranchSelection = useCallback(
    (remote: string[]) => runMutation(() => client.setGraphRemoteBranchSelection(repoPath, remote)),
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
    stashAndSwitch,
    closeSwitchConflict,
    deleteBranch,
    renameBranch,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    setGraphBranchSelection,
    setGraphRemoteBranchSelection,
    clearGraphBranchSelection,
    showAllGraphBranches,
    isolateGraphBranch,
    afterUndoStep,
  };
}
