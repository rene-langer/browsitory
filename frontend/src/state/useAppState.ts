import { useCallback, useState } from "react";
import type {
  BranchInfo,
  FileConflictChoice,
  GraphCommit,
  MergeOutcome,
  RebasePlanEntry,
  RebaseStepResult,
  RemoteInfo,
  RepoClient,
  StashEntry,
  StatusEntry,
  UpstreamInfo,
} from "../ipc/RepoClient";

const GRAPH_LIMIT = 300;

export type SelectedRow = "uncommitted" | { commitId: string };

export interface AppState {
  repoPath: string | null;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  commits: GraphCommit[];
  branches: BranchInfo[];
  remotes: RemoteInfo[];
  upstream: UpstreamInfo | null;
  createBranchDraft: { startPoint: string } | null;
  stashes: StashEntry[];
  mergeMessage: string | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  rebaseOnto: string | null;
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
  addRemote(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  renameRemote(oldName: string, newName: string): Promise<void>;
  updateRemoteUrls(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  removeRemote(name: string): Promise<void>;
  setCurrentUpstream(remoteName: string, remoteBranch: string): Promise<void>;
  clearCurrentUpstream(): Promise<void>;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  saveStash(): Promise<void>;
  applyStash(index: number): Promise<void>;
  dropStash(index: number): Promise<void>;
  mergeBranch(branchName: string): Promise<void>;
  resolveConflict(path: string, resolvedContent: string): Promise<void>;
  resolveAddDeleteConflict(path: string, choice: FileConflictChoice): Promise<void>;
  abortMerge(): Promise<void>;
  openRebasePlanner(commitId: string): void;
  closeRebasePlanner(): void;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<void>;
  rebaseContinue(): Promise<void>;
  abortRebase(): Promise<void>;
  refresh(): Promise<void>;
}

export function useAppState(client: RepoClient): UseAppStateResult {
  const [state, setState] = useState<AppState>({
    repoPath: null,
    selectedRow: "uncommitted",
    status: [],
    commits: [],
    branches: [],
    remotes: [],
    upstream: null,
    createBranchDraft: null,
    stashes: [],
    mergeMessage: null,
    rebaseProgress: null,
    rebaseOnto: null,
    error: null,
    pending: false,
  });

  const refresh = useCallback(async () => {
    try {
      const [status, commits, branches, remotes, upstream, stashes, mergeMessage, rebaseProgress] =
        await Promise.all([
          client.getStatus(),
          client.getCommitGraph(GRAPH_LIMIT),
          client.listBranches(),
          client.listRemotes(),
          client.getCurrentUpstream(),
          client.listStashes(),
          client.getMergeMessage(),
          client.getRebaseProgress(),
        ]);
      setState((prev) => ({
        ...prev,
        status,
        commits,
        branches,
        remotes,
        upstream,
        stashes,
        mergeMessage,
        rebaseProgress,
        error: null,
      }));
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
  const addRemote = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) => runMutation(() => client.addRemote(name, fetchUrl, pushUrl)),
    [client, runMutation],
  );
  const renameRemote = useCallback(
    (oldName: string, newName: string) => runMutation(() => client.renameRemote(oldName, newName)),
    [client, runMutation],
  );
  const updateRemoteUrls = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) => runMutation(() => client.updateRemoteUrls(name, fetchUrl, pushUrl)),
    [client, runMutation],
  );
  const removeRemote = useCallback(
    (name: string) => runMutation(() => client.removeRemote(name)),
    [client, runMutation],
  );
  const setCurrentUpstream = useCallback(
    (remoteName: string, remoteBranch: string) => runMutation(() => client.setCurrentUpstream(remoteName, remoteBranch)),
    [client, runMutation],
  );
  const clearCurrentUpstream = useCallback(
    () => runMutation(() => client.clearCurrentUpstream()),
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

  const mergeBranch = useCallback(
    (branchName: string): Promise<void> =>
      runMutation(async () => {
        const outcome: MergeOutcome = await client.mergeBranch(branchName);
        void outcome;
      }),
    [client, runMutation],
  );
  const resolveConflict = useCallback(
    (path: string, resolvedContent: string) =>
      runMutation(() => client.resolveConflict(path, resolvedContent)),
    [client, runMutation],
  );
  const resolveAddDeleteConflict = useCallback(
    (path: string, choice: FileConflictChoice) =>
      runMutation(() => client.resolveAddDeleteConflict(path, choice)),
    [client, runMutation],
  );
  const abortMerge = useCallback(
    () => runMutation(() => client.abortMerge()),
    [client, runMutation],
  );

  const openRebasePlanner = useCallback((commitId: string) => {
    setState((prev) => ({ ...prev, rebaseOnto: commitId }));
  }, []);
  const closeRebasePlanner = useCallback(() => {
    setState((prev) => ({ ...prev, rebaseOnto: null }));
  }, []);

  const startRebase = useCallback(
    (onto: string, plan: RebasePlanEntry[]): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.startRebase(onto, plan);
        void result;
        setState((prev) => ({ ...prev, rebaseOnto: null }));
      }),
    [client, runMutation],
  );
  const rebaseContinue = useCallback(
    (): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.rebaseContinue();
        void result;
      }),
    [client, runMutation],
  );
  const abortRebase = useCallback(
    () => runMutation(() => client.abortRebase()),
    [client, runMutation],
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
    addRemote,
    renameRemote,
    updateRemoteUrls,
    removeRemote,
    setCurrentUpstream,
    clearCurrentUpstream,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    saveStash,
    applyStash,
    dropStash,
    mergeBranch,
    resolveConflict,
    resolveAddDeleteConflict,
    abortMerge,
    openRebasePlanner,
    closeRebasePlanner,
    startRebase,
    rebaseContinue,
    abortRebase,
    refresh,
  };
}
