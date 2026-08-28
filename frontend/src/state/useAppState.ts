import { useCallback, useRef, useState } from "react";
import type {
  BranchInfo,
  CreatePullRequest,
  FileConflictChoice,
  ForgeProvider,
  ForgeRepository,
  GraphCommit,
  PullOutcome,
  PullRequestList,
  RebasePlanEntry,
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
import { useMutationRunner } from "./useMutationRunner";
import { useBranchActions } from "./useBranchActions";
import { useForgeActions } from "./useForgeActions";
import { useMergeRebaseActions } from "./useMergeRebaseActions";
import { useReflogActions } from "./useReflogActions";
import { useRemoteTransferActions } from "./useRemoteTransferActions";
import { useStagingActions } from "./useStagingActions";
import { useStashActions } from "./useStashActions";
import { useSubmoduleActions } from "./useSubmoduleActions";
import { useWorktreeActions } from "./useWorktreeActions";

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
  // Mirrors createBranchDraft: lets both BranchTree's own "+" button and the command palette's
  // "Add remote" entry open the same inline add-remote form.
  addRemoteDraftOpen: boolean;
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
  openAddRemoteDraft(): void;
  closeAddRemoteDraft(): void;
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
    addRemoteDraftOpen: false,
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

  const {
    runMutation,
    runMutationWithOutcome,
    runMutationWithMessage,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    runOptimisticMutationWithOutcome,
  } = useMutationRunner(refresh, setState);

  const {
    selectRow,
    stageFile,
    unstageFile,
    stageAllFiles,
    unstageAllFiles,
    stageHunk,
    unstageHunk,
    discardHunk,
    commit,
  } = useStagingActions(client, repoPath, runMutation, runOptimisticMutation, setState);

  const {
    createBranch,
    switchBranch,
    deleteBranch,
    renameBranch,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    setGraphBranchSelection,
  } = useBranchActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    setState,
  );
  const { createWorktree, removeWorktree, pruneWorktrees } = useWorktreeActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
  );

  const { initSubmodule, updateSubmodule } = useSubmoduleActions(client, repoPath, runMutation);

  const { selectReflogReference, restoreReflogEntry } = useReflogActions(
    client,
    repoPath,
    runMutation,
    setState,
    selectedReflogReference,
    reflogRequestGeneration,
  );

  const {
    addRemote,
    renameRemote,
    updateRemoteUrls,
    removeRemote,
    saveHttpsCredential,
    forgetHttpsCredential,
    setRemoteAuthMode,
    setCurrentUpstream,
    clearCurrentUpstream,
    listRemoteBranches,
    fetchRemote,
    createTag,
    deleteTag,
    pushCurrentBranch,
    pushTags,
    pullCurrentUpstream,
    clearPendingPull,
    openAddRemoteDraft,
    closeAddRemoteDraft,
  } = useRemoteTransferActions(
    client,
    repoPath,
    refresh,
    runMutation,
    runMutationWithMessage,
    runMutationWithOutcome,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    runOptimisticMutationWithOutcome,
    setState,
  );

  const { saveStash, applyStash, dropStash } = useStashActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
  );

  const {
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
  } = useMergeRebaseActions(client, repoPath, runMutation, setState);

  const { listPullRequests, saveForgeToken, forgetForgeToken, createPullRequest, openExternalUrl } =
    useForgeActions(client, repoPath, runMutation, runMutationWithOutcome, setState);
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
    openAddRemoteDraft,
    closeAddRemoteDraft,
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
