import { useCallback, useEffect, useRef, useState } from "react";
import type {
  BranchInfo,
  CreatePullRequest,
  FileConflictChoice,
  ForgeProvider,
  ForgeRepository,
  GraphCommit,
  MergeOutcome,
  PullOutcome,
  PullRequest,
  RebasePlanEntry,
  RebaseStepResult,
  RemoteAuthMode,
  RemoteInfo,
  ReflogEntry,
  RepoClient,
  StashEntry,
  StatusEntry,
  SubmoduleInfo,
  TagInfo,
  TransferProgress,
  UpstreamInfo,
  WorktreeInfo,
} from "../ipc/RepoClient";

function transferFailureMessage(progress: TransferProgress): string {
  if (progress.errorKind === "MissingCredential") return credentialFailureMessage("missing credential");
  if (progress.errorKind === "CredentialStoreFailure") return credentialFailureMessage("credential keychain failure");
  if (progress.errorKind === "SshAgentFailure") return credentialFailureMessage("SSH agent failure");
  const isPush = progress.operation === "PushBranch" || progress.operation === "PushTags";
  if (isPush && progress.errorKind === "NonFastForward") {
    return "Push was rejected because the remote has newer commits. Pull or reconcile history, then try again.";
  }
  if (isPush && progress.errorKind === "RejectedRemoteRef") {
    return "The remote rejected the pushed reference.";
  }
  if (isPush) return "Push failed";
  if (progress.operation === "Pull") return "Pull failed";
  if (progress.operation === "Fetch") return "Fetch failed";
  return "Transfer failed";
}

function credentialFailureMessage(error: unknown): string {
  const message = String(error);
  if (message.includes("missing credential")) return "Save an HTTPS token for this remote before retrying.";
  if (message.includes("credential keychain failure")) return "The operating-system credential store is unavailable. Unlock it and try again.";
  if (message.includes("SSH agent failure")) return "Load a key into your SSH agent and try again.";
  return message;
}

const GRAPH_LIMIT = 300;

export type SelectedRow = "uncommitted" | { commitId: string };

export interface AppState {
  repoPath: string | null;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  commits: GraphCommit[];
  branches: BranchInfo[];
  worktrees: WorktreeInfo[];
  submodules: SubmoduleInfo[];
  reflogRefs: string[];
  selectedReflogReference: string | null;
  reflog: ReflogEntry[];
  remotes: RemoteInfo[];
  tags: TagInfo[];
  upstream: UpstreamInfo | null;
  remoteUpstreams: Record<string, UpstreamInfo[]>;
  forgeRepositories: ForgeRepository[];
  pullRequests: PullRequest[];
  createBranchDraft: { startPoint: string } | null;
  stashes: StashEntry[];
  mergeMessage: string | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  rebaseOnto: string | null;
  pendingPull: { upstreamRef: string } | null;
  pullOutcome: PullOutcome | null;
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
  createWorktree(name: string, path: string, branch: string, startPoint: string | null): Promise<void>;
  removeWorktree(name: string): Promise<void>;
  pruneWorktrees(): Promise<void>;
  initSubmodule(path: string): Promise<void>;
  updateSubmodule(path: string, recursive: boolean): Promise<void>;
  selectReflogReference(reference: string): Promise<void>;
  restoreReflogEntry(reference: string, newId: string): Promise<void>;
  addRemote(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  renameRemote(oldName: string, newName: string): Promise<boolean>;
  updateRemoteUrls(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  removeRemote(name: string, clearUpstreams: boolean): Promise<void>;
  saveHttpsCredential(remoteName: string, username: string, token: string): Promise<void>;
  forgetHttpsCredential(remoteName: string): Promise<void>;
  setRemoteAuthMode(remoteName: string, mode: RemoteAuthMode, username: string | null): Promise<boolean>;
  setCurrentUpstream(remoteName: string, remoteBranch: string): Promise<void>;
  clearCurrentUpstream(): Promise<void>;
  fetchRemote(remoteName: string): Promise<void>;
  createTag(name: string, message: string | null): Promise<void>;
  deleteTag(name: string): Promise<void>;
  pushCurrentBranch(remoteName: string): Promise<void>;
  pushTags(remoteName: string, names: string[]): Promise<void>;
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
  listPullRequests(remoteName: string, account: string): Promise<void>;
  saveForgeToken(provider: ForgeProvider, account: string, token: string): Promise<void>;
  forgetForgeToken(provider: ForgeProvider, account: string): Promise<void>;
  createPullRequest(remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<void>;
  refresh(): Promise<void>;
}

export function useAppState(client: RepoClient): UseAppStateResult {
  const [state, setState] = useState<AppState>({
    repoPath: null,
    selectedRow: "uncommitted",
    status: [],
    commits: [],
    worktrees: [],
    submodules: [],
    reflogRefs: [],
    selectedReflogReference: null,
    reflog: [],
    branches: [],
    remotes: [],
    tags: [],
    upstream: null,
    remoteUpstreams: {},
    forgeRepositories: [],
    pullRequests: [],
    createBranchDraft: null,
    stashes: [],
    mergeMessage: null,
    rebaseProgress: null,
    rebaseOnto: null,
    pendingPull: null,
    pullOutcome: null,
    transfer: null,
    error: null,
    pending: false,
  });

  const selectedReflogReference = useRef<string | null>(null);
  const reflogRequestGeneration = useRef(0);

  const refresh = useCallback(async () => {
    try {
      const [status, commits, branches, worktrees, submodules, reflogRefs, remotes, tags, upstream, stashes, mergeMessage, rebaseProgress, forgeRepositories] =
        await Promise.all([
          client.getStatus(),
          client.getCommitGraph(GRAPH_LIMIT),
          client.listBranches(),
          client.listWorktrees(),
          client.listSubmodules(),
          client.listReflogRefs(),
          client.listRemotes(),
          client.listTags(),
          client.getCurrentUpstream(),
          client.listStashes(),
          client.getMergeMessage(),
          client.getRebaseProgress(),
          client.detectForgeRepository(),
        ]);
      const remoteUpstreams = Object.fromEntries(
        await Promise.all(
          remotes.map(async (remote) => [remote.name, await client.getRemoteUpstreams(remote.name)]),
        ),
      );
      const reference = selectedReflogReference.current;
      const selectedReference = reference !== null && reflogRefs.includes(reference)
        ? reference
        : null;
      selectedReflogReference.current = selectedReference;
      const reflog = selectedReference === null
        ? []
        : await client.getReflog(selectedReference);
      setState((prev) => ({
        ...prev,
        status,
        commits,
        branches,
        worktrees,
        submodules,
        reflogRefs,
        selectedReflogReference: selectedReference,
        reflog,
        remotes,
        tags,
        upstream,
        remoteUpstreams,
        stashes,
        mergeMessage,
        rebaseProgress,
        forgeRepositories,
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
          error: transferFailureMessage(progress),
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
        setState((prev) => ({ ...prev, error: credentialFailureMessage(err), pending: false }));
      }
    },
    [refresh],
  );

  const runMutationWithOutcome = useCallback(
    async (mutate: () => Promise<void>): Promise<boolean> => {
      try {
        setState((prev) => ({ ...prev, pending: true }));
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return true;
      } catch (err) {
        setState((prev) => ({ ...prev, error: credentialFailureMessage(err), pending: false }));
        return false;
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
      selectedReflogReference.current = null;
      setState((prev) => ({ ...prev, transfer: null }));
      return runMutation(async () => {
        await client.openRepo(path);
        setState((prev) => ({
          ...prev,
          repoPath: path,
          selectedRow: "uncommitted",
          pullOutcome: null,
        }));
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
        setState((prev) => ({ ...prev, selectedRow: "uncommitted", pullOutcome: null }));
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
  const createWorktree = useCallback(
    (name: string, path: string, branch: string, startPoint: string | null) =>
      runMutation(() => client.createWorktree(name, path, branch, startPoint)),
    [client, runMutation],
  );
  const removeWorktree = useCallback(
    (name: string) => runMutation(() => client.removeWorktree(name)),
    [client, runMutation],
  );
  const pruneWorktrees = useCallback(
    () => runMutation(() => client.pruneWorktrees()),
    [client, runMutation],
  );

  const initSubmodule = useCallback(
    (path: string) => runMutation(() => client.initSubmodule(path)),
    [client, runMutation],
  );
  const updateSubmodule = useCallback(
    (path: string, recursive: boolean) =>
      runMutation(() => client.updateSubmodule(path, recursive)),
    [client, runMutation],
  );

  const selectReflogReference = useCallback(
    async (reference: string) => {
      const requestGeneration = ++reflogRequestGeneration.current;
      try {
        selectedReflogReference.current = reference;
        const reflog = await client.getReflog(reference);
        if (
          requestGeneration !== reflogRequestGeneration.current ||
          selectedReflogReference.current !== reference
        ) {
          return;
        }
        setState((prev) => ({
          ...prev,
          selectedReflogReference: reference,
          reflog,
          error: null,
        }));
      } catch (err) {
        if (
          requestGeneration === reflogRequestGeneration.current &&
          selectedReflogReference.current === reference
        ) {
          setState((prev) => ({ ...prev, error: String(err) }));
        }
      }
    },
    [client],
  );
  const restoreReflogEntry = useCallback(
    (reference: string, newId: string) => {
      selectedReflogReference.current = reference;
      reflogRequestGeneration.current += 1;
      setState((prev) => ({ ...prev, selectedReflogReference: reference }));
      return runMutation(() => client.restoreReflogEntry(reference, newId));
    },
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
  const saveHttpsCredential = useCallback(
    (remoteName: string, username: string, token: string) =>
      runMutation(() => client.saveHttpsCredential(remoteName, username, token)),
    [client, runMutation],
  );
  const forgetHttpsCredential = useCallback(
    (remoteName: string) => runMutation(() => client.forgetHttpsCredential(remoteName)),
    [client, runMutation],
  );
  const setRemoteAuthMode = useCallback(
    (remoteName: string, mode: RemoteAuthMode, username: string | null) =>
      runMutationWithOutcome(() => client.setRemoteAuthMode(remoteName, mode, username)),
    [client, runMutationWithOutcome],
  );
  const setCurrentUpstream = useCallback(
    (remoteName: string, remoteBranch: string) =>
      runMutation(async () => {
        await client.setCurrentUpstream(remoteName, remoteBranch);
        setState((prev) => ({ ...prev, pullOutcome: null }));
      }),
    [client, runMutation],
  );
  const clearCurrentUpstream = useCallback(
    () =>
      runMutation(async () => {
        await client.clearCurrentUpstream();
        setState((prev) => ({ ...prev, pullOutcome: null }));
      }),
    [client, runMutation],
  );
  const startTransfer = useCallback(
    async (operation: TransferProgress["operation"], start: () => Promise<string>) => {
      try {
        transferRequestPending.current = true;
        activeTransferId.current = null;
        setState((prev) => ({ ...prev, pending: true, error: null }));
        const operationId = await start();
        if (transferRequestPending.current && activeTransferId.current === null) {
          activeTransferId.current = operationId;
          setState((prev) => ({
            ...prev,
            transfer: {
              operationId,
              operation,
              phase: "Starting",
              errorKind: null,
              current: 0,
              total: 0,
              receivedBytes: 0,
              message: null,
            },
          }));
        }
      } catch (err) {
        const message = String(err);
        if (operation === "Fetch" && (message === "Fetch failed" || message.endsWith(": Fetch failed"))) return;
        transferRequestPending.current = false;
        activeTransferId.current = null;
        setState((prev) => ({ ...prev, error: message, pending: false }));
      }
    },
    [],
  );

  const fetchRemote = useCallback(
    (remoteName: string) => startTransfer("Fetch", () => client.fetchRemote(remoteName)),
    [client, startTransfer],
  );

  const createTag = useCallback(
    (name: string, message: string | null) => runMutation(() => client.createTag(name, message)),
    [client, runMutation],
  );
  const deleteTag = useCallback(
    (name: string) => runMutation(() => client.deleteTag(name)),
    [client, runMutation],
  );
  const pushCurrentBranch = useCallback(
    (remoteName: string) =>
      startTransfer("PushBranch", () => client.pushCurrentBranch(remoteName)),
    [client, startTransfer],
  );
  const pushTags = useCallback(
    (remoteName: string, names: string[]) =>
      startTransfer("PushTags", () => client.pushTags(remoteName, names)),
    [client, startTransfer],
  );

  const pullCurrentUpstream = useCallback(async () => {
    try {
      transferRequestPending.current = true;
      activeTransferId.current = null;
      setState((prev) => ({
        ...prev,
        pending: true,
        error: null,
        pendingPull: null,
        pullOutcome: null,
      }));
      const outcome: PullOutcome = await client.pullCurrentUpstream();
      if (outcome.kind === "Diverged") {
        setState((prev) => ({ ...prev, pending: false, pendingPull: { upstreamRef: outcome.upstreamRef } }));
        return;
      }
      await refresh();
      setState((prev) => ({ ...prev, pending: false, pullOutcome: outcome }));
    } catch (err) {
      const message = String(err);
      if (message === "pull failed" || message.endsWith(": pull failed")) return;
      transferRequestPending.current = false;
      activeTransferId.current = null;
      setState((prev) => ({
        ...prev,
        transfer: null,
        pending: false,
        pendingPull: null,
        pullOutcome: null,
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

  const listPullRequests = useCallback(
    async (remoteName: string, account: string) => {
      try {
        const pullRequests = await client.listPullRequests(remoteName, account);
        setState((prev) => ({ ...prev, pullRequests, error: null }));
      } catch (err) {
        setState((prev) => ({ ...prev, error: String(err) }));
      }
    },
    [client],
  );
  const saveForgeToken = useCallback(
    (provider: ForgeProvider, account: string, token: string) =>
      runMutation(() => client.saveForgeToken(provider, account, token)),
    [client, runMutation],
  );
  const forgetForgeToken = useCallback(
    (provider: ForgeProvider, account: string) =>
      runMutation(() => client.forgetForgeToken(provider, account)),
    [client, runMutation],
  );
  const createPullRequest = useCallback(
    (remoteName: string, account: string, pullRequest: CreatePullRequest) =>
      runMutation(async () => {
        const created = await client.createPullRequest(remoteName, account, pullRequest);
        setState((prev) => ({ ...prev, pullRequests: [created, ...prev.pullRequests] }));
      }),
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
    createWorktree,
    removeWorktree,
    pruneWorktrees,
    initSubmodule,
    updateSubmodule,
    selectReflogReference,
    restoreReflogEntry,
    switchBranch,
    deleteBranch,
    renameBranch,
    addRemote,
    renameRemote,
    updateRemoteUrls,
    removeRemote,
    saveHttpsCredential,
    forgetHttpsCredential,
    setRemoteAuthMode,
    setCurrentUpstream,
    clearCurrentUpstream,
    fetchRemote,
    createTag,
    deleteTag,
    pushCurrentBranch,
    pushTags,
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
    listPullRequests,
    saveForgeToken,
    forgetForgeToken,
    createPullRequest,
    refresh,
  };
}
