import { useCallback, useState } from "react";
import type {
  BranchInfo,
  CommitInfo,
  RepoClient,
  StashEntry,
  StatusEntry,
} from "../ipc/RepoClient";

const LOG_LIMIT = 300;

export type SelectedRow = "uncommitted" | { commitId: string };

export interface AppState {
  repoPath: string | null;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  log: CommitInfo[];
  branches: BranchInfo[];
  createBranchDraft: { startPoint: string } | null;
  stashes: StashEntry[];
  error: string | null;
  // True only while a `runMutation` call is in flight (from just before its `mutate()` call
  // through the trailing `refresh()`). Lets callers (e.g. `HistoryList`'s Apply/Drop buttons)
  // disable themselves so a rapid double-click can't fire the same index-based mutation twice
  // before the first one's refresh lands — see the stash Drop race this was added for.
  pending: boolean;
}

export interface UseAppStateResult {
  state: AppState;
  openRepo(path: string): Promise<void>;
  selectRow(row: SelectedRow): void;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  commit(message: string): Promise<void>;
  createBranch(name: string, startPoint: string): Promise<void>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  saveStash(): Promise<void>;
  applyStash(index: number): Promise<void>;
  dropStash(index: number): Promise<void>;
  refresh(): Promise<void>;
}

export function useAppState(client: RepoClient): UseAppStateResult {
  const [state, setState] = useState<AppState>({
    repoPath: null,
    selectedRow: "uncommitted",
    status: [],
    log: [],
    branches: [],
    createBranchDraft: null,
    stashes: [],
    error: null,
    pending: false,
  });

  const refresh = useCallback(async () => {
    try {
      const [status, log, branches, stashes] = await Promise.all([
        client.getStatus(),
        client.getLog(LOG_LIMIT),
        client.listBranches(),
        client.listStashes(),
      ]);
      setState((prev) => ({ ...prev, status, log, branches, stashes, error: null }));
    } catch (err) {
      setState((prev) => ({ ...prev, error: String(err) }));
    }
  }, [client]);

  const runMutation = useCallback(
    async (mutate: () => Promise<void>) => {
      try {
        setState((prev) => ({ ...prev, pending: true }));
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
      } catch (err) {
        setState((prev) => ({ ...prev, error: String(err), pending: false }));
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

  const createBranch = useCallback(
    (name: string, startPoint: string) =>
      runMutation(async () => {
        await client.createBranch(name, startPoint);
        setState((prev) => ({ ...prev, createBranchDraft: null, selectedRow: "uncommitted" }));
      }),
    [client, runMutation],
  );
  const switchBranch = useCallback(
    (name: string) =>
      runMutation(async () => {
        await client.switchBranch(name);
        setState((prev) => ({ ...prev, selectedRow: "uncommitted" }));
      }),
    [client, runMutation],
  );
  const deleteBranch = useCallback(
    (name: string, force: boolean) => runMutation(() => client.deleteBranch(name, force)),
    [client, runMutation],
  );
  const renameBranch = useCallback(
    (oldName: string, newName: string) => runMutation(() => client.renameBranch(oldName, newName)),
    [client, runMutation],
  );

  const openCreateBranchDraft = useCallback((startPoint: string) => {
    setState((prev) => ({ ...prev, createBranchDraft: { startPoint } }));
  }, []);
  const closeCreateBranchDraft = useCallback(() => {
    setState((prev) => ({ ...prev, createBranchDraft: null }));
  }, []);

  const saveStash = useCallback(
    () => runMutation(() => client.saveStash()),
    [client, runMutation],
  );
  const applyStash = useCallback(
    (index: number) => runMutation(() => client.applyStash(index)),
    [client, runMutation],
  );
  const dropStash = useCallback(
    (index: number) =>
      runMutation(async () => {
        // Read the about-to-be-dropped stash's commitId before calling the client: if it's
        // the one currently selected, `DiffPane` would otherwise keep showing a diff for a
        // commit that's about to become unreachable until GC.
        const droppedCommitId = state.stashes[index]?.commitId;
        const dropsSelectedStash =
          droppedCommitId !== undefined &&
          typeof state.selectedRow === "object" &&
          state.selectedRow.commitId === droppedCommitId;
        await client.dropStash(index);
        if (dropsSelectedStash) {
          setState((prev) => ({ ...prev, selectedRow: "uncommitted" }));
        }
      }),
    [client, runMutation, state],
  );

  return {
    state,
    openRepo,
    selectRow,
    stageFile,
    unstageFile,
    commit,
    createBranch,
    switchBranch,
    deleteBranch,
    renameBranch,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    saveStash,
    applyStash,
    dropStash,
    refresh,
  };
}
