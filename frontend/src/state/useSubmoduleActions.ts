import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { RunMutation } from "./useMutationRunner";

export interface SubmoduleActions {
  initSubmodule(path: string): Promise<void>;
  updateSubmodule(path: string, recursive: boolean): Promise<void>;
}

export function useSubmoduleActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
): SubmoduleActions {
  const initSubmodule = useCallback(
    (path: string) => runMutation(() => client.initSubmodule(repoPath, path)),
    [client, runMutation, repoPath],
  );
  const updateSubmodule = useCallback(
    (path: string, recursive: boolean) =>
      runMutation(() => client.updateSubmodule(repoPath, path, recursive)),
    [client, runMutation, repoPath],
  );

  return { initSubmodule, updateSubmodule };
}
