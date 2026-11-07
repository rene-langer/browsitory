import { useCallback, useEffect, useRef } from "react";
import type { RepoClient, WorktreeInfo } from "../ipc/RepoClient";
import { comparableWorktreePath, takenNameMessage } from "../lib/takenNames";
import type { AppState } from "./useAppState";
import type {
  RunMutation,
  RunOptimisticMutation,
  RunOptimisticMutationWithMessage,
} from "./useMutationRunner";

export interface WorktreeActions {
  // Resolves to `null` on success, or the failure message on failure — the "Create worktree"
  // form is its own natural trigger point (see `createBranch`'s doc comment in `useAppState`
  // for the shared rationale behind this shape).
  createWorktree(name: string, path: string, branch: string, startPoint: string | null): Promise<string | null>;
  removeWorktree(name: string): Promise<void>;
  pruneWorktrees(): Promise<void>;
}

export function useWorktreeActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  setState: (updater: (prev: AppState) => AppState) => void,
  worktrees: WorktreeInfo[],
): WorktreeActions {
  const worktreesRef = useRef<WorktreeInfo[]>(worktrees);
  useEffect(() => {
    worktreesRef.current = worktrees;
  }, [worktrees]);

  const createWorktree = useCallback(
    async (name: string, path: string, branch: string, startPoint: string | null) => {
      // A linked worktree's name is its folder under `.git/worktrees`, so a listed one is taken
      // for sure: refused before the optimistic row (#413). The main worktree's "main" is only a
      // label, not a taken name.
      if (worktreesRef.current.some((worktree) => !worktree.isMain && worktree.name === name)) {
        const message = takenNameMessage("worktree", name, name);
        setState((prev) => ({ ...prev, error: message }));
        return message;
      }
      // A taken path is left to the backend, which refuses any existing folder and knows how the
      // drive compares paths (symlinks, case folding); a listed worktree whose folder was deleted
      // can even be replaced there. The optimistic row is only skipped when the path looks like
      // a listed one, so the same folder is never shown twice while the backend decides.
      const comparable = comparableWorktreePath(path, repoPath);
      return runOptimisticMutationWithMessage(
        (prev) =>
          prev.worktrees.some(
            (worktree) =>
              (!worktree.isMain && worktree.name === name) || comparableWorktreePath(worktree.path, repoPath) === comparable,
          )
            ? prev
            : {
                ...prev,
                worktrees: [...prev.worktrees, { name, path, head: null, isMain: false, isLocked: false, isPrunable: false }],
              },
        () => client.createWorktree(repoPath, name, path, branch, startPoint),
      );
    },
    [client, runOptimisticMutationWithMessage, repoPath, setState],
  );
  const removeWorktree = useCallback(
    (name: string) =>
      runOptimisticMutation(
        // Only linked worktrees are removed: one may be named "main" like the main worktree's label.
        (prev) => ({ ...prev, worktrees: prev.worktrees.filter((worktree) => worktree.isMain || worktree.name !== name) }),
        () => client.removeWorktree(repoPath, name),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const pruneWorktrees = useCallback(
    () => runMutation(() => client.pruneWorktrees(repoPath)),
    [client, runMutation, repoPath],
  );

  return { createWorktree, removeWorktree, pruneWorktrees };
}
