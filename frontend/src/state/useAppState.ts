import { useCallback, useMemo, useRef, useState } from "react";
import type {
  AmendOptions,
  Trailer,
  BranchInfo,
  ConflictSide,
  PickOutcome,
  SequenceAbort,
  RebaseAbort,
  SequenceOutcome,
  CreatePullRequest,
  FileConflictChoice,
  MarkResolvedOutcome,
  ForgeProvider,
  ForgeRepository,
  GraphCommit,
  MergeOptions,
  MergeOutcome,
  PullOutcome,
  StashApplyOutcome,
  StashOptions,
  PullRequestList,
  RebasePlanEntry,
  RemoteAuthMode,
  RemoteInfo,
  ReflogEntry,
  HeadInfo,
  RepoClient,
  ResetMode,
  StashEntry,
  StatusEntry,
  SubmoduleInfo,
  TagInfo,
  TransferProgress,
  UndoOutcome,
  UndoState,
  UpstreamInfo,
  WorktreeInfo,
} from "../ipc/RepoClient";
import { credentialFailureMessage, useMutationRunner } from "./useMutationRunner";
import { errorAfterOwnSuccess, forgettingStaleOwnErrors, useOwnError } from "./ownError";
import { readFetchPrune } from "../lib/fetchPrune";
import { useBranchActions } from "./useBranchActions";
import { useForgeActions } from "./useForgeActions";
import { useHistoryActions } from "./useHistoryActions";
import { useHookActions } from "./useHookActions";
import type { HookFailure, HookRetry } from "../lib/hookFailure";
import { useUndoActions } from "./useUndoActions";
import { useMergeRebaseActions } from "./useMergeRebaseActions";
import { useReflogActions } from "./useReflogActions";
import { useRemoteTransferActions } from "./useRemoteTransferActions";
import { useStagingActions } from "./useStagingActions";
import type { PresetAction } from "../lib/rebaseSelection";
import { useStashActions } from "./useStashActions";
import { useSubmoduleActions } from "./useSubmoduleActions";
import { useWorktreeActions } from "./useWorktreeActions";

// History is loaded a page at a time; `loadMoreHistory` grows the limit by one page.
const GRAPH_PAGE_SIZE = 300;

export type SelectedRow = "uncommitted" | { commitId: string };

/** A remote's Edit / Credentials / Remove dialog, opened from its menu or the palette. */
export type RemoteDialog = { kind: "edit" | "credentials" | "remove"; remoteName: string };

/** Whether two selections name the same row. */
export function rowsEqual(a: SelectedRow, b: SelectedRow): boolean {
  if (a === "uncommitted" || b === "uncommitted") {
    return a === b;
  }
  return a.commitId === b.commitId;
}

export interface AppState {
  repoPath: string;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  commits: GraphCommit[];
  // Limit the current `commits` were fetched with, and whether the graph filled it (so older
  // history probably exists beyond what is shown).
  graphLimit: number;
  hasMoreHistory: boolean;
  // `null` means "no filter saved" — every local branch is walked (see `graph_log` in
  // `git-core`). Non-null is the persisted subset CommitGraph currently shows.
  graphBranchSelection: string[] | null;
  // Independent of graphBranchSelection: this one never affects which commits `graph_log`
  // walks, only which remote-branch badges CommitGraph renders. `null` means "show all".
  graphRemoteBranchSelection: string[] | null;
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
  // Non-null while a remote's Edit / Credentials / Remove dialog is open — in app state (not
  // BranchTree's) so the palette can open it while the Branches section is collapsed.
  remoteDialog: RemoteDialog | null;
  stashes: StashEntry[];
  mergeMessage: string | null;
  headInfo: HeadInfo;
  // Set when a branch push fails NonFastForward; shows the "Force push (with lease)" button.
  forcePushOffer: { remoteName: string } | null;
  // Non-null while the force-push confirm dialog is open.
  forcePushConfirm: { remoteName: string } | null;
  // The Prune choice for every fetch in this repo (localStorage, see `lib/fetchPrune.ts`);
  // `null` = never set, so git config decides per remote.
  fetchPrune: boolean | null;
  // Non-null while the "Push to…" dialog is open for that local branch.
  pushDialog: { localBranch: string } | null;
  // Non-null while the delete-remote-branch dialog is open; both null = palette picker mode.
  deleteRemoteBranchDialog: { remoteName: string | null; branchName: string | null } | null;
  // CommitBox amend toggle.
  amendMode: boolean;
  // CommitBox "Skip hooks" (`--no-verify`) for the next commit/amend; reset after one lands.
  skipHooks: boolean;
  // The commit/amend hook running right now (from `subscribeHookProgress`), for its status line
  // and Cancel button. Pushes show their `pre-push` through `transfer` instead.
  hookRun: { operationId: string; hook: string } | null;
  // The last failed gate hook: its output, and the request to retry without hooks.
  hookFailure: HookFailure | null;
  // One-shot request for CommitBox to clear its message, carrying the committed text (a commit
  // retried from the hook dialog landed; CommitBox did not submit it, so it cannot know).
  // CommitBox clears only if the box still holds exactly that text.
  clearCommitMessage: string | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  rebaseOnto: string | null;
  // Per-commit actions to default to in the rebase planner, set only when it was opened via a
  // CommitGraph multi-select action (as opposed to "Rebase onto here").
  rebasePreset: ReadonlyMap<string, PresetAction> | null;
  pendingPull: { upstreamRef: string } | null;
  pullOutcome: PullOutcome | null;
  // Non-null while the merge options dialog is open for that branch.
  mergeDialog: { branchName: string } | null;
  // The last "Merge into current branch" result, for its toast.
  mergeOutcome: { branchName: string; outcome: MergeOutcome } | null;
  // Non-null while the stash options dialog is open; `paths` non-empty for a file stash.
  stashDialog: { paths: string[] } | null;
  // "Restore staged changes" for stash Apply/Pop; session-only, default on.
  stashRestoreIndex: boolean;
  // The last stash Apply/Pop result, for its toast.
  stashOutcome: { action: "apply" | "pop"; outcome: StashApplyOutcome } | null;
  transfer: TransferProgress | null;
  error: string | null;
  // True only while a `runMutation` call is in flight (from just before its `mutate()` call
  // through the trailing `refresh()`). Lets callers (e.g. `HistoryList`'s Apply/Drop buttons)
  // disable themselves so a rapid double-click can't fire the same index-based mutation twice
  // before the first one's refresh lands — see the stash Drop race this was added for.
  pending: boolean;
  // Advances once per `refresh()` call, whether it succeeded or not. `DiffPane` folds it into
  // every open working-tree diff's staleness signal, so anything that refreshes — a mutation's
  // trailing refresh, the palette's Refresh, a stash apply, a pull — refetches the diffs on
  // screen, even when the new `status` looks identical (hunk-level edits don't change it).
  refreshGeneration: number;
  undoState: UndoState;
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
  stageHunk(path: string, oldStart: number, newStart: number, contextLines: number): Promise<void>;
  unstageHunk(path: string, oldStart: number, newStart: number, contextLines: number): Promise<void>;
  discardHunk(path: string, oldStart: number, newStart: number, contextLines: number): Promise<void>;
  stageLines(path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]): Promise<void>;
  unstageLines(path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]): Promise<void>;
  discardLines(path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]): Promise<void>;
  discardFile(path: string): Promise<void>;
  discardAllUnstaged(paths: string[]): Promise<void>;
  ignorePath(pattern: string): Promise<void>;
  // `true` once committed. `noVerify` skips `pre-commit`/`commit-msg`; a failing hook stores
  // `state.hookFailure` and resolves `false`.
  commit(message: string, noVerify: boolean, trailers?: Trailer[]): Promise<boolean>;
  // Resolves to `null` on success, or the failure message on failure — mirrors `addRemote`
  // below. Naming a new branch is the one create-form action here with an obvious single trigger
  // point (the "New branch…" draft form), so its failure surfaces next to that form instead of
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
  pushCurrentBranch(remoteName: string, noVerify?: boolean): Promise<void>;
  pushTags(remoteName: string, names: string[], noVerify?: boolean): Promise<void>;
  pullCurrentUpstream(): Promise<void>;
  clearPendingPull(): void;
  openAddRemoteDraft(): void;
  closeAddRemoteDraft(): void;
  openRemoteDialog(kind: RemoteDialog["kind"], remoteName: string): void;
  closeRemoteDialog(): void;
  requestForcePush(remoteName: string): void;
  cancelForcePush(): void;
  forcePushCurrentBranch(remoteName: string, noVerify?: boolean): Promise<void>;
  fetchAllRemotes(): Promise<void>;
  setFetchPrune(prune: boolean): void;
  pushBranch(remoteName: string, localBranch: string, remoteBranch: string, setUpstream: boolean, noVerify?: boolean): Promise<void>;
  openPushDialog(localBranch: string): void;
  closePushDialog(): void;
  deleteRemoteBranch(remoteName: string, branchName: string, noVerify?: boolean): Promise<void>;
  openDeleteRemoteBranchDialog(remoteName: string | null, branchName: string | null): void;
  closeDeleteRemoteBranchDialog(): void;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  saveStash(options?: StashOptions): Promise<void>;
  openStashDialog(paths: string[]): void;
  closeStashDialog(): void;
  applyStash(index: number): Promise<void>;
  popStash(index: number): Promise<void>;
  setStashRestoreIndex(on: boolean): void;
  dropStash(index: number): Promise<void>;
  openMergeDialog(branchName: string): void;
  closeMergeDialog(): void;
  getMergeOptions(): Promise<MergeOptions>;
  mergeBranch(branchName: string, options: MergeOptions): Promise<void>;
  mergePull(upstreamRef: string): Promise<void>;
  resolveConflict(path: string, resolvedContent: string): Promise<void>;
  resolveAddDeleteConflict(path: string, choice: FileConflictChoice): Promise<void>;
  /** See `MergeRebaseActions.resolveWholeFile`. */
  resolveWholeFile(path: string, side: ConflictSide): Promise<void>;
  /** See `MergeRebaseActions.markResolved`. */
  markResolved(path: string, force: boolean): Promise<MarkResolvedOutcome | null>;
  abortMerge(): Promise<void>;
  openRebasePlanner(commitId: string): void;
  openRebaseSelection(onto: string, preset: ReadonlyMap<string, PresetAction>): void;
  closeRebasePlanner(): void;
  startRebase(onto: string, plan: RebasePlanEntry[]): Promise<void>;
  rebaseContinue(): Promise<void>;
  abortRebase(): Promise<RebaseAbort | null>;
  /** See `HistoryActions`: outcomes, or `null` on failure. */
  cherryPick(commitId: string, mainline?: number | null): Promise<PickOutcome | null>;
  revertCommit(commitId: string, mainline?: number | null): Promise<PickOutcome | null>;
  cherryPickMany(commitIdsOldestFirst: string[]): Promise<SequenceOutcome | null>;
  revertMany(commitIdsOldestFirst: string[]): Promise<SequenceOutcome | null>;
  continueSequence(noVerify?: boolean): Promise<SequenceOutcome | null>;
  abortSequence(): Promise<SequenceAbort | null>;
  resetToCommit(commitId: string, mode: ResetMode): Promise<void>;
  checkoutCommit(commitId: string): Promise<void>;
  amendCommit(message: string, noVerify: boolean, options?: AmendOptions): Promise<boolean>;
  setAmendMode(on: boolean): void;
  setSkipHooks(on: boolean): void;
  // Cancels the running commit/amend hook (`state.hookRun`) through `cancelTransfer`.
  cancelHookRun(): Promise<void>;
  openHookFailure(): void;
  closeHookFailure(): void;
  clearCommitMessageHandled(): void;
  // Replays the failed request with `noVerify: true`. Resolves `true` when a commit/amend landed
  // (pushes: once started; their outcome arrives as transfer progress).
  retryWithoutHooks(retry: HookRetry): Promise<boolean>;
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
  setGraphRemoteBranchSelection(selectedBranches: string[]): Promise<void>;
  // Fetches the next page of history (grows the graph limit by one page and reloads).
  loadMoreHistory(): Promise<void>;
  refresh(): Promise<void>;
  undo(): Promise<UndoOutcome | null>;
  redo(): Promise<UndoOutcome | null>;
  refreshUndoState(): Promise<void>;
  // Clears `state.error` without waiting for the next successful action of the same kind — the
  // global banner's dismiss control (issue #30/UX-002). See `App.tsx`'s `RepoWorkspace`.
  dismissError(): void;
}

export function useAppState(client: RepoClient, repoPath: string): UseAppStateResult {
  const [state, setRawState] = useState<AppState>({
    repoPath,
    selectedRow: "uncommitted",
    status: [],
    commits: [],
    graphLimit: GRAPH_PAGE_SIZE,
    hasMoreHistory: false,
    graphBranchSelection: null,
    graphRemoteBranchSelection: null,
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
    remoteDialog: null,
    stashes: [],
    mergeMessage: null,
    headInfo: { headId: null, detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [], sequence: null, rebase: null },
    forcePushOffer: null,
    forcePushConfirm: null,
    fetchPrune: readFetchPrune(repoPath),
    pushDialog: null,
    deleteRemoteBranchDialog: null,
    amendMode: false,
    skipHooks: false,
    hookRun: null,
    hookFailure: null,
    clearCommitMessage: null,
    rebaseProgress: null,
    rebaseOnto: null,
    rebasePreset: null,
    pendingPull: null,
    pullOutcome: null,
    mergeDialog: null,
    mergeOutcome: null,
    stashDialog: null,
    stashRestoreIndex: true,
    stashOutcome: null,
    transfer: null,
    error: null,
    pending: false,
    refreshGeneration: 0,
    undoState: { undo: null, redo: null },
  });

  const graphLimit = useRef(GRAPH_PAGE_SIZE);
  const selectedReflogReference = useRef<string | null>(null);
  const reflogRequestGeneration = useRef(0);
  const reflogError = useOwnError();

  // The message the last failed `refresh()` set, so a later successful one clears that error and
  // only that one: a refresh never clears another action's error (#177).
  const refreshError = useOwnError();
  const forgeError = useOwnError();
  // Every update goes through this, so a read's remembered error never outlives it on screen.
  const setState = useMemo(
    () => forgettingStaleOwnErrors(setRawState, [refreshError, reflogError, forgeError]),
    [refreshError, reflogError, forgeError],
  );
  const refresh = useCallback(async () => {
    try {
      const [graphBranchSelection, graphRemoteBranchSelection] = await Promise.all([
        client.getGraphBranchSelection(repoPath),
        client.getGraphRemoteBranchSelection(repoPath),
      ]);
      const limit = graphLimit.current;
      const [status, commits, branches, worktrees, submodules, reflogRefs, remotes, tags, upstream, stashes, mergeMessage, forgeRepositories, headInfo, undoState] =
        await Promise.all([
          client.getStatus(repoPath),
          client.getCommitGraph(repoPath, limit, graphBranchSelection),
          client.listBranches(repoPath),
          client.listWorktrees(repoPath),
          client.listSubmodules(repoPath),
          client.listReflogRefs(repoPath),
          client.listRemotes(repoPath),
          client.listTags(repoPath),
          client.getCurrentUpstream(repoPath),
          client.listStashes(repoPath),
          client.getMergeMessage(repoPath),
          client.detectForgeRepository(repoPath),
          client.getHeadInfo(repoPath),
          client.getUndoState(repoPath),
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
      const own = refreshError.take();
      setState((prev) => ({
        ...prev,
        status,
        commits,
        graphLimit: limit,
        hasMoreHistory: commits.length >= limit,
        graphBranchSelection,
        graphRemoteBranchSelection,
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
        headInfo,
        // The backend reports a paused rebase in `HeadInfo`, restored from disk after a restart.
        rebaseProgress: headInfo.rebase
          ? { currentStep: headInfo.rebase.currentStep, totalSteps: headInfo.rebase.totalSteps }
          : null,
        forgeRepositories,
        undoState,
        error: errorAfterOwnSuccess(prev.error, own),
        refreshGeneration: prev.refreshGeneration + 1,
      }));
    } catch (err) {
      // Runs the same worker-death/credential classification as every mutation's catch block
      // (`useMutationRunner`'s `credentialFailureMessage`) — a `refresh()` call after a
      // successful mutation can hit a worker that died in between just as easily as the mutation
      // itself, and it shouldn't fall back to a raw, unclassified message just because it's this
      // call site instead of one of `runMutation`'s. See AUD-2026-09-05-CONC-002.
      // Still advances `refreshGeneration`: a mutation that landed before this refresh failed has
      // changed the working tree all the same, so open diffs shouldn't keep trusting what they
      // last fetched.
      const message = credentialFailureMessage(err);
      refreshError.set(message);
      setState((prev) => ({
        ...prev,
        error: message,
        refreshGeneration: prev.refreshGeneration + 1,
      }));
    }
  }, [client, repoPath, refreshError, setState]);

  // Cheap focus-time re-check: another process (CLI, the other frontend) may have changed what
  // undo would do. Errors are ignored — the next full refresh reports them.
  const refreshUndoState = useCallback(async () => {
    try {
      const undoState = await client.getUndoState(repoPath);
      setState((prev) => ({ ...prev, undoState }));
    } catch {
      // ignored, see above
    }
  }, [client, repoPath, setState]);

  const loadMoreHistory = useCallback(async () => {
    graphLimit.current += GRAPH_PAGE_SIZE;
    await refresh();
  }, [refresh]);

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
    stageLines,
    unstageLines,
    discardLines,
    discardFile,
    discardAllUnstaged,
    ignorePath,
    commit,
  } = useStagingActions(client, repoPath, runMutation, runMutationWithOutcome, runOptimisticMutation, setState);

  const branchNames = useMemo(() => state.branches.map((branch) => branch.name), [state.branches]);
  const currentBranch = state.branches.find((branch) => branch.isCurrent)?.name ?? null;

  const {
    createBranch,
    switchBranch,
    deleteBranch,
    renameBranch,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    setGraphBranchSelection,
    setGraphRemoteBranchSelection,
  } = useBranchActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    setState,
    state.graphBranchSelection,
    branchNames,
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
    reflogError,
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
    openRemoteDialog,
    closeRemoteDialog,
    requestForcePush,
    cancelForcePush,
    forcePushCurrentBranch,
    fetchAllRemotes,
    setFetchPrune,
    pushBranch,
    openPushDialog,
    closePushDialog,
    deleteRemoteBranch,
    openDeleteRemoteBranchDialog,
    closeDeleteRemoteBranchDialog,
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
    state.upstream,
    state.fetchPrune,
    currentBranch,
  );

  const {
    saveStash,
    openStashDialog,
    closeStashDialog,
    applyStash,
    popStash,
    setStashRestoreIndex,
    dropStash,
  } = useStashActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
    setState,
    state.stashes,
    state.stashRestoreIndex,
  );

  const {
    openMergeDialog,
    closeMergeDialog,
    getMergeOptions,
    mergeBranch,
    mergePull,
    resolveConflict,
    resolveAddDeleteConflict,
    resolveWholeFile,
    markResolved,
    abortMerge,
    openRebasePlanner,
    openRebaseSelection,
    closeRebasePlanner,
    startRebase,
    rebaseContinue,
    abortRebase,
  } = useMergeRebaseActions(client, repoPath, runMutation, setState);

  const {
    cherryPick,
    revertCommit,
    cherryPickMany,
    revertMany,
    continueSequence,
    abortSequence,
    resetToCommit,
    checkoutCommit,
    amendCommit,
    setAmendMode,
  } =
    useHistoryActions(client, repoPath, runMutation, runMutationWithOutcome, setState);
  const { setSkipHooks, cancelHookRun, openHookFailure, closeHookFailure, clearCommitMessageHandled } =
    useHookActions(client, repoPath, state.hookRun, setState);
  const retryWithoutHooks = useCallback(
    async (retry: HookRetry) => {
      setState((prev) => ({ ...prev, hookFailure: null, error: null }));
      if (retry.kind === "commit") {
        const ok = await commit(retry.message, true, retry.trailers);
        if (ok) setState((prev) => ({ ...prev, clearCommitMessage: retry.message }));
        return ok;
      }
      if (retry.kind === "amend") {
        // No clear request: a landed amend leaves amend mode, which already puts the
        // pre-amend draft back into the CommitBox.
        return amendCommit(retry.message, true, retry.options);
      }
      if (retry.kind === "continueSequence") return (await continueSequence(true)) !== null;
      const push = retry.push;
      if (push.operation === "PushBranch") await pushCurrentBranch(push.remoteName, true);
      else if (push.operation === "ForcePushBranch") await forcePushCurrentBranch(push.remoteName, true);
      else if (push.operation === "PushTags") await pushTags(push.remoteName, push.names, true);
      else if (push.operation === "PushBranchTo")
        await pushBranch(push.remoteName, push.localBranch, push.remoteBranch, push.setUpstream, true);
      else await deleteRemoteBranch(push.remoteName, push.branchName, true);
      return true;
    },
    [commit, amendCommit, continueSequence, pushCurrentBranch, forcePushCurrentBranch, pushTags, pushBranch, deleteRemoteBranch, setState],
  );

  const { undo, redo } = useUndoActions(client, repoPath, runMutationWithOutcome, setState);

  const { listPullRequests, saveForgeToken, forgetForgeToken, createPullRequest, openExternalUrl } =
    useForgeActions(client, repoPath, runMutation, runMutationWithOutcome, setState, forgeError);
  const dismissError = useCallback(() => {
    setState((prev) => ({ ...prev, error: null, forcePushOffer: null, forcePushConfirm: null, hookFailure: null }));
  }, [setState]);

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
    stageLines,
    unstageLines,
    discardLines,
    discardFile,
    discardAllUnstaged,
    ignorePath,
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
    openRemoteDialog,
    closeRemoteDialog,
    requestForcePush,
    cancelForcePush,
    forcePushCurrentBranch,
    fetchAllRemotes,
    setFetchPrune,
    pushBranch,
    openPushDialog,
    closePushDialog,
    deleteRemoteBranch,
    openDeleteRemoteBranchDialog,
    closeDeleteRemoteBranchDialog,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    saveStash,
    openStashDialog,
    closeStashDialog,
    applyStash,
    popStash,
    setStashRestoreIndex,
    dropStash,
    openMergeDialog,
    closeMergeDialog,
    getMergeOptions,
    mergeBranch,
    mergePull,
    resolveConflict,
    resolveAddDeleteConflict,
    resolveWholeFile,
    markResolved,
    abortMerge,
    openRebasePlanner,
    openRebaseSelection,
    closeRebasePlanner,
    startRebase,
    rebaseContinue,
    abortRebase,
    cherryPick,
    revertCommit,
    cherryPickMany,
    revertMany,
    continueSequence,
    abortSequence,
    resetToCommit,
    checkoutCommit,
    amendCommit,
    setAmendMode,
    listPullRequests,
    saveForgeToken,
    forgetForgeToken,
    createPullRequest,
    openExternalUrl,
    setGraphBranchSelection,
    setGraphRemoteBranchSelection,
    loadMoreHistory,
    refresh,
    undo,
    redo,
    refreshUndoState,
    dismissError,
    setSkipHooks,
    cancelHookRun,
    openHookFailure,
    closeHookFailure,
    clearCommitMessageHandled,
    retryWithoutHooks,
  };
}
