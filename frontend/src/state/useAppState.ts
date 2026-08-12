import { useCallback, useState } from "react";
import type { CommitInfo, RepoClient, StatusEntry } from "../ipc/RepoClient";

const LOG_LIMIT = 300;

export type SelectedRow = "uncommitted" | { commitId: string };

export interface AppState {
  repoPath: string | null;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  log: CommitInfo[];
  error: string | null;
}

export interface UseAppStateResult {
  state: AppState;
  openRepo(path: string): Promise<void>;
  selectRow(row: SelectedRow): void;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  commit(message: string): Promise<void>;
  refresh(): Promise<void>;
}

export function useAppState(client: RepoClient): UseAppStateResult {
  const [state, setState] = useState<AppState>({
    repoPath: null,
    selectedRow: "uncommitted",
    status: [],
    log: [],
    error: null,
  });

  const refresh = useCallback(async () => {
    try {
      const [status, log] = await Promise.all([
        client.getStatus(),
        client.getLog(LOG_LIMIT),
      ]);
      setState((prev) => ({ ...prev, status, log, error: null }));
    } catch (err) {
      setState((prev) => ({ ...prev, error: String(err) }));
    }
  }, [client]);

  const runMutation = useCallback(
    async (mutate: () => Promise<void>) => {
      try {
        await mutate();
        await refresh();
      } catch (err) {
        setState((prev) => ({ ...prev, error: String(err) }));
      }
    },
    [refresh],
  );

  const openRepo = useCallback(
    (path: string) =>
      runMutation(async () => {
        await client.openRepo(path);
        setState((prev) => ({ ...prev, repoPath: path, selectedRow: "uncommitted" }));
      }),
    [client, runMutation],
  );

  const selectRow = useCallback((row: SelectedRow) => {
    setState((prev) => ({ ...prev, selectedRow: row }));
  }, []);

  const stageFile = useCallback(
    (path: string) => runMutation(() => client.stageFile(path)),
    [client, runMutation],
  );
  const unstageFile = useCallback(
    (path: string) => runMutation(() => client.unstageFile(path)),
    [client, runMutation],
  );
  const commit = useCallback(
    (message: string) => runMutation(() => client.commit(message)),
    [client, runMutation],
  );

  return { state, openRepo, selectRow, stageFile, unstageFile, commit, refresh };
}
