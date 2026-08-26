import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { RunMutation, RunMutationWithMessage } from "./useMutationRunner";

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
  runMutationWithMessage: RunMutationWithMessage,
): WorktreeActions {
  const createWorktree = useCallback(
    (name: string, path: string, branch: string, startPoint: string | null) =>
      runMutationWithMessage(() => client.createWorktree(repoPath, name, path, branch, startPoint)),
    [client, runMutationWithMessage, repoPath],
  );
  const removeWorktree = useCallback(
    (name: string) => runMutation(() => client.removeWorktree(repoPath, name)),
    [client, runMutation, repoPath],
  );
  const pruneWorktrees = useCallback(
    () => runMutation(() => client.pruneWorktrees(repoPath)),
    [client, runMutation, repoPath],
  );

  return { createWorktree, removeWorktree, pruneWorktrees };
}
