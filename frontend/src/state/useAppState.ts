import { useCallback, useEffect, useRef, useState } from "react";
import type {
  BranchInfo,
  FileConflictChoice,
  GraphCommit,
  MergeOutcome,
  PullOutcome,
  RebasePlanEntry,
  RebaseStepResult,
  RemoteInfo,
  RepoClient,
  StashEntry,
  StatusEntry,
  TransferProgress,
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
  remoteUpstreams: Record<string, UpstreamInfo[]>;
  createBranchDraft: { startPoint: string } | null;
  stashes: StashEntry[];
  mergeMessage: string | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  rebaseOnto: string | null;
  pendingPull: { upstreamRef: string } | null;
  transfer: TransferProgress | null;
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
  renameRemote(oldName: string, newName: string): Promise<boolean>;
  updateRemoteUrls(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  removeRemote(name: string, clearUpstreams: boolean): Promise<void>;
  setCurrentUpstream(remoteName: string, remoteBranch: string): Promise<void>;
  clearCurrentUpstream(): Promise<void>;
  fetchRemote(remoteName: string): Promise<void>;
  pullCurrentUpstream(): Promise<void>;
  clearPendingPull(): void;
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
    remoteUpstreams: {},
    createBranchDraft: null,
    stashes: [],
    mergeMessage: null,
    rebaseProgress: null,
    rebaseOnto: null,
    pendingPull: null,
    transfer: null,
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
      const remoteUpstreams = Object.fromEntries(
        await Promise.all(
          remotes.map(async (remote) => [remote.name, await client.getRemoteUpstreams(remote.name)]),
        ),
      );
      setState((prev) => ({
        ...prev,
        status,
        commits,
        branches,
        remotes,
        upstream,
        remoteUpstreams,
        stashes,
        mergeMessage,
        rebaseProgress,
        error: null,
      }));
    } catch (err) {
      setState((prev) => ({ ...prev, error: String(err) }));
    }
  }, [client]);

  const activeTransferId = useRef<string | null>(null);
  const transferRequestPending = useRef(false);

  useEffect(() => {
    if (state.repoPath === null) return;

    return client.subscribeTransferProgress((progress) => {
      if (progress.phase === "Starting") {
        if (!transferRequestPending.current || activeTransferId.current !== null) return;
        activeTransferId.current = progress.operationId;
        setState((prev) => ({ ...prev, transfer: progress }));
        return;
      }

      if (progress.operationId !== activeTransferId.current) return;

      if (progress.phase === "Completed") {
        transferRequestPending.current = false;
        void refresh().finally(() => {
          if (activeTransferId.current !== progress.operationId) return;
          activeTransferId.current = null;
          setState((prev) => ({ ...prev, transfer: null, pending: false }));
        });
        return;
      }

      if (progress.phase === "Failed") {
        transferRequestPending.current = false;
        activeTransferId.current = null;
        setState((prev) => ({
          ...prev,
          transfer: null,
          error: "Fetch failed",
          pending: false,
        }));
        return;
      }

      setState((prev) => ({ ...prev, transfer: progress }));
    });
  }, [client, refresh, state.repoPath]);

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
    (path: string) => {
      // Invalidate a former repository's transfer before opening the replacement. A completion
      // event can arrive while the worker is changing repositories; it must never refresh or
      // clear pending state for the newly opened repository.
      activeTransferId.current = null;
      transferRequestPending.current = false;
      setState((prev) => ({ ...prev, transfer: null }));
      return runMutation(async () => {
        await client.openRepo(path);
        setState((prev) => ({ ...prev, repoPath: path, selectedRow: "uncommitted" }));
      });
    },
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
    async (oldName: string, newName: string) => {
      let renamed = false;
      await runMutation(async () => {
        await client.renameRemote(oldName, newName);
        renamed = true;
      });
      return renamed;
    },
    [client, runMutation],
  );
  const updateRemoteUrls = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) => runMutation(() => client.updateRemoteUrls(name, fetchUrl, pushUrl)),
    [client, runMutation],
  );
  const removeRemote = useCallback(
    (name: string, clearUpstreams: boolean) => runMutation(() => client.removeRemote(name, clearUpstreams)),
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
  const fetchRemote = useCallback(
    async (remoteName: string) => {
      try {
        transferRequestPending.current = true;
        activeTransferId.current = null;
        setState((prev) => ({ ...prev, pending: true, error: null }));
        const operationId = await client.fetchRemote(remoteName);
        if (transferRequestPending.current && activeTransferId.current === null) {
          activeTransferId.current = operationId;
          setState((prev) => ({
            ...prev,
            transfer: {
              operationId,
              phase: "Starting",
              current: 0,
              total: 0,
              receivedBytes: 0,
              message: null,
            },
          }));
        }
      } catch (err) {
        transferRequestPending.current = false;
        activeTransferId.current = null;
        setState((prev) => ({ ...prev, error: String(err), pending: false }));
      }
    },
    [client],
  );

  const pullCurrentUpstream = useCallback(async () => {
    try {
      transferRequestPending.current = true;
      activeTransferId.current = null;
      setState((prev) => ({ ...prev, pending: true, error: null, pendingPull: null }));
      const outcome: PullOutcome = await client.pullCurrentUpstream();
      if (outcome.kind === "Diverged") {
        setState((prev) => ({ ...prev, pending: false, pendingPull: { upstreamRef: outcome.upstreamRef } }));
        return;
      }
      await refresh();
      setState((prev) => ({ ...prev, pending: false }));
    } catch (err) {
      const message = String(err);
      setState((prev) => ({
        ...prev,
        pending: false,
        pendingPull: null,
        error: message.includes("cannot pull with a dirty worktree")
          ? "Commit or stash your changes before pulling."
          : message,
      }));
    }
  }, [client, refresh]);
  const clearPendingPull = useCallback(() => {
    setState((prev) => ({ ...prev, pendingPull: null }));
  }, []);

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
    fetchRemote,
    pullCurrentUpstream,
    clearPendingPull,
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
