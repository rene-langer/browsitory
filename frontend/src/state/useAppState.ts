import { useCallback, useMemo, useRef, useState } from "react";
import type {
  BranchInfo,
  ForgeRepository,
  GraphCommit,
  HeadInfo,
  HookSource,
  MergeOutcome,
  PullOutcome,
  PullRequestList,
  ReflogEntry,
  RemoteInfo,
  RepoClient,
  StashApplyOutcome,
  StashEntry,
  StatusEntry,
  SubmoduleInfo,
  TagInfo,
  TransferProgress,
  UndoState,
  UpstreamInfo,
  WorktreeInfo,
} from "../ipc/RepoClient";
import { type SavedGraphSelection, toGraphSelection } from "./graphSelection";
import { failureError, useMutationRunner, withoutStaleErrorHint } from "./useMutationRunner";
import { errorAfterOwnSuccess, forgettingStaleOwnErrors, useOwnError } from "./ownError";
import { readFetchPrune } from "../lib/fetchPrune";
import {
  initialNavigationHistory,
  moveToEntry,
  navigationTarget,
  recordVisit,
  type NavigationHistory,
} from "../lib/navigationHistory";
import type { HookFailure, HookRetry } from "../lib/hookFailure";
import type { PresetAction } from "../lib/rebaseSelection";
import { type BranchActions, useBranchActions } from "./useBranchActions";
import { type DialogActions, useDialogActions } from "./useDialogActions";
import { type ForgeActions, useForgeActions } from "./useForgeActions";
import { type HistoryActions, useHistoryActions } from "./useHistoryActions";
import { type HookActions, useHookActions } from "./useHookActions";
import { type MergeRebaseActions, useMergeRebaseActions } from "./useMergeRebaseActions";
import { type PatchActions, usePatchActions } from "./usePatchActions";
import { type ReflogActions, useReflogActions } from "./useReflogActions";
import { type RemoteTransferActions, useRemoteTransferActions } from "./useRemoteTransferActions";
import { type StagingActions, useStagingActions } from "./useStagingActions";
import { type StashActions, useStashActions } from "./useStashActions";
import { type SubmoduleActions, useSubmoduleActions } from "./useSubmoduleActions";
import { type UndoActions, useUndoActions } from "./useUndoActions";
import { type WorktreeActions, useWorktreeActions } from "./useWorktreeActions";
import { type WorkspaceDialog, withoutDialog } from "./workspaceDialog";

// History is loaded a page at a time; `loadMoreHistory` grows the limit by one page.
const GRAPH_PAGE_SIZE = 300;

export type SelectedRow = "uncommitted" | { commitId: string };

export type { RemoteDialog, WorkspaceDialog } from "./workspaceDialog";

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
  // The rows this tab visited, for Back and Forward (#618); `selectRow` records them.
  navigation: NavigationHistory;
  status: StatusEntry[];
  commits: GraphCommit[];
  // Limit the current `commits` were fetched with, and whether the graph filled it (so older
  // history probably exists beyond what is shown).
  graphLimit: number;
  hasMoreHistory: boolean;
  // The persisted branch selection. `local` `null` means "no filter saved" — every local branch
  // is walked (see `graph_log` in `git-core`); non-null is the subset CommitGraph currently
  // shows. `remote` holds `<remote>/<branch>` names; `null` means "no remote selection saved": no
  // remote branch is walked and CommitGraph shows every remote-branch badge on the commits it
  // has. Non-null: these remote branches' commits join the walk (#385) and only their badges
  // show. Together, exactly one branch in total makes the graph first-parent only. The client
  // takes the same pair as a `GraphSelection`, with a saved `null` remote sent as `[]`.
  graphSelection: SavedGraphSelection;
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
  // The one open modal dialog (#493): see `workspaceDialog.ts`, and `docs/ARCHITECTURE.md`'s "Who
  // owns a dialog's open state" for which dialogs live here.
  dialog: WorkspaceDialog | null;
  // The sidebar's inline New branch form, open with that start point. Not a `dialog`: an inline
  // form, open alongside the Add remote form or a dialog. Here so the palette can open it.
  createBranchDraft: { startPoint: string } | null;
  // Mirrors createBranchDraft: lets both BranchTree's own "+" button and the command palette's
  // "Add remote" entry open the same inline add-remote form.
  addRemoteDraftOpen: boolean;
  stashes: StashEntry[];
  mergeMessage: string | null;
  headInfo: HeadInfo;
  // Set when a branch push fails NonFastForward; shows the "Force push (with lease)" button.
  forcePushOffer: { remoteName: string } | null;
  // The Prune choice for every fetch in this repo (localStorage, see `lib/fetchPrune.ts`);
  // `null` = never set, so git config decides per remote.
  fetchPrune: boolean | null;
  // CommitBox amend toggle.
  amendMode: boolean;
  // CommitBox "Skip hooks" (`--no-verify`) for the next commit/amend; reset after one lands.
  skipHooks: boolean;
  // The hook running right now (from `subscribeHookProgress`), for its status line and Cancel
  // button: a `Commit` source's in the commit box and sequence panel, the others in the "Hook
  // status" strip (#185). Pushes and pulls show theirs through `transfer` instead.
  hookRun: { operationId: string; hook: string; source: HookSource } | null;
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
  // The last one-click history edit that finished, for its toast (#442).
  historyEditOutcome: { message: string } | null;
  // The last finished revert/apply of a commit's file or hunk, saved patch or applied patch file,
  // for its toast (#448).
  patchOutcome: { message: string } | null;
  pendingPull: { upstreamRef: string } | null;
  pullOutcome: PullOutcome | null;
  // The last "Merge into current branch" result, for its toast.
  mergeOutcome: { branchName: string; outcome: MergeOutcome } | null;
  // "Restore staged changes" for stash Apply/Pop; session-only, default on.
  stashRestoreIndex: boolean;
  // The last stash Apply/Pop result, for its toast.
  stashOutcome: { action: "apply" | "pop"; outcome: StashApplyOutcome } | null;
  transfer: TransferProgress | null;
  error: string | null;
  // A plain-language next step for `error`, from its kind (`failureError`, #573); shown under it in
  // the banner. Dropped whenever `error` changes without a new one (`withoutStaleErrorHint`).
  errorHint: string | null;
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

/** The members `useAppState` defines itself; the rest come from its action slices. */
export interface AppStateOwnMembers {
  state: AppState;
  // Replays the failed request with `noVerify: true`. Resolves `true` when a commit/amend landed
  // (pushes: once started; their outcome arrives as transfer progress).
  retryWithoutHooks(retry: HookRetry): Promise<boolean>;
  // Fetches the next page of history (grows the graph limit by one page and reloads).
  loadMoreHistory(): Promise<void>;
  // Reloads the graph with at least `count` commits (whole pages), so a "Go to" target's row
  // (`CommitLocation.position + 1`) is loaded (#441).
  loadHistoryThrough(count: number): Promise<void>;
  refresh(): Promise<void>;
  refreshUndoState(): Promise<void>;
  // Clears `state.error` without waiting for the next successful action of the same kind — the
  // global banner's dismiss control (issue #30/UX-002). See `RepoWorkspace.tsx`.
  dismissError(): void;
  // Back (`-1`) or Forward (`1`) through `state.navigation` (#618): selects the nearest entry that
  // way whose row `isAvailable`, skipping the others, and returns it; `null` at the end. A
  // selection made without `selectRow` (one an action set) counts as the current entry.
  stepNavigation(direction: -1 | 1, isAvailable: (row: SelectedRow) => boolean): SelectedRow | null;
}

// The members are documented on the slice interfaces, the one place each is declared.
export interface UseAppStateResult
  extends AppStateOwnMembers,
    DialogActions,
    StagingActions,
    PatchActions,
    BranchActions,
    WorktreeActions,
    SubmoduleActions,
    ReflogActions,
    RemoteTransferActions,
    StashActions,
    MergeRebaseActions,
    HistoryActions,
    HookActions,
    UndoActions,
    ForgeActions {}

// `useAppState` spreads the slices into one object, where a later slice would silently replace
// an earlier one's member of the same name (and `extends` accepts two identical declarations).
// `UseAppStateParts` lists the same interfaces as `extends` above; `NoClash` fails to compile,
// naming the member, when two parts share a member or a part is missing from the list.
type UseAppStateParts = [
  AppStateOwnMembers,
  DialogActions,
  StagingActions,
  PatchActions,
  BranchActions,
  WorktreeActions,
  SubmoduleActions,
  ReflogActions,
  RemoteTransferActions,
  StashActions,
  MergeRebaseActions,
  HistoryActions,
  HookActions,
  UndoActions,
  ForgeActions,
];
type MembersOf<T extends unknown[]> = T extends [infer Head, ...infer Rest] ? keyof Head | MembersOf<Rest> : never;
type SharedMembers<T extends unknown[]> = T extends [infer Head, ...infer Rest]
  ? (keyof Head & MembersOf<Rest>) | SharedMembers<Rest>
  : never;
type NoClash<T extends never> = T;
export type UseAppStatePartsCheck = NoClash<
  SharedMembers<UseAppStateParts> | Exclude<keyof UseAppStateResult, MembersOf<UseAppStateParts>>
>;

export function useAppState(client: RepoClient, repoPath: string): UseAppStateResult {
  const [state, setRawState] = useState<AppState>({
    repoPath,
    selectedRow: "uncommitted",
    navigation: initialNavigationHistory("uncommitted"),
    status: [],
    commits: [],
    graphLimit: GRAPH_PAGE_SIZE,
    hasMoreHistory: false,
    graphSelection: { local: null, remote: null },
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
    dialog: null,
    createBranchDraft: null,
    addRemoteDraftOpen: false,
    stashes: [],
    mergeMessage: null,
    headInfo: { headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
    forcePushOffer: null,
    fetchPrune: readFetchPrune(repoPath),
    amendMode: false,
    skipHooks: false,
    hookRun: null,
    hookFailure: null,
    clearCommitMessage: null,
    rebaseProgress: null,
    rebaseOnto: null,
    rebasePreset: null,
    historyEditOutcome: null,
    patchOutcome: null,
    pendingPull: null,
    pullOutcome: null,
    mergeOutcome: null,
    stashRestoreIndex: true,
    stashOutcome: null,
    transfer: null,
    error: null,
    errorHint: null,
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
  const setState = useMemo(() => {
    const set = forgettingStaleOwnErrors(setRawState, [refreshError, reflogError, forgeError]);
    return (updater: (prev: AppState) => AppState) => set((prev) => withoutStaleErrorHint(prev, updater(prev)));
  }, [refreshError, reflogError, forgeError]);
  // Replies don't arrive in the order their requests were sent: the VSCode sidecar answers each
  // request on its own thread, Tauri runs commands concurrently. So a read sent before a
  // mutation landed can answer after a later read that already shows it, and its stale result
  // would then stay on screen until the next refresh: a commit's own Undo button never appearing
  // (#342). Each `refresh()` (when it sends its reads) and each `refreshUndoState()` takes a
  // ticket; a result is applied only when no read sent after it has been applied already.
  const readOrder = useRef({ issued: 0, refreshApplied: 0, undoApplied: 0 });
  const takeReadTicket = useCallback(() => ++readOrder.current.issued, []);
  const refresh = useCallback(async () => {
    let ticket = 0;
    try {
      const [local, remote] = await Promise.all([
        client.getGraphBranchSelection(repoPath),
        client.getGraphRemoteBranchSelection(repoPath),
      ]);
      const limit = graphLimit.current;
      ticket = takeReadTicket();
      const [status, commits, branches, worktrees, submodules, reflogRefs, remotes, tags, upstream, stashes, mergeMessage, forgeRepositories, headInfo, undoState] =
        await Promise.all([
          client.getStatus(repoPath),
          client.getCommitGraph(repoPath, limit, toGraphSelection({ local, remote })),
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
      const order = readOrder.current;
      if (ticket < order.refreshApplied) {
        // A refresh that sent its reads later has already landed: these results are older.
        setState((prev) => ({ ...prev, refreshGeneration: prev.refreshGeneration + 1 }));
        return;
      }
      order.refreshApplied = ticket;
      const undoIsNewest = ticket > order.undoApplied;
      if (undoIsNewest) order.undoApplied = ticket;
      const own = refreshError.take();
      setState((prev) => ({
        ...prev,
        status,
        commits,
        graphLimit: limit,
        hasMoreHistory: commits.length >= limit,
        graphSelection: { local, remote },
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
        undoState: undoIsNewest ? undoState : prev.undoState,
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
      if (ticket !== 0 && ticket < readOrder.current.refreshApplied) {
        // Superseded as above: a later refresh already succeeded.
        setState((prev) => ({ ...prev, refreshGeneration: prev.refreshGeneration + 1 }));
        return;
      }
      const failure = failureError(err);
      refreshError.set(failure.error);
      setState((prev) => ({
        ...prev,
        ...failure,
        refreshGeneration: prev.refreshGeneration + 1,
      }));
    }
  }, [client, repoPath, refreshError, setState, takeReadTicket]);

  // Cheap focus-time re-check: another process (CLI, the other frontend) may have changed what
  // undo would do. Errors are ignored — the next full refresh reports them. Ordered with
  // `refresh()`'s reads (see `readOrder`): a focus that lands during a commit must not put the
  // pre-commit state back after the commit's refresh.
  const refreshUndoState = useCallback(async () => {
    const ticket = takeReadTicket();
    try {
      const undoState = await client.getUndoState(repoPath);
      if (ticket < readOrder.current.undoApplied) return;
      readOrder.current.undoApplied = ticket;
      setState((prev) => ({ ...prev, undoState }));
    } catch {
      // ignored, see above
    }
  }, [client, repoPath, setState, takeReadTicket]);

  const loadMoreHistory = useCallback(async () => {
    graphLimit.current += GRAPH_PAGE_SIZE;
    await refresh();
  }, [refresh]);

  const loadHistoryThrough = useCallback(
    async (count: number) => {
      graphLimit.current = Math.max(graphLimit.current, Math.ceil(count / GRAPH_PAGE_SIZE) * GRAPH_PAGE_SIZE);
      await refresh();
    },
    [refresh],
  );

  const mutationRunner = useMutationRunner(refresh, setState);
  const { runMutation, runMutationWithOutcome, runOptimisticMutation, runOptimisticMutationWithMessage } =
    mutationRunner;

  const dialogs = useDialogActions(setState);
  const staging = useStagingActions(client, repoPath, runMutation, runMutationWithOutcome, runOptimisticMutation, setState);
  const patches = usePatchActions(client, repoPath, runMutationWithOutcome, setState);

  const branchNames = useMemo(() => state.branches.map((branch) => branch.name), [state.branches]);
  const currentBranch = state.branches.find((branch) => branch.isCurrent)?.name ?? null;

  const branches = useBranchActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    setState,
    state.graphSelection,
    branchNames,
  );
  const worktrees = useWorktreeActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    setState,
    state.worktrees,
  );

  const submodules = useSubmoduleActions(client, repoPath, runMutation);

  const reflog = useReflogActions(
    client,
    repoPath,
    runMutation,
    setState,
    selectedReflogReference,
    reflogRequestGeneration,
    reflogError,
  );

  const remoteTransfer = useRemoteTransferActions({
    client,
    repoPath,
    refresh,
    runners: mutationRunner,
    setState,
    snapshot: {
      upstream: state.upstream,
      fetchPrune: state.fetchPrune,
      currentBranch,
      remotes: state.remotes,
      tags: state.tags,
    },
  });

  const stash = useStashActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
    setState,
    state.stashes,
    state.stashRestoreIndex,
  );

  const mergeRebase = useMergeRebaseActions(client, repoPath, runMutation, setState);

  const history = useHistoryActions(client, repoPath, runMutation, runMutationWithOutcome, setState);
  const hooks = useHookActions(client, repoPath, state.hookRun, setState);
  // The slice objects are new on every render, their actions are not: the callback depends on
  // the actions, so it stays as stable as they are.
  const { commit } = staging;
  const { amendCommit, continueSequence } = history;
  const { pushCurrentBranch, forcePushCurrentBranch, pushTags, pushBranch, deleteRemoteBranch, deleteRemoteTag } =
    remoteTransfer;
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
      // A pick's `prepare-commit-msg` can't be skipped; the dialog offers no retry for it.
      if (retry.kind === "pick") return false;
      const push = retry.push;
      if (push.operation === "PushBranch") await pushCurrentBranch(push.remoteName, true);
      else if (push.operation === "ForcePushBranch") await forcePushCurrentBranch(push.remoteName, true);
      else if (push.operation === "PushTags") await pushTags(push.remoteName, push.names, true);
      else if (push.operation === "PushBranchTo")
        await pushBranch(push.remoteName, push.localBranch, push.remoteBranch, push.setUpstream, true);
      else if (push.operation === "DeleteRemoteTag")
        await deleteRemoteTag(push.remoteName, push.tagName, push.deleteLocal, true);
      else await deleteRemoteBranch(push.remoteName, push.branchName, true);
      return true;
    },
    [commit, amendCommit, continueSequence, pushCurrentBranch, forcePushCurrentBranch, pushTags, pushBranch, deleteRemoteBranch, deleteRemoteTag, setState],
  );

  const undoActions = useUndoActions(client, repoPath, runMutationWithOutcome, setState);

  const forge = useForgeActions(client, repoPath, runMutation, runMutationWithOutcome, setState, forgeError);
  const dismissError = useCallback(() => {
    setState((prev) => ({
      ...prev,
      error: null,
      forcePushOffer: null,
      dialog: withoutDialog(prev.dialog, "forcePush"),
      hookFailure: null,
    }));
  }, [setState]);

  const { navigation, selectedRow } = state;
  const stepNavigation = useCallback(
    (direction: -1 | 1, isAvailable: (row: SelectedRow) => boolean): SelectedRow | null => {
      const synced = recordVisit(navigation, selectedRow, "jump");
      const index = navigationTarget(synced, direction, isAvailable);
      if (index === null) return null;
      const row = synced.entries[index];
      setState((prev) => ({ ...prev, selectedRow: row, navigation: moveToEntry(synced, index) }));
      return row;
    },
    [navigation, selectedRow, setState],
  );

  return {
    state,
    stepNavigation,
    retryWithoutHooks,
    loadMoreHistory,
    loadHistoryThrough,
    refresh,
    refreshUndoState,
    dismissError,
    ...dialogs,
    ...staging,
    ...patches,
    ...branches,
    ...worktrees,
    ...submodules,
    ...reflog,
    ...remoteTransfer,
    ...stash,
    ...mergeRebase,
    ...history,
    ...hooks,
    ...undoActions,
    ...forge,
  };
}
