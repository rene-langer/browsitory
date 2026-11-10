import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { BranchTree } from "./components/BranchTree";
import { CommandPalette } from "./components/CommandPalette";
import type { CommitDraft } from "./components/CommitBox";
import { ShortcutSheet } from "./components/ShortcutSheet";
import { CommitGraph } from "./components/CommitGraph";
import { CompareDiffPane } from "./components/CompareDiffPane";
import { FileHistoryList } from "./components/FileHistoryList";
import { folderSearchQuery } from "./lib/folderHistory";
import { CommitSearchBar } from "./components/CommitSearchBar";
import { CommitSearchResults } from "./components/CommitSearchResults";
import { GoToNotice } from "./components/GoToNotice";
import { SyncBar } from "./components/SyncBar";
import { UndoRedoButtons } from "./components/UndoRedoButtons";
import { DiffPane, type DiffPanePatchActions } from "./components/DiffPane";
import { commitsPatchName } from "./state/usePatchActions";
import { RebasePlanner } from "./components/RebasePlanner";
import { ReflogPanel } from "./components/ReflogPanel";
import { repoPanelId, repoTabId } from "./components/repoTabIds";
import { OperationStatusStrip } from "./components/OperationStatusStrip";
import { HookRunStrip } from "./components/HookRunStrip";
import type { ToastAction } from "./state/useToasts";
import { InlineError } from "./components/primitives/InlineError";
import { ConfirmDialog } from "./components/primitives/ConfirmDialog";
import { HookFailureDialog } from "./components/HookFailureDialog";
import { retrySuccessLabel } from "./lib/hookFailure";
import { Overlay } from "./components/primitives/Overlay";
import { Sidebar } from "./components/primitives/Sidebar";
import { SplitView } from "./components/primitives/SplitView";
import { PullRequestPanel } from "./components/PullRequestPanel";
import { StashPanel } from "./components/StashPanel";
import { TagPanel } from "./components/TagPanel";
import { SubmodulePanel } from "./components/SubmodulePanel";
import { TransferPanel } from "./components/TransferPanel";
import { WorktreePanel } from "./components/WorktreePanel";
import { MergeDialog } from "./components/MergeDialog";
import { PushBranchDialog } from "./components/PushBranchDialog";
import { StashDialog } from "./components/StashDialog";
import { ParentPickerDialog } from "./components/ParentPickerDialog";
import { ResetDialog } from "./components/ResetDialog";
import { DeleteRemoteBranchDialog } from "./components/DeleteRemoteBranchDialog";
import { CreateTagDialog } from "./components/CreateTagDialog";
import { RewordDialog } from "./components/RewordDialog";
import { DeleteRemoteTagDialog } from "./components/DeleteRemoteTagDialog";
import { RemoteDialogs } from "./components/RemoteDialogs";
import type { GraphCommit, RepoClient } from "./ipc/RepoClient";
import { buildCommands, type AppCommands, type CommitCommands } from "./lib/commands";
import { useExternalTools } from "./state/useExternalTools";
import { effectiveFetchPrune } from "./lib/fetchPrune";
import {
  effectiveAutoFetchMinutes,
  lastFetchAt,
  readAutoFetchOverride,
  subscribeAutoFetchOverride,
  subscribeLastFetch,
} from "./lib/autoFetch";
import { useAutoFetch } from "./state/useAutoFetch";
import { useFetchSettingsContext } from "./state/useFetchSettings";
import { upstreamPushBranch } from "./lib/pushTarget";
import { useAppState } from "./state/useAppState";
import { useFileHistory } from "./state/useFileHistory";
import {
  busyReason,
  operationBlockers,
  pickToast,
  sequenceToast,
  stashBlockReason,
  terminalAbortConfirm,
} from "./lib/operationStatus";
import { stashBlockers } from "./lib/actions/stashActions";
import { useCommitSearch } from "./state/useCommitSearch";
import { useCompare } from "./state/useCompare";
import { CurrentDiffContext, useCurrentDiffRegistry } from "./state/useCurrentDiff";
import { useGoTo } from "./state/useGoTo";
import type { SelectedRow } from "./state/useAppState";
import { useDiffSettingsContext } from "./state/useDiffSettings";
import type { DiscardRequest, ReportedFileSelection } from "./components/DiffPane";
import type { OpenRepo } from "./state/useOpenRepos";
import { useSidebarPanelVisibility } from "./state/useSidebarPanelVisibility";
import styles from "./App.module.css";
import { useAnnounce } from "./state/useAnnounce";
import { useCommitDraftActions } from "./state/useCommitDraftActions";
import { useNarrowLayout } from "./state/useNarrowLayout";
import { useOutcomeToasts } from "./state/useOutcomeToasts";
import { useSequenceActions } from "./state/useSequenceActions";
import { useUndoRedoActions } from "./state/useUndoRedoActions";
import { useWorkspaceShortcuts } from "./state/useWorkspaceShortcuts";

// Toast copy for line-level stage/unstage/discard ("Staged 1 line", "Discarded 3 lines").
const linesToast = (verb: string, n: number) => `${verb} ${n} ${n === 1 ? "line" : "lines"}`;

export function RepoWorkspace({
  repoPath,
  active,
  client,
  onOpenRepoTab,
  onBusyChange,
  openRepos,
  onSwitchRepoTab,
  onCloseRepoTab,
  getCommitDraft,
  onCommitDraftChange,
  appCommands,
  pushToast,
}: {
  repoPath: string;
  client: RepoClient;
  active: boolean;
  // `Promise<void>` rather than the plan's `void`: `WorktreePanel`'s `onOpenWorktree` is typed
  // `(path: string) => Promise<void>`, and this is the only value passed to it. It stays
  // assignable to `buildCommands`' `(path: string) => void` parameter (Task 8).
  onOpenRepoTab: (path: string) => Promise<void>;
  onBusyChange: (repoPath: string, busy: boolean) => void;
  openRepos: OpenRepo[];
  onSwitchRepoTab: (path: string) => void;
  onCloseRepoTab: (path: string) => void;
  // `App`'s per-repo commit-message drafts (see `useCommitDrafts`): this workspace unmounts
  // whenever its tab isn't active, so `CommitBox`'s own state can't outlive a tab switch.
  getCommitDraft: (repoPath: string) => CommitDraft | undefined;
  onCommitDraftChange: (repoPath: string, draft: CommitDraft) => void;
  appCommands?: AppCommands;
  // The single app-level toast stack (`App` owns the one `ToastRegion`).
  pushToast: (message: string, action?: ToastAction) => void;
}) {
  const appState = useAppState(client, repoPath);
  const fileHistory = useFileHistory(
    client,
    repoPath,
    appState.state.refreshGeneration,
    appState.state.selectedRow,
    appState.selectRow,
  );
  const commitSearch = useCommitSearch(
    client,
    repoPath,
    appState.state.refreshGeneration,
    appState.state.graphBranchSelection,
    appState.state.graphRemoteBranchSelection,
    appState.state.selectedRow,
    appState.selectRow,
  );
  const compare = useCompare(client, repoPath, appState.state.refreshGeneration, appState.state.selectedRow);
  const conflictedPaths = useMemo(
    () => appState.state.status.filter((entry) => entry.kind === "Conflicted").map((entry) => entry.path),
    [appState.state.status],
  );
  const externalTools = useExternalTools(client, repoPath, conflictedPaths, appState.refresh);
  // A click on a row closes an open compare even when it is the row already selected (the
  // compare otherwise only closes when `selectedRow` changes), so clicking a stash or commit
  // always shows it.
  const closeCompare = compare.close;
  const selectRow = appState.selectRow;
  const selectRowClosingCompare = useCallback(
    (row: SelectedRow) => {
      closeCompare();
      selectRow(row);
    },
    [closeCompare, selectRow],
  );
  const [searchFocusRequested, setSearchFocusRequested] = useState(false);
  const clearSearchFocusRequest = useCallback(() => setSearchFocusRequested(false), []);
  const searchResultsRef = useRef<HTMLUListElement>(null);
  const closeFileHistory = fileHistory.close;
  const fileHistoryOpen = fileHistory.history !== null;
  // Ctrl/Cmd+F on a commit list, or the palette: the field is hidden while file history is
  // open, so close that first.
  const requestSearchFocus = useCallback(() => {
    if (fileHistoryOpen) closeFileHistory();
    setSearchFocusRequested(true);
  }, [fileHistoryOpen, closeFileHistory]);
  // "Go to" (#441): show the graph (closing search, file history and compare), select the row
  // and scroll to it, even when it was already selected.
  const [revealRequest, setRevealRequest] = useState(0);
  const closeSearch = commitSearch.close;
  const reveal = useCallback(
    (row: SelectedRow) => {
      closeSearch();
      if (fileHistoryOpen) closeFileHistory();
      selectRowClosingCompare(row);
      setRevealRequest((n) => n + 1);
    },
    [closeSearch, fileHistoryOpen, closeFileHistory, selectRowClosingCompare],
  );
  const goTo = useGoTo(client, repoPath, appState, reveal);
  const setSearchQuery = commitSearch.setQuery;
  const searchHistory = useCallback(
    (query: string) => {
      setSearchQuery(query);
      requestSearchFocus();
    },
    [setSearchQuery, requestSearchFocus],
  );
  // "Folder history" (#447) is a `path:` search for the folder.
  const openFolderHistory = useCallback((folder: string) => searchHistory(folderSearchQuery(folder)), [searchHistory]);
  const selectedRow = appState.state.selectedRow;
  const historyEntry =
    fileHistory.history !== null && typeof selectedRow === "object"
      ? fileHistory.history.entries.find((e) => e.commit.id === selectedRow.commitId) ?? null
      : null;
  const historyEntries = fileHistory.history?.entries;
  const searchCommits = commitSearch.results?.commits;
  const diffPaneCommits = useMemo(
    () => historyEntries?.map((e) => e.commit) ?? searchCommits ?? appState.state.commits,
    [historyEntries, searchCommits, appState.state.commits],
  );
  const { handleCommitDraftChange, commitAndClearDraft, amendAndRecord, retryWithCurrentMessage } = useCommitDraftActions(
    repoPath,
    getCommitDraft,
    onCommitDraftChange,
    appState,
  );
  const panelVisibility = useSidebarPanelVisibility();
  const [focusCommitMessage, setFocusCommitMessage] = useState(false);
  const clearFocusCommitMessage = useCallback(() => setFocusCommitMessage(false), []);
  // Discard confirmations live here (not in DiffPane) so the palette can open the same dialogs.
  const [discardRequest, setDiscardRequest] = useState<DiscardRequest | null>(null);
  // The parent picker and reset dialog of one commit (#507, #514): the commit menu and the
  // palette open this one instance, so the palette needs no rendered graph row (search results,
  // file history) and both get the same busy guard.
  const [commitDialog, setCommitDialog] = useState<{
    kind: "Cherry-pick" | "Revert" | "Reset";
    commit: GraphCommit;
    returnFocusTo: HTMLElement | null;
  } | null>(null);
  // State, not a ref: the palette's commands are built during render (`react-hooks/refs`).
  const [workspaceElement, setWorkspaceElement] = useState<HTMLDivElement | null>(null);
  const [currentFile, setCurrentFile] = useState<string | null>(null);
  // DiffPane's selection of two or more changed files, for the palette's selection commands (#449).
  const [fileSelection, setFileSelection] = useState<ReportedFileSelection | null>(null);
  const diffSettings = useDiffSettingsContext();
  // The tab's diff views, for the palette's "Wrap lines" / "Show whole file" (#469).
  const currentDiff = useCurrentDiffRegistry();
  const narrow = useNarrowLayout();

  // Populate this tab's state as soon as it mounts. `useAppState` no longer fetches anything on
  // its own — its old `openRepo` method (removed when repo-opening moved out to `useOpenRepos`)
  // used to be what triggered the first `refresh()` right after a repo opened, via
  // `runMutation`'s trailing `await refresh()`. `RepoWorkspace` only ever mounts once a repo is
  // already open (`App`'s `openRepos.openRepos.map`, keyed by `repo.path` so each tab gets
  // exactly one fresh `RepoWorkspace`/`useAppState` instance for its lifetime), so firing this
  // once on mount reproduces that old behavior without needing `useAppState` itself to auto-fetch
  // — which would also fire on every render in hook-level tests that manage refresh manually.
  useEffect(() => {
    void appState.refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The one "repository busy" rule and its reason (#495), shared with the command palette.
  // `operationDisabledReason` is threaded into the sidebar mutation panels (`BranchTree`,
  // `WorktreePanel`, `TagPanel`, `PullRequestPanel`) so their disabled buttons carry a `title`
  // explaining the block instead of just going inert (issue #31/UX-003). Deliberately not a
  // fine-grained per-panel lock — that's the "Large" option the issue called out of scope.
  const { blocked: repositoryOperationDisabled, reason: operationDisabledReason } = operationBlockers(appState.state);
  // Discarding changes and dropping a stash stay allowed during a paused merge, rebase or
  // sequence step (users discard while resolving one); only an action in flight or a transfer
  // blocks them (#521).
  const discardDropBlockedReason = busyReason(appState.state);
  // Re-checked: the repository can become busy while a Drop confirmation is open (#521).
  const dropStash = (index: number): Promise<void> =>
    discardDropBlockedReason !== null ? Promise.resolve() : appState.dropStash(index);

  const { pending, transfer } = appState.state;
  // #448: "Save as patch…" for commits (oldest first), and the commit view's revert/apply/save.
  const savePatchOfCommits = (commitIds: string[]) =>
    void appState.savePatch({ kind: "Commits", commitIds }, commitsPatchName(commitIds, appState.state.commits));
  const diffPanePatchActions: DiffPanePatchActions = {
    blockedReason: operationDisabledReason,
    onApplyCommitChange: (change) => void appState.applyCommitChange(change),
    onSavePatch: (source, suggestedName) => void appState.savePatch(source, suggestedName),
  };
  const { mounted, handleUndo, handleRedo } = useUndoRedoActions(appState, active, pushToast);

  const { paletteOpen, setPaletteOpen, shortcutsOpen, setShortcutsOpen } = useWorkspaceShortcuts({
    active,
    repositoryOperationDisabled,
    onCloseRepoTab,
    repoPath,
    handleUndo,
    handleRedo,
    pending,
    transfer,
  });

  // A stored stash's Apply/Pop and Drop rules, shared by the stash list, the stash view and the
  // palette (#509).
  const storedStashBlockers = stashBlockers(appState.state);

  const { dismissError } = appState;
  const announce = useAnnounce(appState, pushToast);

  // "Fetch automatically" (#440): the repository's override, else the app setting.
  const fetchSettings = useFetchSettingsContext();
  const autoFetchOverride = useSyncExternalStore(subscribeAutoFetchOverride, () => readAutoFetchOverride(repoPath));
  const autoFetchMinutes = effectiveAutoFetchMinutes(autoFetchOverride, fetchSettings.settings.autoFetchMinutes);
  const lastFetchedAt = useSyncExternalStore(subscribeLastFetch, () => lastFetchAt(repoPath));
  const openRemoteDialog = appState.openRemoteDialog;
  const openCredentials = useCallback(
    (remoteName: string) => openRemoteDialog("credentials", remoteName),
    [openRemoteDialog],
  );
  useAutoFetch({
    client,
    repoPath,
    minutes: autoFetchMinutes,
    prune: appState.state.fetchPrune,
    busy: pending || transfer !== null,
    refresh: appState.refresh,
    notify: pushToast,
    openCredentials,
  });
  const {
    terminalAbortRequest,
    setTerminalAbortRequest,
    runPickAction,
    runContinueSequence,
    continueSequence,
    abortSequence,
    abortRebase,
    terminalSequence,
    runTerminalAbort,
    abortTerminalSequence,
    abortMergeOrSequence,
    paletteAbortSequence,
    paletteAbortMerge,
  } = useSequenceActions(appState, pushToast, mounted, announce);
  const currentBranchName = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? appState.state.upstream?.remoteBranch ?? "";
  // Cherry-pick, revert and reset of one commit, shared by the commit menu and the palette
  // (#507): the toast comes from the outcome (#107).
  const cherryPickCommit = (commitId: string, mainline: number | null) =>
    void runPickAction(
      () => appState.cherryPick(commitId, mainline),
      (outcome) => pickToast("CherryPick", outcome),
    );
  const revertCommit = (commitId: string, mainline: number | null) =>
    void runPickAction(
      () => appState.revertCommit(commitId, mainline),
      (outcome) => pickToast("Revert", outcome),
    );
  const resetToCommit = announce("Reset branch", appState.resetToCommit);
  // The dialogs return focus to the list showing the selection (#146): the commit menu passes
  // its list; from the palette, which is gone by then, it is looked up.
  const openCommitDialog = (
    kind: "Cherry-pick" | "Revert" | "Reset",
    commitId: string,
    returnFocusTo?: HTMLElement | null,
  ) => {
    const commit = appState.state.commits.find((candidate) => candidate.id === commitId);
    if (commit === undefined) return;
    setCommitDialog({
      kind,
      commit,
      returnFocusTo:
        returnFocusTo ??
        workspaceElement?.querySelector<HTMLElement>(
          '[role="listbox"][aria-label="Commit history"], [role="listbox"][aria-label="Search results"], [role="listbox"][aria-label="File history commits"]',
        ) ??
        null,
    });
  };
  const commitCommands: CommitCommands = {
    onCherryPick: (commitId) => cherryPickCommit(commitId, null),
    onRevert: (commitId) => revertCommit(commitId, null),
    onChooseParent: (action, commitId) => openCommitDialog(action, commitId),
    onReset: (commitId) => openCommitDialog("Reset", commitId),
  };
  const currentLocalBranch = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null;
  // #508: the stash dialog re-checks this while open; a rebase or sequence can start meanwhile.
  const stashBlocked = stashBlockReason(appState.state);
  // The branch `force_push_current_branch` overwrites on `remoteName`: what push.default resolves
  // for the upstream's remote, else the current branch's own name.
  const forcePushTargetBranch = (remoteName: string) =>
    (appState.state.upstream?.remoteName === remoteName ? upstreamPushBranch(appState.state.upstream) : null) ??
    currentBranchName;
  const pushDialogBranch = appState.state.pushDialog?.localBranch ?? null;
  const createTagDialog = appState.state.createTagDialog;
  const createTagCommit =
    createTagDialog === null ? undefined : appState.state.commits.find((commit) => commit.id === createTagDialog.commitId);
  const mergeDialogBranch = appState.state.mergeDialog?.branchName ?? null;
  const rewordDialog = appState.state.rewordDialog;
  const rewordCommitId = rewordDialog?.commitId ?? null;
  const loadRewordMessage = useCallback(
    () => (rewordCommitId === null ? Promise.resolve("") : client.getCommitMessage(repoPath, rewordCommitId)),
    [client, repoPath, rewordCommitId],
  );
  useOutcomeToasts(appState.state, pushToast);

  // Closing this tab while a transfer/merge/rebase is in progress would orphan it mid-operation
  // — report busy status up so `App`'s `RepoTabs` can disable this tab's close button, the same
  // rule that already disables every other mutating action while this is true.
  useEffect(() => {
    onBusyChange(repoPath, repositoryOperationDisabled);
  }, [repoPath, repositoryOperationDisabled, onBusyChange]);
  // Only the active tab's workspace is mounted (PERF-001), so switching away mid-operation
  // unmounts this component before its "no longer busy" report can ever fire — which would leave
  // the backgrounded tab's close button disabled for good. Clear it on the way out: the worker
  // finishes its current command and exits cleanly, and every paused operation lives on disk
  // (`MERGE_HEAD`, `sequence.json`, `rebase.json`), so `refresh()` re-reads it on remount.
  useEffect(() => () => onBusyChange(repoPath, false), [repoPath, onBusyChange]);

  return (
    // `data-active-repo` marks which workspace is the visible one. `App` only ever mounts the
    // active tab's `RepoWorkspace` now (PERF-001), so in practice exactly one of these exists at
    // a time — but the attribute (and the `active` prop it mirrors) stays: `commands.ts`'s
    // `goToSidebarSection` still scopes its `document.querySelector` lookup to it, and this
    // component's own internal gates (`active &&` below) still need a real boolean rather than an
    // assumption that they're always mounted-implies-active.
    <CurrentDiffContext.Provider value={currentDiff}>
    <div
      ref={setWorkspaceElement}
      style={{ display: active ? "contents" : "none" }}
      data-active-repo={active ? "true" : "false"}
      role="tabpanel"
      id={repoPanelId(repoPath)}
      aria-labelledby={repoTabId(repoPath)}
    >
      <OperationStatusStrip
        merging={appState.state.mergeMessage !== null}
        operation={appState.state.headInfo.operation}
        operationLeftover={appState.state.headInfo.operationLeftover}
        rebaseProgress={appState.state.rebaseProgress}
        rebase={appState.state.headInfo.rebase}
        sequence={appState.state.headInfo.sequence}
        bisect={appState.state.headInfo.bisect}
        terminalSequence={terminalSequence}
        terminalRebase={appState.state.headInfo.terminalRebase}
        onAbortTerminalSequence={abortTerminalSequence}
        pending={appState.state.pending}
        conflictCount={appState.state.status.filter((entry) => entry.kind === "Conflicted").length}
        onAbortMerge={abortMergeOrSequence}
        onAbortRebase={() => void abortRebase()}
        onContinueSequence={continueSequence}
        onAbortSequence={abortSequence}
      />
      <HookRunStrip running={appState.state.hookRun} onCancel={() => void appState.cancelHookRun()} />
      {externalTools.error !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={externalTools.error} onDismiss={externalTools.dismissError} />
        </div>
      )}
      {appState.state.error !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={appState.state.error} onDismiss={appState.dismissError} />
          {appState.state.forcePushOffer !== null && appState.state.forcePushConfirm === null && (
            <button type="button" onClick={() => appState.requestForcePush(appState.state.forcePushOffer!.remoteName)}>
              Force push (with lease)…
            </button>
          )}
          {appState.state.hookFailure !== null && !appState.state.hookFailure.dialogOpen && (
            <button type="button" onClick={appState.openHookFailure}>
              Show hook output…
            </button>
          )}
        </div>
      )}
      {active && appState.state.hookFailure?.dialogOpen === true && (
        <HookFailureDialog
          failure={appState.state.hookFailure}
          onRetry={() => {
            const retry = appState.state.hookFailure!.retry;
            // A Continue's toast comes from its outcome, like a plain Continue's. Dismissed first
            // (as every retry is), also when the sequence has gone meanwhile and nothing runs.
            if (retry.kind === "continueSequence") {
              dismissError();
              runContinueSequence(true);
            }
            // A pick's failure offers no retry (`retryLabel` is null), so this never runs for it.
            else if (retry.kind !== "pick") void announce(retrySuccessLabel(retry), retryWithCurrentMessage)(retry);
          }}
          onClose={appState.closeHookFailure}
        />
      )}
      {active && appState.state.forcePushConfirm !== null && (
        <ConfirmDialog
          ariaLabel="Confirm force push"
          message={
            <>
              <p>
                Overwrites {appState.state.forcePushConfirm.remoteName}/
                {forcePushTargetBranch(appState.state.forcePushConfirm.remoteName)} with your local branch.
              </p>
              {/* #514: the repository can become busy while the confirmation is open. */}
              {operationDisabledReason !== null && <p>{operationDisabledReason}</p>}
            </>
          }
          confirmLabel="Force push"
          confirmDisabled={repositoryOperationDisabled}
          confirmTitle={operationDisabledReason ?? undefined}
          onConfirm={() => {
            if (repositoryOperationDisabled) return;
            void announce("Force pushed", appState.forcePushCurrentBranch)(appState.state.forcePushConfirm!.remoteName);
          }}
          onCancel={appState.cancelForcePush}
        />
      )}
      {active && terminalAbortRequest !== null && (
        <ConfirmDialog
          ariaLabel={terminalAbortConfirm(terminalAbortRequest.sequence).title}
          message={
            <>
              <p>
                <strong>{terminalAbortConfirm(terminalAbortRequest.sequence).title}</strong>
              </p>
              <p>{terminalAbortConfirm(terminalAbortRequest.sequence).body}</p>
            </>
          }
          confirmLabel={terminalAbortConfirm(terminalAbortRequest.sequence).confirmLabel}
          onConfirm={() => {
            const request = terminalAbortRequest;
            setTerminalAbortRequest(null);
            runTerminalAbort(request.sequence, request.headId);
          }}
          onCancel={() => setTerminalAbortRequest(null)}
        />
      )}
      {active && mergeDialogBranch !== null && (
        <MergeDialog
          branchName={mergeDialogBranch}
          currentBranchName={currentBranchName}
          loadOptions={appState.getMergeOptions}
          blockedReason={operationDisabledReason}
          onMerge={(options) => {
            if (repositoryOperationDisabled) return;
            void appState.mergeBranch(mergeDialogBranch, options);
          }}
          onCancel={appState.closeMergeDialog}
        />
      )}
      {active && appState.state.stashDialog !== null && (
        <StashDialog
          paths={appState.state.stashDialog.paths}
          untrackedOnly={
            appState.state.stashDialog.paths.length > 0 &&
            appState.state.stashDialog.paths.every((path) =>
              appState.state.status.some((entry) => entry.path === path && !entry.staged && entry.kind === "New"),
            )
          }
          blockedReason={stashBlocked}
          onStash={(options) => {
            if (stashBlocked !== null) return;
            void announce("Changes stashed", appState.saveStash)(options);
          }}
          onCancel={appState.closeStashDialog}
        />
      )}
      {active && commitDialog !== null && commitDialog.kind !== "Reset" && (
        <ParentPickerDialog
          action={commitDialog.kind}
          commit={commitDialog.commit}
          commits={appState.state.commits}
          returnFocusTo={commitDialog.returnFocusTo}
          blockedReason={operationDisabledReason}
          onConfirm={(mainline) => {
            // Re-checked: the repository can become busy while the dialog is open (#514).
            if (repositoryOperationDisabled) return;
            setCommitDialog(null);
            if (commitDialog.kind === "Cherry-pick") cherryPickCommit(commitDialog.commit.id, mainline);
            else revertCommit(commitDialog.commit.id, mainline);
          }}
          onCancel={() => setCommitDialog(null)}
        />
      )}
      {active && commitDialog !== null && commitDialog.kind === "Reset" && currentLocalBranch !== null && (
        <ResetDialog
          branch={currentLocalBranch}
          shortId={commitDialog.commit.shortId}
          returnFocusTo={commitDialog.returnFocusTo}
          blockedReason={operationDisabledReason}
          onReset={(mode) => {
            if (repositoryOperationDisabled) return;
            setCommitDialog(null);
            void resetToCommit(commitDialog.commit.id, mode);
          }}
          onCancel={() => setCommitDialog(null)}
        />
      )}
      {active && pushDialogBranch !== null && (
        <PushBranchDialog
          localBranch={pushDialogBranch}
          remotes={appState.state.remotes}
          branchUpstream={appState.state.branches.find((branch) => branch.name === pushDialogBranch)?.upstream}
          blockedReason={operationDisabledReason}
          onPush={(remoteName, remoteBranch, setUpstream) => {
            if (repositoryOperationDisabled) return;
            void announce("Push complete", appState.pushBranch)(remoteName, pushDialogBranch, remoteBranch, setUpstream);
          }}
          onCancel={appState.closePushDialog}
        />
      )}
      {active && (
        // Here, not in `BranchTree`: the sidebar stays mounted but `hidden` while collapsed.
        <RemoteDialogs
          remoteDialog={appState.state.remoteDialog}
          remotes={appState.state.remotes}
          remoteUpstreams={appState.state.remoteUpstreams}
          blockedReason={operationDisabledReason}
          onRenameRemote={appState.renameRemote}
          onUpdateRemoteUrls={appState.updateRemoteUrls}
          onRemoveRemote={appState.removeRemote}
          onSaveHttpsCredential={appState.saveHttpsCredential}
          onForgetHttpsCredential={appState.forgetHttpsCredential}
          onSetRemoteAuthMode={appState.setRemoteAuthMode}
          onClose={appState.closeRemoteDialog}
          onRemoteGone={(name) => pushToast(`Remote ${name} was removed or renamed.`)}
        />
      )}
      {active && appState.state.deleteRemoteBranchDialog !== null && (
        <DeleteRemoteBranchDialog
          remotes={appState.state.remotes}
          initialRemote={appState.state.deleteRemoteBranchDialog.remoteName}
          initialBranch={appState.state.deleteRemoteBranchDialog.branchName}
          onListRemoteBranches={appState.listRemoteBranches}
          blockedReason={operationDisabledReason}
          onDelete={(remoteName, branchName) => {
            if (repositoryOperationDisabled) return;
            void announce("Remote branch deleted", appState.deleteRemoteBranch)(remoteName, branchName);
          }}
          onCancel={appState.closeDeleteRemoteBranchDialog}
        />
      )}
      {active && createTagDialog !== null && (
        <CreateTagDialog
          key={createTagDialog.commitId}
          shortId={createTagCommit?.shortId ?? createTagDialog.commitId.slice(0, 7)}
          summary={createTagCommit?.summary ?? null}
          existingTags={appState.state.tags.map((tag) => tag.name)}
          remotes={appState.state.remotes}
          disabled={repositoryOperationDisabled}
          disabledReason={operationDisabledReason}
          onCreate={async (name, message, pushTo) => {
            // With a push to follow, only the push is announced: its toast reports both steps.
            const create = pushTo === null ? announce("Tag created", appState.createTag) : appState.createTag;
            const failure = await create(name, message, createTagDialog.commitId);
            if (failure !== null) return failure;
            appState.closeCreateTagDialog();
            if (pushTo !== null) void announce("Push complete", appState.pushTags)(pushTo, [name]);
            return null;
          }}
          onCancel={appState.closeCreateTagDialog}
        />
      )}
      {active && rewordDialog !== null && (
        <RewordDialog
          key={rewordDialog.commitId}
          shortId={
            appState.state.commits.find((commit) => commit.id === rewordDialog.commitId)?.shortId ??
            rewordDialog.commitId.slice(0, 7)
          }
          loadMessage={loadRewordMessage}
          disabled={repositoryOperationDisabled}
          disabledReason={operationDisabledReason}
          onReword={(message) => {
            const { commitId, onto } = rewordDialog;
            appState.closeRewordDialog();
            void appState.editHistory(commitId, onto, { kind: "Reword", message });
          }}
          onCancel={appState.closeRewordDialog}
        />
      )}
      {active && appState.state.deleteRemoteTagDialog !== null && (
        <DeleteRemoteTagDialog
          tagName={appState.state.deleteRemoteTagDialog.tagName}
          remotes={appState.state.remotes}
          disabled={repositoryOperationDisabled}
          disabledReason={operationDisabledReason}
          onDelete={(remoteName, deleteLocal) =>
            void announce("Remote tag deleted", appState.deleteRemoteTag)(
              remoteName,
              appState.state.deleteRemoteTagDialog?.tagName ?? "",
              deleteLocal,
            )
          }
          onCancel={appState.closeDeleteRemoteTagDialog}
        />
      )}
      {/* Every `Overlay` below is gated on `active` as well as its own open-state. `Overlay`
          calls `dialog.showModal()`, which blocks the whole document (top-layer + `inert`
          outside the dialog) even when this wrapper is `display: none` — so a backgrounded tab
          whose transfer/palette/rebase overlay is open would freeze the visible tab behind an
          invisible modal. The transfer overlay makes that the common case: it opens by itself
          whenever `state.transfer !== null`, i.e. exactly the "start a push, switch tabs"
          flow this feature exists for. Gating unmounts the dialog (releasing the block) while
          backgrounded and loses nothing — `paletteOpen`/`transfer`/`rebaseOnto` all live in
          state this doesn't touch, so switching back re-shows the same overlay. */}
      {active && appState.state.transfer !== null && (
        <Overlay>
          <TransferPanel
            progress={appState.state.transfer}
            // Fire-and-forget: the backend records the cancel request and the transfer's own
            // terminal progress event (errorKind "Cancelled") is what closes this panel.
            onCancel={(operationId) => void client.cancelTransfer(repoPath, operationId)}
          />
        </Overlay>
      )}
      {active && paletteOpen && (
        <Overlay onClose={() => setPaletteOpen(false)}>
          <CommandPalette
            commands={buildCommands(
              { ...appState, abortRebase, abortSequence: paletteAbortSequence, abortMerge: paletteAbortMerge },
              onOpenRepoTab,
              openRepos.filter((repo) => repo.path !== repoPath),
              onSwitchRepoTab,
              panelVisibility.visibility,
              () => setShortcutsOpen(true),
              () => onCloseRepoTab(repoPath),
              {
                settings: diffSettings.settings,
                update: diffSettings.update,
                currentFile,
                openCurrentFileHistory: () => {
                  if (currentFile !== null) fileHistory.open(currentFile);
                },
                requestDiscardCurrentFile: () => {
                  if (currentFile === null) return;
                  appState.selectRow("uncommitted");
                  setDiscardRequest({ kind: "file", path: currentFile });
                },
                requestDiscardAllUnstaged: () => {
                  appState.selectRow("uncommitted");
                  setDiscardRequest({ kind: "all" });
                },
                currentDiff:
                  currentDiff.current === null
                    ? null
                    : {
                        ...currentDiff.current,
                        toggleWrap: currentDiff.toggleWrap,
                        toggleWholeFile: currentDiff.toggleWholeFile,
                      },
                selection: fileSelection,
                requestDiscardSelection: () => {
                  if (fileSelection === null || fileSelection.staged) return;
                  appState.selectRow("uncommitted");
                  setDiscardRequest({ kind: "files", paths: fileSelection.paths });
                },
              },
              appCommands,
              {
                undoLabel: appState.state.undoState.undo,
                redoLabel: appState.state.undoState.redo,
                onUndo: handleUndo,
                onRedo: handleRedo,
              },
              { onSearchCommits: requestSearchFocus },
              {
                onOpenTerminal: externalTools.openTerminal,
                onRevealRepository: () => externalTools.reveal(null),
                onOpenCurrentFileInEditor: () => {
                  if (currentFile !== null) externalTools.openInEditor(currentFile, null);
                },
                onOpenCurrentFileInDiffTool: () => {
                  if (currentFile !== null) externalTools.openDiffTool(currentFile, { kind: "Unstaged" });
                },
              },
              {
                onGoToCommit: (revision) => void goTo.goToRevision(revision),
                onSearchHistory: searchHistory,
                onGoToBranch: (kind, name) => void goTo.goToBranch(kind, name),
                onGoToTag: (name, commitId) => void goTo.goToTag(name, commitId),
                onGoToStash: (commitId) => reveal({ commitId }),
              },
              commitCommands,
            )}
            onRun={() => setPaletteOpen(false)}
          />
        </Overlay>
      )}
      {active && shortcutsOpen && (
        <Overlay onClose={() => setShortcutsOpen(false)}>
          <ShortcutSheet onClose={() => setShortcutsOpen(false)} />
        </Overlay>
      )}
      <SplitView
        storageKey="sidebar-width"
        defaultWidth={260}
        minWidth={200}
        maxWidth={420}
        collapsible
        forceCollapsed={narrow}
        label="Sidebar width"
        left={
          <Sidebar
            panelToggles={[
              { id: "stash", label: "Stashes", visible: panelVisibility.visibility.stash, onToggle: (v) => panelVisibility.setPanelVisible("stash", v) },
              { id: "worktree", label: "Worktrees", visible: panelVisibility.visibility.worktree, onToggle: (v) => panelVisibility.setPanelVisible("worktree", v) },
              { id: "submodule", label: "Submodules", visible: panelVisibility.visibility.submodule, onToggle: (v) => panelVisibility.setPanelVisible("submodule", v) },
              { id: "reflog", label: "Reflog", visible: panelVisibility.visibility.reflog, onToggle: (v) => panelVisibility.setPanelVisible("reflog", v) },
              { id: "tags", label: "Tags", visible: panelVisibility.visibility.tags, onToggle: (v) => panelVisibility.setPanelVisible("tags", v) },
              { id: "pullRequests", label: "Pull requests", visible: panelVisibility.visibility.pullRequests, onToggle: (v) => panelVisibility.setPanelVisible("pullRequests", v) },
            ]}
          >
            <BranchTree
              branches={appState.state.branches}
              createBranchDraft={appState.state.createBranchDraft}
              onSwitchBranch={announce("Switched branch", appState.switchBranch)}
              onCreateBranch={appState.createBranch}
              onDeleteBranch={announce("Deleted branch", appState.deleteBranch)}
              onRenameBranch={appState.renameBranch}
              onOpenCreateBranchDraft={appState.openCreateBranchDraft}
              onCloseCreateBranchDraft={appState.closeCreateBranchDraft}
              onMergeBranch={appState.openMergeDialog}
              operationDisabled={repositoryOperationDisabled}
              operationDisabledReason={operationDisabledReason}
              graphBranchSelection={appState.state.graphBranchSelection}
              onSetGraphBranchSelection={appState.setGraphBranchSelection}
              onClearGraphBranchSelection={appState.clearGraphBranchSelection}
              onShowAllGraphBranches={appState.showAllGraphBranches}
              graphRemoteBranchSelection={appState.state.graphRemoteBranchSelection}
              onSetGraphRemoteBranchSelection={appState.setGraphRemoteBranchSelection}
              onIsolateGraphBranch={appState.isolateGraphBranch}
              remoteBranchesInGraph={Array.from(new Set(appState.state.commits.flatMap((c) => c.remoteBranchRefs)))}
              remotes={appState.state.remotes}
              upstream={appState.state.upstream}
              remoteUpstreams={appState.state.remoteUpstreams}
              onAddRemote={appState.addRemote}
              onRenameRemote={appState.renameRemote}
              onUpdateRemoteUrls={appState.updateRemoteUrls}
              onRemoveRemote={appState.removeRemote}
              onSaveHttpsCredential={appState.saveHttpsCredential}
              onForgetHttpsCredential={appState.forgetHttpsCredential}
              onSetRemoteAuthMode={appState.setRemoteAuthMode}
              onSetUpstream={appState.setCurrentUpstream}
              onClearUpstream={appState.clearCurrentUpstream}
              onListRemoteBranches={appState.listRemoteBranches}
              onFetchRemote={announce("Fetch complete", appState.fetchRemote)}
              onPushCurrentBranch={announce("Push complete", appState.pushCurrentBranch)}
              onPull={appState.pullCurrentUpstream}
              pendingPull={appState.state.pendingPull}
              pullOutcome={appState.state.pullOutcome}
              onMergePull={async (upstreamRef) => {
                appState.clearPendingPull();
                await appState.mergePull(upstreamRef);
              }}
              onRebasePull={(upstreamRef) => {
                appState.clearPendingPull();
                appState.openRebasePlanner(upstreamRef);
              }}
              onCancelPull={appState.clearPendingPull}
              addRemoteDraftOpen={appState.state.addRemoteDraftOpen}
              onOpenAddRemoteDraft={appState.openAddRemoteDraft}
              onCloseAddRemoteDraft={appState.closeAddRemoteDraft}
              onOpenPushDialog={appState.openPushDialog}
              onOpenDeleteRemoteBranch={appState.openDeleteRemoteBranchDialog}
              onCompareBranch={compare.compareBranch}
              onGoToBranch={(kind, name) => void goTo.goToBranch(kind, name)}
              refreshGeneration={appState.state.refreshGeneration}
              remoteDialog={appState.state.remoteDialog}
              onRemoteDialogChange={(dialog) =>
                dialog === null ? appState.closeRemoteDialog() : appState.openRemoteDialog(dialog.kind, dialog.remoteName)
              }
            />
            {panelVisibility.visibility.stash && (
              <StashPanel
                stashes={appState.state.stashes}
                onSelectRow={selectRowClosingCompare}
                onApplyStash={appState.applyStash}
                onPopStash={appState.popStash}
                onDropStash={dropStash}
                restoreIndex={appState.state.stashRestoreIndex}
                onSetRestoreIndex={appState.setStashRestoreIndex}
                blockers={storedStashBlockers}
              />
            )}
            {panelVisibility.visibility.worktree && (
              <WorktreePanel
                worktrees={appState.state.worktrees}
                branches={appState.state.branches}
                onOpenWorktree={onOpenRepoTab}
                onCreateWorktree={appState.createWorktree}
                onRemoveWorktree={appState.removeWorktree}
                onPruneWorktrees={appState.pruneWorktrees}
                operationDisabled={repositoryOperationDisabled}
                operationDisabledReason={operationDisabledReason}
              />
            )}
            {panelVisibility.visibility.submodule && (
              <SubmodulePanel
                submodules={appState.state.submodules}
                onInit={appState.initSubmodule}
                onUpdate={appState.updateSubmodule}
                onOpenInNewTab={(workdir) => void onOpenRepoTab(workdir)}
                operationDisabled={repositoryOperationDisabled}
                operationDisabledReason={operationDisabledReason}
              />
            )}
            {panelVisibility.visibility.reflog && (
              <ReflogPanel
                references={appState.state.reflogRefs}
                selectedReference={appState.state.selectedReflogReference}
                entries={appState.state.reflog}
                onSelectReference={appState.selectReflogReference}
                onRestore={appState.restoreReflogEntry}
                operationDisabled={repositoryOperationDisabled}
              />
            )}
            {panelVisibility.visibility.tags && (
              <TagPanel
                tags={appState.state.tags}
                remotes={appState.state.remotes}
                onDelete={appState.deleteTag}
                onDeleteFromRemote={appState.openDeleteRemoteTagDialog}
                onPush={appState.pushTags}
                onPushTag={(remoteName, tag) => void announce("Push complete", appState.pushTags)(remoteName, [tag])}
                onBranchFromCommit={appState.openCreateBranchDraft}
                onCheckoutCommit={announce("Checked out commit", appState.checkoutCommit)}
                onGoToTag={(tag) => void goTo.goToTag(tag.name, tag.commitId)}
                pushDisabled={repositoryOperationDisabled}
                operationDisabledReason={operationDisabledReason}
              />
            )}
            {panelVisibility.visibility.pullRequests && (
              <PullRequestPanel
                forgeRepositories={appState.state.forgeRepositories}
                pullRequests={appState.state.pullRequests}
                onListPullRequests={appState.listPullRequests}
                onSaveForgeToken={appState.saveForgeToken}
                onForgetForgeToken={appState.forgetForgeToken}
                onCreatePullRequest={appState.createPullRequest}
                onOpenExternalUrl={appState.openExternalUrl}
                branches={appState.state.branches}
                operationDisabled={repositoryOperationDisabled}
                operationDisabledReason={operationDisabledReason}
              />
            )}
          </Sidebar>
        }
        right={
          <SplitView
            storageKey="history-diff-width"
            defaultWidth={420}
            minWidth={280}
            maxWidth={800}
            label="History and diff width"
            left={
              <>
              <SyncBar
                remotes={appState.state.remotes}
                upstream={appState.state.upstream}
                detachedAt={appState.state.headInfo.detachedAt}
                operationDisabled={repositoryOperationDisabled}
                operationDisabledReason={operationDisabledReason}
                onFetch={appState.fetchRemote}
                onPull={appState.pullCurrentUpstream}
                onPush={appState.pushCurrentBranch}
                prune={effectiveFetchPrune(appState.state.fetchPrune, appState.state.remotes, appState.state.upstream)}
                onFetchAll={announce("Fetch complete", appState.fetchAllRemotes)}
                onTogglePrune={appState.setFetchPrune}
                lastFetchedAt={lastFetchedAt}
                autoFetchMinutes={autoFetchMinutes}
                onPublish={() => {
                  const current = appState.state.branches.find((branch) => branch.isCurrent);
                  if (current !== undefined) appState.openPushDialog(current.name);
                }}
                leading={
                  <UndoRedoButtons
                    undoLabel={appState.state.undoState.undo}
                    redoLabel={appState.state.undoState.redo}
                    disabled={pending || transfer !== null}
                    disabledReason={operationDisabledReason}
                    onUndo={handleUndo}
                    onRedo={handleRedo}
                  />
                }
              />
              {goTo.notice !== null && <GoToNotice notice={goTo.notice} onDismiss={goTo.dismissNotice} />}
              {fileHistory.history !== null ? (
                <FileHistoryList
                  history={fileHistory.history}
                  selectedRow={appState.state.selectedRow}
                  onSelectRow={selectRowClosingCompare}
                  onClose={fileHistory.close}
                  onLoadMore={fileHistory.loadMore}
                  onRetry={fileHistory.retry}
                />
              ) : (
                <>
                  <CommitSearchBar
                    query={commitSearch.query}
                    onQueryChange={commitSearch.setQuery}
                    onClose={commitSearch.close}
                    onFocusResults={() => searchResultsRef.current?.focus()}
                    focusRequested={searchFocusRequested}
                    onFocusHandled={clearSearchFocusRequest}
                    error={commitSearch.results?.queryError ?? null}
                    recentSearches={commitSearch.recentSearches}
                    onRemoveRecentSearch={commitSearch.removeRecentSearch}
                  />
                  {commitSearch.results !== null ? (
                    <CommitSearchResults
                      results={commitSearch.results}
                      selectedRow={appState.state.selectedRow}
                      onSelectRow={selectRowClosingCompare}
                      onClose={commitSearch.close}
                      onLoadMore={commitSearch.loadMore}
                      onRetry={commitSearch.retry}
                      onRequestSearch={requestSearchFocus}
                      listRef={searchResultsRef}
                    />
                  ) : (
                    <CommitGraph
                      onRequestSearch={requestSearchFocus}
                      onCompareCommits={compare.compareCommits}
                      onCompareWithWorkingTree={compare.compareWorkingTree}
                      onSavePatch={savePatchOfCommits}
                      hasMore={appState.state.hasMoreHistory}
                      onLoadMore={() => void appState.loadMoreHistory()}
                      status={appState.state.status}
                      commits={appState.state.commits}
                      selectedRow={appState.state.selectedRow}
                      operationsBlocked={repositoryOperationDisabled}
                      operationsBlockedReason={operationDisabledReason}
                      onSelectRow={selectRowClosingCompare}
                      onBranchFromCommit={appState.openCreateBranchDraft}
                      onRebaseFromCommit={appState.openRebasePlanner}
                      onRebaseSelection={appState.openRebaseSelection}
                      headInfo={appState.state.headInfo}
                      currentBranch={appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null}
                      onCherryPick={cherryPickCommit}
                      onRevert={revertCommit}
                      onCherryPickMany={(ids) =>
                        void runPickAction(
                          () => appState.cherryPickMany(ids),
                          (outcome) => sequenceToast("CherryPick", outcome),
                        )
                      }
                      onRevertMany={(ids) =>
                        void runPickAction(
                          () => appState.revertMany(ids),
                          (outcome) => sequenceToast("Revert", outcome),
                        )
                      }
                      onCheckoutCommit={announce("Checked out commit", appState.checkoutCommit)}
                      onChooseParent={openCommitDialog}
                      onRequestReset={(commitId, returnFocusTo) => openCommitDialog("Reset", commitId, returnFocusTo)}
                      onEditHeadMessage={() => {
                        appState.selectRow("uncommitted");
                        appState.setAmendMode(true);
                        setFocusCommitMessage(true);
                      }}
                      onEditHistory={(commitId, onto, request) => void appState.editHistory(commitId, onto, request)}
                      onRewordCommit={appState.openRewordDialog}
                      graphRemoteBranchSelection={appState.state.graphRemoteBranchSelection}
                      remotes={appState.state.remotes}
                      onPushTag={(remoteName, tag) => void announce("Push complete", appState.pushTags)(remoteName, [tag])}
                      onDeleteTag={appState.deleteTag}
                      onCreateTag={appState.openCreateTagDialog}
                      onDeleteTagFromRemote={appState.openDeleteRemoteTagDialog}
                      revealRequest={revealRequest}
                    />
                  )}
                </>
              )}
              </>
            }
            right={
              compare.compare !== null ? (
                <CompareDiffPane
                  repoPath={repoPath}
                  client={client}
                  compare={compare.compare}
                  commits={diffPaneCommits}
                  refreshGeneration={appState.state.refreshGeneration}
                  onSwap={compare.swap}
                  onClose={compare.close}
                  onRetry={compare.retry}
                  onSelectRow={selectRowClosingCompare}
                  onOpenFileHistory={fileHistory.open}
                  onOpenLineHistory={fileHistory.openLines}
                  onOpenFolderHistory={openFolderHistory}
                  externalTools={externalTools}
                />
              ) : (
                <DiffPane
                  repoPath={repoPath}
                  client={client}
                  selectedRow={appState.state.selectedRow}
                  commits={diffPaneCommits}
                  branchCommits={appState.state.commits}
                  fileHistoryFocus={historyEntry === null ? null : { path: historyEntry.path, oldPath: historyEntry.oldPath }}
                  stashes={appState.state.stashes}
                  stashActions={{
                    onApply: appState.applyStash,
                    onPop: appState.popStash,
                    onDrop: (index) => void dropStash(index),
                    restoreIndex: appState.state.stashRestoreIndex,
                    onSetRestoreIndex: appState.setStashRestoreIndex,
                    blockers: storedStashBlockers,
                  }}
                  onOpenFileHistory={fileHistory.open}
                  onOpenLineHistory={fileHistory.openLines}
                  onOpenFolderHistory={openFolderHistory}
                  externalTools={externalTools}
                  onRestoreFileVersion={(commitId, path) =>
                    announce(`Restored ${path} from ${commitId.slice(0, 7)}`, appState.restoreFileVersion)(commitId, path)
                  }
                  restoreBlockedReason={operationDisabledReason}
                  patchActions={diffPanePatchActions}
                  onMarkResolved={appState.markResolved}
                  onResolveWholeFile={(path, side) => void appState.resolveWholeFile(path, side)}
                  status={appState.state.status}
                  refreshGeneration={appState.state.refreshGeneration}
                  discardRequest={discardRequest}
                  onDiscardRequestChange={setDiscardRequest}
                  onCurrentFileChange={setCurrentFile}
                  initialCommitDraft={getCommitDraft(repoPath)}
                  onCommitDraftChange={handleCommitDraftChange}
                  onStageFile={appState.stageFile}
                  onUnstageFile={appState.unstageFile}
                  onStageAllFiles={appState.stageAllFiles}
                  onUnstageAllFiles={appState.unstageAllFiles}
                  onStageHunk={appState.stageHunk}
                  onUnstageHunk={appState.unstageHunk}
                  onDiscardHunk={appState.discardHunk}
                  onStageLines={(p, o, n, c, idx, shown) => announce(linesToast("Staged", idx.length), appState.stageLines)(p, o, n, c, idx, shown)}
                  onUnstageLines={(p, o, n, c, idx, shown) => announce(linesToast("Unstaged", idx.length), appState.unstageLines)(p, o, n, c, idx, shown)}
                  onDiscardLines={(p, o, n, c, idx, shown) => announce(linesToast("Discarded", idx.length), appState.discardLines)(p, o, n, c, idx, shown)}
                  discardBlockedReason={discardDropBlockedReason}
                  // Re-checked: the repository can become busy while a confirmation is open (#521).
                  onDiscardFile={(path) =>
                    discardDropBlockedReason !== null
                      ? undefined
                      : announce(`Discarded changes to ${path}`, appState.discardFile)(path)
                  }
                  onDiscardAllUnstaged={(paths) => {
                    if (discardDropBlockedReason === null) void announce("Discarded all unstaged changes", appState.discardAllUnstaged)(paths);
                  }}
                  onDiscardFiles={(paths) => {
                    if (discardDropBlockedReason !== null) return;
                    void announce(`Discarded changes to ${paths.length} ${paths.length === 1 ? "file" : "files"}`, appState.discardAllUnstaged)(paths);
                  }}
                  onIgnorePaths={(patterns) =>
                    announce(`Added ${patterns.length} ${patterns.length === 1 ? "pattern" : "patterns"} to .gitignore`, appState.ignorePaths)(patterns)
                  }
                  onSelectionChange={setFileSelection}
                  selectionBusyReason={appState.state.pending ? "Another action is in progress." : null}
                  onIgnorePath={(pattern) => announce(`Added ${pattern} to .gitignore`, appState.ignorePath)(pattern)}
                  onCommit={(message, trailers) =>
                    announce("Committed", commitAndClearDraft)(message, appState.state.skipHooks, trailers)
                  }
                  onSaveStash={announce("Changes stashed", () => appState.saveStash())}
                  onOpenStashDialog={appState.openStashDialog}
                  onSelectRow={appState.selectRow}
                  onResolveConflict={appState.resolveConflict}
                  onResolveAddDeleteConflict={appState.resolveAddDeleteConflict}
                  mergeMessage={appState.state.mergeMessage}
                  onAbortMerge={abortMergeOrSequence}
                  onSequenceContinue={continueSequence}
                  onSequenceAbort={abortSequence}
                  sequencePending={appState.state.pending}
                  amendMode={appState.state.amendMode}
                  headInfo={appState.state.headInfo}
                  onSetAmendMode={appState.setAmendMode}
                  onAmendCommit={(message, options) =>
                    announce("Amended commit", amendAndRecord)(message, appState.state.skipHooks, options)
                  }
                  focusCommitMessage={focusCommitMessage}
                  onCommitMessageFocused={clearFocusCommitMessage}
                  commitHooks={{
                    skip: appState.state.skipHooks,
                    onToggleSkip: appState.setSkipHooks,
                    // Only a commit's, amend's or Continue's hooks; the others show in the
                    // "Hook status" strip (#185).
                    running: appState.state.hookRun?.source === "Commit" ? appState.state.hookRun : null,
                    onCancel: () => void appState.cancelHookRun(),
                  }}
                  clearCommitMessage={appState.state.clearCommitMessage}
                  onCommitMessageCleared={appState.clearCommitMessageHandled}
                  rebaseProgress={appState.state.rebaseProgress}
                  onRebaseContinue={appState.rebaseContinue}
                  onRebaseAbort={() => void abortRebase()}
                />
              )
            }
          />
        }
      />
      {active && appState.state.rebaseOnto !== null && (
        <Overlay onClose={appState.closeRebasePlanner}>
          <RebasePlanner
            repoPath={repoPath}
            client={client}
            onto={appState.state.rebaseOnto}
            onStartRebase={(onto, plan) => {
              // Re-checked: the repository can become busy while the planner is open (#521).
              if (repositoryOperationDisabled) return;
              void appState.startRebase(onto, plan);
            }}
            onCancel={appState.closeRebasePlanner}
            blockedReason={operationDisabledReason}
            presetActions={appState.state.rebasePreset ?? undefined}
          />
        </Overlay>
      )}
    </div>
    </CurrentDiffContext.Provider>
  );
}
