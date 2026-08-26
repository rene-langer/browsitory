import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState, SelectedRow } from "./useAppState";
import type { RunMutation } from "./useMutationRunner";

export interface StagingActions {
  selectRow(row: SelectedRow): void;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  // Bulk variants for DiffPane's "Stage all"/"Unstage all". There is no bulk backend op (and
  // none is planned) — these still make one `client.stageFile`/`unstageFile` IPC call per path,
  // but wrap the whole loop in a *single* `runMutation`, so a batch costs one `pending`/
  // `refresh()` cycle instead of one per file. `refresh()` alone is ~13 IPC reads plus one per
  // remote, all serialized through the single per-repo worker thread, so looping the per-file
  // action from the UI locked the app up on a large changeset.
  stageAllFiles(paths: string[]): Promise<void>;
  unstageAllFiles(paths: string[]): Promise<void>;
  stageHunk(path: string, oldStart: number, newStart: number): Promise<void>;
  unstageHunk(path: string, oldStart: number, newStart: number): Promise<void>;
  discardHunk(path: string, oldStart: number, newStart: number): Promise<void>;
  commit(message: string): Promise<void>;
}

export function useStagingActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): StagingActions {
  const selectRow = useCallback(
    (row: SelectedRow) => {
      setState((prev) => ({ ...prev, selectedRow: row }));
    },
    [setState],
  );

  const stageFile = useCallback(
    (path: string) => runMutation(() => client.stageFile(repoPath, path)),
    [client, runMutation, repoPath],
  );
  const unstageFile = useCallback(
    (path: string) => runMutation(() => client.unstageFile(repoPath, path)),
    [client, runMutation, repoPath],
  );
  // See the `stageAllFiles`/`unstageAllFiles` note above: one `runMutation` for the whole batch,
  // not one per path. Sequential rather than `Promise.all` because every call lands on the same
  // worker thread anyway, and index writes must not interleave.
  const stageAllFiles = useCallback(
    (paths: string[]) =>
      runMutation(async () => {
        for (const path of paths) {
          await client.stageFile(repoPath, path);
        }
      }),
    [client, runMutation, repoPath],
  );
  const unstageAllFiles = useCallback(
    (paths: string[]) =>
      runMutation(async () => {
        for (const path of paths) {
          await client.unstageFile(repoPath, path);
        }
      }),
    [client, runMutation, repoPath],
  );
  const stageHunk = useCallback(
    (path: string, oldStart: number, newStart: number) =>
      runMutation(() => client.stageHunk(repoPath, path, oldStart, newStart)),
    [client, runMutation, repoPath],
  );
  const unstageHunk = useCallback(
    (path: string, oldStart: number, newStart: number) =>
      runMutation(() => client.unstageHunk(repoPath, path, oldStart, newStart)),
    [client, runMutation, repoPath],
  );
  const discardHunk = useCallback(
    (path: string, oldStart: number, newStart: number) =>
      runMutation(() => client.discardHunk(repoPath, path, oldStart, newStart)),
    [client, runMutation, repoPath],
  );
  const commit = useCallback(
    (message: string) => runMutation(() => client.commit(repoPath, message)),
    [client, runMutation, repoPath],
  );

  return {
    selectRow,
    stageFile,
    unstageFile,
    stageAllFiles,
    unstageAllFiles,
    stageHunk,
    unstageHunk,
    discardHunk,
    commit,
  };
}
