import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
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
): WorktreeActions {
  const createWorktree = useCallback(
    (name: string, path: string, branch: string, startPoint: string | null) =>
      runOptimisticMutationWithMessage(
        (prev) => ({
          ...prev,
          worktrees: [...prev.worktrees, { name, path, head: null, isMain: false, isLocked: false, isPrunable: false }],
        }),
        () => client.createWorktree(repoPath, name, path, branch, startPoint),
      ),
    [client, runOptimisticMutationWithMessage, repoPath],
  );
  const removeWorktree = useCallback(
    (name: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, worktrees: prev.worktrees.filter((worktree) => worktree.name !== name) }),
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
