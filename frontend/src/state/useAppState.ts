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
  PullRequestList,
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
import { credentialFailureMessage, useMutationRunner } from "./useMutationRunner";

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

const GRAPH_LIMIT = 300;

export type SelectedRow = "uncommitted" | { commitId: string };

export interface AppState {
  repoPath: string;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  commits: GraphCommit[];
  // `null` means "no filter saved" — every local branch is walked (see `graph_log` in
  // `git-core`). Non-null is the persisted subset CommitGraph currently shows.
  graphBranchSelection: string[] | null;
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
  // Keyed by `remoteName` (see `PullRequestPanel.tsx`'s `ForgeRepositorySection`, one per
  // remote) rather than a single flat list shared by every remote — otherwise, listing/creating
  // against one remote clobbers or hides another remote's already-listed rows. A remote absent
  // from this record simply hasn't been listed yet (or its listing failed — see
  // `listPullRequests` below, which removes the key on failure rather than leaving stale rows).
  pullRequests: Record<string, PullRequestList>;
  createBranchDraft: { startPoint: string } | null;
  stashes: StashEntry[];
  mergeMessage: string | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  rebaseOnto: string | null;
  // Commit ids to default to "Squash" in the rebase planner, set only when it was opened via
  // CommitGraph's "Squash N commits" action (as opposed to "Rebase onto here").
  squashPreset: Set<string> | null;
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
  // Resolves to `null` on success, or the failure message on failure — mirrors `addRemote`
  // below. Naming a new branch is the one create-form action here with an obvious single trigger
  // point (the "New Branch…" draft form), so its failure surfaces next to that form instead of
  // the shared banner (issue #30/UX-002).
  createBranch(name: string, startPoint: string): Promise<string | null>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  // `Promise<string | null>` for the same reason as `createBranch` above — the "Create worktree"
  // form is its own natural trigger point.
  createWorktree(name: string, path: string, branch: string, startPoint: string | null): Promise<string | null>;
  removeWorktree(name: string): Promise<void>;
  pruneWorktrees(): Promise<void>;
  initSubmodule(path: string): Promise<void>;
  updateSubmodule(path: string, recursive: boolean): Promise<void>;
  selectReflogReference(reference: string): Promise<void>;
  restoreReflogEntry(reference: string, newId: string): Promise<void>;
  // Resolves to `null` on success, or the failure message on failure — `RemotePanel` renders
  // that message inline next to the Fetch URL field and keeps the typed values. A plain
  // `runMutation` can't serve that: it swallows the error into `state.error` and *always*
  // resolves, so a `try`/`catch` around this call in the panel would be dead code and its
  // success path would wipe the user's input on a failed add. Returning the message (rather
  // than `renameRemote`'s `boolean`) keeps the panel off global `state.error`, which isn't
  // scoped to this action and can hold a stale message from an unrelated mutation.
  addRemote(name: string, fetchUrl: string, pushUrl: string | null): Promise<string | null>;
  renameRemote(oldName: string, newName: string): Promise<boolean>;
  updateRemoteUrls(name: string, fetchUrl: string, pushUrl: string | null): Promise<void>;
  removeRemote(name: string, clearUpstreams: boolean): Promise<void>;
  saveHttpsCredential(remoteName: string, username: string, token: string): Promise<void>;
  forgetHttpsCredential(remoteName: string): Promise<void>;
  setRemoteAuthMode(remoteName: string, mode: RemoteAuthMode, username: string | null): Promise<boolean>;
  setCurrentUpstream(remoteName: string, remoteBranch: string): Promise<void>;
  clearCurrentUpstream(): Promise<void>;
  listRemoteBranches(remoteName: string): Promise<string[]>;
  fetchRemote(remoteName: string): Promise<void>;
  // `Promise<string | null>` for the same reason as `createBranch`/`createWorktree` above.
  createTag(name: string, message: string | null): Promise<string | null>;
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
  openSquashPlanner(ontoId: string, squashIds: string[]): void;
  closeRebasePlanner(): void;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<void>;
  rebaseContinue(): Promise<void>;
  abortRebase(): Promise<void>;
  listPullRequests(remoteName: string, account: string): Promise<void>;
  saveForgeToken(provider: ForgeProvider, account: string, token: string): Promise<void>;
  forgetForgeToken(provider: ForgeProvider, account: string): Promise<void>;
  // `Promise<boolean>` (unlike the other forge actions above) so `PullRequestPanel` can tell a
  // failed creation from a successful one and only clear its form on success — `runMutation`
  // swallows the underlying error into `state.error` rather than rejecting, so the boolean is
  // the only success/failure signal available to the caller. Matches `renameRemote`/
  // `setRemoteAuthMode`'s existing `Promise<boolean>` pattern below.
  createPullRequest(remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<boolean>;
  openExternalUrl(url: string): Promise<void>;
  setGraphBranchSelection(selectedBranches: string[]): Promise<void>;
  refresh(): Promise<void>;
  // Clears `state.error` without waiting for the next successful action of the same kind — the
  // global banner's dismiss control (issue #30/UX-002). See `App.tsx`'s `RepoWorkspace`.
  dismissError(): void;
}

export function useAppState(client: RepoClient, repoPath: string): UseAppStateResult {
  const [state, setState] = useState<AppState>({
    repoPath,
    selectedRow: "uncommitted",
    status: [],
    commits: [],
    graphBranchSelection: null,
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
    pullRequests: {},
    createBranchDraft: null,
    stashes: [],
    mergeMessage: null,
    rebaseProgress: null,
    rebaseOnto: null,
    squashPreset: null,
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
      const graphBranchSelection = await client.getGraphBranchSelection(repoPath);
      const [status, commits, branches, worktrees, submodules, reflogRefs, remotes, tags, upstream, stashes, mergeMessage, rebaseProgress, forgeRepositories] =
        await Promise.all([
          client.getStatus(repoPath),
          client.getCommitGraph(repoPath, GRAPH_LIMIT, graphBranchSelection),
          client.listBranches(repoPath),
          client.listWorktrees(repoPath),
          client.listSubmodules(repoPath),
          client.listReflogRefs(repoPath),
          client.listRemotes(repoPath),
          client.listTags(repoPath),
          client.getCurrentUpstream(repoPath),
          client.listStashes(repoPath),
          client.getMergeMessage(repoPath),
          client.getRebaseProgress(repoPath),
          client.detectForgeRepository(repoPath),
        ]);
      const remoteUpstreams = Object.fromEntries(
        await Promise.all(
          remotes.map(async (remote) => [remote.name, await client.getRemoteUpstreams(repoPath, remote.name)]),
        ),
      );
      const reference = selectedReflogReference.current;
      const selectedReference = reference !== null && reflogRefs.includes(reference)
        ? reference
        : null;
      selectedReflogReference.current = selectedReference;
      const reflog = selectedReference === null
        ? []
        : await client.getReflog(repoPath, selectedReference);
      setState((prev) => ({
        ...prev,
        status,
        commits,
        graphBranchSelection,
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
  }, [client, repoPath]);

  const activeTransferId = useRef<string | null>(null);
  const transferRequestPending = useRef(false);

  useEffect(() => {
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
  }, [client, refresh, repoPath]);

  const { runMutation, runMutationWithOutcome, runMutationWithMessage } = useMutationRunner(refresh, setState);

  const selectRow = useCallback((row: SelectedRow) => {
    setState((prev) => ({ ...prev, selectedRow: row }));
  }, []);

  const stageFile = useCallback(
    (path: string) => runMutation(() => client.stageFile(repoPath, path)),
    [client, runMutation, repoPath],
  );
  const unstageFile = useCallback(
    (path: string) => runMutation(() => client.unstageFile(repoPath, path)),
    [client, runMutation, repoPath],
  );
  // See the `stageAllFiles`/`unstageAllFiles` note in `UseAppStateResult`: one `runMutation`
  // for the whole batch, not one per path. Sequential rather than `Promise.all` because every
  // call lands on the same worker thread anyway, and index writes must not interleave.
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

  const createBranch = useCallback(
    (name: string, startPoint: string) =>
      runMutationWithMessage(async () => {
        await client.createBranch(repoPath, name, startPoint);
        setState((prev) => ({ ...prev, createBranchDraft: null, selectedRow: "uncommitted" }));
      }),
    [client, runMutationWithMessage, repoPath],
  );
  const switchBranch = useCallback(
    (name: string) =>
      runMutation(async () => {
        await client.switchBranch(repoPath, name);
        setState((prev) => ({ ...prev, selectedRow: "uncommitted", pullOutcome: null }));
      }),
    [client, runMutation, repoPath],
  );
  const deleteBranch = useCallback(
    (name: string, force: boolean) => runMutation(() => client.deleteBranch(repoPath, name, force)),
    [client, runMutation, repoPath],
  );
  const renameBranch = useCallback(
    (oldName: string, newName: string) => runMutation(() => client.renameBranch(repoPath, oldName, newName)),
    [client, runMutation, repoPath],
  );
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

  const initSubmodule = useCallback(
    (path: string) => runMutation(() => client.initSubmodule(repoPath, path)),
    [client, runMutation, repoPath],
  );
  const updateSubmodule = useCallback(
    (path: string, recursive: boolean) =>
      runMutation(() => client.updateSubmodule(repoPath, path, recursive)),
    [client, runMutation, repoPath],
  );

  const selectReflogReference = useCallback(
    async (reference: string) => {
      const requestGeneration = ++reflogRequestGeneration.current;
      try {
        selectedReflogReference.current = reference;
        const reflog = await client.getReflog(repoPath, reference);
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
    [client, repoPath],
  );
  const restoreReflogEntry = useCallback(
    (reference: string, newId: string) => {
      selectedReflogReference.current = reference;
      reflogRequestGeneration.current += 1;
      setState((prev) => ({ ...prev, selectedReflogReference: reference }));
      return runMutation(() => client.restoreReflogEntry(repoPath, reference, newId));
    },
    [client, runMutation, repoPath],
  );

  const addRemote = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) =>
      runMutationWithMessage(() => client.addRemote(repoPath, name, fetchUrl, pushUrl)),
    [client, runMutationWithMessage, repoPath],
  );
  const renameRemote = useCallback(
    async (oldName: string, newName: string) => {
      let renamed = false;
      await runMutation(async () => {
        await client.renameRemote(repoPath, oldName, newName);
        renamed = true;
      });
      return renamed;
    },
    [client, runMutation, repoPath],
  );
  const updateRemoteUrls = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) => runMutation(() => client.updateRemoteUrls(repoPath, name, fetchUrl, pushUrl)),
    [client, runMutation, repoPath],
  );
  const removeRemote = useCallback(
    (name: string, clearUpstreams: boolean) => runMutation(() => client.removeRemote(repoPath, name, clearUpstreams)),
    [client, runMutation, repoPath],
  );
  const saveHttpsCredential = useCallback(
    (remoteName: string, username: string, token: string) =>
      runMutation(() => client.saveHttpsCredential(repoPath, remoteName, username, token)),
    [client, runMutation, repoPath],
  );
  const forgetHttpsCredential = useCallback(
    (remoteName: string) => runMutation(() => client.forgetHttpsCredential(repoPath, remoteName)),
    [client, runMutation, repoPath],
  );
  const setRemoteAuthMode = useCallback(
    (remoteName: string, mode: RemoteAuthMode, username: string | null) =>
      runMutationWithOutcome(() => client.setRemoteAuthMode(repoPath, remoteName, mode, username)),
    [client, runMutationWithOutcome, repoPath],
  );
  const setCurrentUpstream = useCallback(
    (remoteName: string, remoteBranch: string) =>
      runMutation(async () => {
        await client.setCurrentUpstream(repoPath, remoteName, remoteBranch);
        setState((prev) => ({ ...prev, pullOutcome: null }));
      }),
    [client, runMutation, repoPath],
  );
  const clearCurrentUpstream = useCallback(
    () =>
      runMutation(async () => {
        await client.clearCurrentUpstream(repoPath);
        setState((prev) => ({ ...prev, pullOutcome: null }));
      }),
    [client, runMutation, repoPath],
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
    (remoteName: string) => startTransfer("Fetch", () => client.fetchRemote(repoPath, remoteName)),
    [client, startTransfer, repoPath],
  );
  const listRemoteBranches = useCallback(
    (remoteName: string) => client.listRemoteBranches(repoPath, remoteName),
    [client, repoPath],
  );

  const createTag = useCallback(
    (name: string, message: string | null) => runMutationWithMessage(() => client.createTag(repoPath, name, message)),
    [client, runMutationWithMessage, repoPath],
  );
  const deleteTag = useCallback(
    (name: string) => runMutation(() => client.deleteTag(repoPath, name)),
    [client, runMutation, repoPath],
  );
  const pushCurrentBranch = useCallback(
    (remoteName: string) =>
      startTransfer("PushBranch", () => client.pushCurrentBranch(repoPath, remoteName)),
    [client, startTransfer, repoPath],
  );
  const pushTags = useCallback(
    (remoteName: string, names: string[]) =>
      startTransfer("PushTags", () => client.pushTags(repoPath, remoteName, names)),
    [client, startTransfer, repoPath],
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
      const outcome: PullOutcome = await client.pullCurrentUpstream(repoPath);
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
  }, [client, refresh, repoPath]);
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
    () => runMutation(() => client.saveStash(repoPath)),
    [client, runMutation, repoPath],
  );
  const applyStash = useCallback(
    (index: number) => runMutation(() => client.applyStash(repoPath, index)),
    [client, runMutation, repoPath],
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
        await client.dropStash(repoPath, index);
        if (dropsSelectedStash) {
          setState((prev) => ({ ...prev, selectedRow: "uncommitted" }));
        }
      }),
    [client, runMutation, state, repoPath],
  );

  const mergeBranch = useCallback(
    (branchName: string): Promise<void> =>
      runMutation(async () => {
        const outcome: MergeOutcome = await client.mergeBranch(repoPath, branchName);
        void outcome;
      }),
    [client, runMutation, repoPath],
  );
  const resolveConflict = useCallback(
    (path: string, resolvedContent: string) =>
      runMutation(() => client.resolveConflict(repoPath, path, resolvedContent)),
    [client, runMutation, repoPath],
  );
  const resolveAddDeleteConflict = useCallback(
    (path: string, choice: FileConflictChoice) =>
      runMutation(() => client.resolveAddDeleteConflict(repoPath, path, choice)),
    [client, runMutation, repoPath],
  );
  const abortMerge = useCallback(
    () => runMutation(() => client.abortMerge(repoPath)),
    [client, runMutation, repoPath],
  );

  const openRebasePlanner = useCallback((commitId: string) => {
    setState((prev) => ({ ...prev, rebaseOnto: commitId, squashPreset: null }));
  }, []);
  const openSquashPlanner = useCallback((ontoId: string, squashIds: string[]) => {
    setState((prev) => ({ ...prev, rebaseOnto: ontoId, squashPreset: new Set(squashIds) }));
  }, []);
  const closeRebasePlanner = useCallback(() => {
    setState((prev) => ({ ...prev, rebaseOnto: null, squashPreset: null }));
  }, []);

  const startRebase = useCallback(
    (onto: string, plan: RebasePlanEntry[]): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.startRebase(repoPath, onto, plan);
        void result;
        setState((prev) => ({ ...prev, rebaseOnto: null, squashPreset: null }));
      }),
    [client, runMutation, repoPath],
  );
  const rebaseContinue = useCallback(
    (): Promise<void> =>
      runMutation(async () => {
        const result: RebaseStepResult = await client.rebaseContinue(repoPath);
        void result;
      }),
    [client, runMutation, repoPath],
  );
  const abortRebase = useCallback(
    () => runMutation(() => client.abortRebase(repoPath)),
    [client, runMutation, repoPath],
  );

  const listPullRequests = useCallback(
    async (remoteName: string, account: string) => {
      try {
        const result = await client.listPullRequests(repoPath, remoteName, account);
        setState((prev) => ({
          ...prev,
          pullRequests: { ...prev.pullRequests, [remoteName]: result },
          error: null,
        }));
      } catch (err) {
        // Drop this remote's entry (rather than leaving whatever was there before) so a failed
        // re-list can't leave stale, possibly-successful-looking rows on screen under this
        // remote's heading. Other remotes' entries are untouched — a failure listing remote B
        // must never affect what's shown for remote A.
        setState((prev) => {
          const rest = { ...prev.pullRequests };
          delete rest[remoteName];
          return { ...prev, pullRequests: rest, error: String(err) };
        });
      }
    },
    [client, repoPath],
  );
  const saveForgeToken = useCallback(
    (provider: ForgeProvider, account: string, token: string) =>
      runMutation(() => client.saveForgeToken(repoPath, provider, account, token)),
    [client, runMutation, repoPath],
  );
  const forgetForgeToken = useCallback(
    (provider: ForgeProvider, account: string) =>
      runMutation(() => client.forgetForgeToken(repoPath, provider, account)),
    [client, runMutation, repoPath],
  );
  const createPullRequest = useCallback(
    (remoteName: string, account: string, pullRequest: CreatePullRequest): Promise<boolean> =>
      runMutationWithOutcome(async () => {
        const created = await client.createPullRequest(repoPath, remoteName, account, pullRequest);
        setState((prev) => {
          const existing = prev.pullRequests[remoteName];
          const updated: PullRequestList = {
            pullRequests: [created, ...(existing?.pullRequests ?? [])],
            truncated: existing?.truncated ?? false,
          };
          return {
            ...prev,
            pullRequests: { ...prev.pullRequests, [remoteName]: updated },
          };
        });
      }),
    [client, runMutationWithOutcome, repoPath],
  );
  const openExternalUrl = useCallback(
    (url: string) => client.openExternalUrl(url),
    [client],
  );
  const setGraphBranchSelection = useCallback(
    (selectedBranches: string[]) =>
      runMutation(() => client.setGraphBranchSelection(repoPath, selectedBranches)),
    [client, runMutation, repoPath],
  );

  const dismissError = useCallback(() => {
    setState((prev) => ({ ...prev, error: null }));
  }, []);

  return {
    state,
    selectRow,
    stageFile,
    unstageFile,
    stageAllFiles,
    unstageAllFiles,
    stageHunk,
    unstageHunk,
    discardHunk,
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
    listRemoteBranches,
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
    openSquashPlanner,
    closeRebasePlanner,
    startRebase,
    rebaseContinue,
    abortRebase,
    listPullRequests,
    saveForgeToken,
    forgetForgeToken,
    createPullRequest,
    openExternalUrl,
    setGraphBranchSelection,
    refresh,
    dismissError,
  };
}
