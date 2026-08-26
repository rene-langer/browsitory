import { useCallback } from "react";
import type { RepoClient, StatusEntry, StatusKind } from "../ipc/RepoClient";
import type { AppState, SelectedRow } from "./useAppState";
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";

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

// A path can have two StatusEntry rows simultaneously (staged + unstaged, for a partially-staged
// file — see crates/git-core/src/status.rs). stageFile/unstageFile/stageAllFiles/unstageAllFiles
// are whole-file operations, so the predicted result replaces every existing row for each target
// path with exactly one row at the new `staged` value, preserving `kind`.
function withFilesStaged(status: StatusEntry[], paths: string[], staged: boolean): StatusEntry[] {
  const targets = new Set(paths);
  const kindByPath = new Map<string, StatusKind>();
  for (const entry of status) {
    if (targets.has(entry.path)) kindByPath.set(entry.path, entry.kind);
  }
  const untouched = status.filter((entry) => !targets.has(entry.path));
  const updated = paths
    .filter((path) => kindByPath.has(path))
    .map((path) => ({ path, staged, kind: kindByPath.get(path) as StatusKind }));
  return [...untouched, ...updated];
}

export function useStagingActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): StagingActions {
  const selectRow = useCallback(
    (row: SelectedRow) => {
      setState((prev) => ({ ...prev, selectedRow: row }));
    },
    [setState],
  );

  const stageFile = useCallback(
    (path: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, status: withFilesStaged(prev.status, [path], true) }),
        () => client.stageFile(repoPath, path),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const unstageFile = useCallback(
    (path: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, status: withFilesStaged(prev.status, [path], false) }),
        () => client.unstageFile(repoPath, path),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  // See the `stageAllFiles`/`unstageAllFiles` note above: one `runMutation` for the whole batch,
  // not one per path. Sequential rather than `Promise.all` because every call lands on the same
  // worker thread anyway, and index writes must not interleave.
  const stageAllFiles = useCallback(
    (paths: string[]) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, status: withFilesStaged(prev.status, paths, true) }),
        async () => {
          for (const path of paths) {
            await client.stageFile(repoPath, path);
          }
        },
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const unstageAllFiles = useCallback(
    (paths: string[]) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, status: withFilesStaged(prev.status, paths, false) }),
        async () => {
          for (const path of paths) {
            await client.unstageFile(repoPath, path);
          }
        },
      ),
    [client, runOptimisticMutation, repoPath],
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
