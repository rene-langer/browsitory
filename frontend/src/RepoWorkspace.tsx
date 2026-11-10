import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import type { CommitDraft } from "./components/CommitBox";
import { folderSearchQuery } from "./lib/folderHistory";
import { repoPanelId, repoTabId } from "./components/repoTabIds";
import { OperationStatusStrip } from "./components/OperationStatusStrip";
import { HookRunStrip } from "./components/HookRunStrip";
import type { ToastAction } from "./state/useToasts";
import { InlineError } from "./components/primitives/InlineError";
import { SplitView } from "./components/primitives/SplitView";
import type { RepoClient } from "./ipc/RepoClient";
import { buildCommands, type AppCommands, type CommitCommands } from "./lib/commands";
import { useExternalTools } from "./state/useExternalTools";
import {
  effectiveAutoFetchMinutes,
  lastFetchAt,
  readAutoFetchOverride,
  subscribeAutoFetchOverride,
  subscribeLastFetch,
} from "./lib/autoFetch";
import { useAutoFetch } from "./state/useAutoFetch";
import { useFetchSettingsContext } from "./state/useFetchSettings";
import { useAppState } from "./state/useAppState";
import { useFileHistory } from "./state/useFileHistory";
import { busyReason, operationBlockers, pickToast } from "./lib/operationStatus";
import { stashBlockers } from "./lib/actions/stashActions";
import { useCommitSearch } from "./state/useCommitSearch";
import { useCompare } from "./state/useCompare";
import { CurrentDiffContext, useCurrentDiffRegistry } from "./state/useCurrentDiff";
import { useGoTo } from "./state/useGoTo";
import type { SelectedRow } from "./state/useAppState";
import { useDiffSettingsContext } from "./state/useDiffSettings";
import type { DiffPanePatchActions, DiscardRequest, ReportedFileSelection } from "./components/DiffPane";
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
import { WorkspaceActionsContext, type WorkspaceActions } from "./state/useWorkspaceActions";
import { WorkspaceDialogs, type CommitDialogRequest } from "./WorkspaceDialogs";
import { WorkspaceMain } from "./WorkspaceMain";
import { WorkspaceSidebar } from "./WorkspaceSidebar";

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
    appState.state.graphSelection,
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
  const [commitDialog, setCommitDialog] = useState<CommitDialogRequest | null>(null);
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
  // The commit view's revert/apply/save (#448).
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

  const announce = useAnnounce(appState, pushToast);

  // "Fetch automatically" (#440): the repository's override, else the app setting.
  const fetchSettings = useFetchSettingsContext();
  const autoFetchOverride = useSyncExternalStore(subscribeAutoFetchOverride, () => readAutoFetchOverride(repoPath));
  const autoFetchMinutes = effectiveAutoFetchMinutes(autoFetchOverride, fetchSettings.settings.autoFetchMinutes);
  const lastFetchedAt = useSyncExternalStore(subscribeLastFetch, () => lastFetchAt(repoPath));
  const { openDialog } = appState;
  const openCredentials = useCallback(
    (remoteName: string) => openDialog({ kind: "remote", action: "credentials", remoteName }),
    [openDialog],
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
  useOutcomeToasts(appState.state, pushToast);

  // The palette's commands, built only while it is open.
  const paletteCommands = () =>
    buildCommands(
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
        requestDeleteUntracked: () => {
          appState.selectRow("uncommitted");
          setDiscardRequest({ kind: "untracked" });
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
    );

  // What the diff pane's views and the commit dock can do (#491), read from
  // `WorkspaceActionsContext` instead of being passed down through every level.
  const workspaceActions: WorkspaceActions = {
    onSelectRow: appState.selectRow,
    onStageFile: appState.stageFile,
    onUnstageFile: appState.unstageFile,
    onStageAllFiles: appState.stageAllFiles,
    onUnstageAllFiles: appState.unstageAllFiles,
    onStageHunk: appState.stageHunk,
    onUnstageHunk: appState.unstageHunk,
    onDiscardHunk: appState.discardHunk,
    onStageLines: (p, o, n, c, idx, shown) => announce(linesToast("Staged", idx.length), appState.stageLines)(p, o, n, c, idx, shown),
    onUnstageLines: (p, o, n, c, idx, shown) => announce(linesToast("Unstaged", idx.length), appState.unstageLines)(p, o, n, c, idx, shown),
    onDiscardLines: (p, o, n, c, idx, shown) => announce(linesToast("Discarded", idx.length), appState.discardLines)(p, o, n, c, idx, shown),
    // Re-checked: the repository can become busy while a confirmation is open (#521).
    onDiscardFile: (path) =>
      discardDropBlockedReason !== null
        ? undefined
        : announce(`Discarded changes to ${path}`, appState.discardFile)(path),
    onDiscardAllUnstaged: (paths) => {
      if (discardDropBlockedReason === null) void announce("Discarded all unstaged changes", appState.discardAllUnstaged)(paths);
    },
    onDiscardFiles: (paths) => {
      if (discardDropBlockedReason !== null) return;
      void announce(`Discarded changes to ${paths.length} ${paths.length === 1 ? "file" : "files"}`, appState.discardAllUnstaged)(paths);
    },
    // `discardAllUnstaged` deletes the untracked paths it is given (#622).
    onDeleteUntracked: (paths) => {
      if (discardDropBlockedReason !== null) return;
      void announce(`Deleted ${paths.length} untracked ${paths.length === 1 ? "file" : "files"}`, appState.discardAllUnstaged)(paths);
    },
    onIgnorePath: (pattern) => announce(`Added ${pattern} to .gitignore`, appState.ignorePath)(pattern),
    onIgnorePaths: (patterns) =>
      announce(`Added ${patterns.length} ${patterns.length === 1 ? "pattern" : "patterns"} to .gitignore`, appState.ignorePaths)(patterns),
    onResolveConflict: appState.resolveConflict,
    onResolveAddDeleteConflict: appState.resolveAddDeleteConflict,
    onMarkResolved: appState.markResolved,
    onResolveWholeFile: (path, side) => void appState.resolveWholeFile(path, side),
    onCommit: (message, trailers) => announce("Committed", commitAndClearDraft)(message, appState.state.skipHooks, trailers),
    onAmendCommit: (message, options) => announce("Amended commit", amendAndRecord)(message, appState.state.skipHooks, options),
    onSetAmendMode: appState.setAmendMode,
    onAbortMerge: abortMergeOrSequence,
    onSaveStash: announce("Changes stashed", () => appState.saveStash()),
    onOpenStashDialog: (paths) => openDialog({ kind: "stash", paths }),
    onRebaseContinue: appState.rebaseContinue,
    onRebaseAbort: () => void abortRebase(),
    onSequenceContinue: continueSequence,
    onSequenceAbort: abortSequence,
    onCommitDraftChange: handleCommitDraftChange,
    onCommitMessageFocused: clearFocusCommitMessage,
    onCommitMessageCleared: appState.clearCommitMessageHandled,
    commitHooks: {
      skip: appState.state.skipHooks,
      onToggleSkip: appState.setSkipHooks,
      // Only a commit's, amend's or Continue's hooks; the others show in the "Hook status" strip
      // (#185).
      running: appState.state.hookRun?.source === "Commit" ? appState.state.hookRun : null,
      onCancel: () => void appState.cancelHookRun(),
    },
    onOpenFileHistory: fileHistory.open,
    onOpenLineHistory: fileHistory.openLines,
    onOpenFolderHistory: openFolderHistory,
    externalTools,
    onRestoreFileVersion: (commitId, path) =>
      announce(`Restored ${path} from ${commitId.slice(0, 7)}`, appState.restoreFileVersion)(commitId, path),
    patchActions: diffPanePatchActions,
    stashActions: {
      onApply: appState.applyStash,
      onPop: appState.popStash,
      onDrop: (index) => void dropStash(index),
      restoreIndex: appState.state.stashRestoreIndex,
      onSetRestoreIndex: appState.setStashRestoreIndex,
      blockers: storedStashBlockers,
    },
    onCurrentFileChange: setCurrentFile,
    onSelectionChange: setFileSelection,
  };

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
    <WorkspaceActionsContext.Provider value={workspaceActions}>
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
          {appState.state.forcePushOffer !== null && appState.state.dialog?.kind !== "forcePush" && (
            <button
              type="button"
              onClick={() => openDialog({ kind: "forcePush", remoteName: appState.state.forcePushOffer!.remoteName })}
            >
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
      <WorkspaceDialogs
        active={active}
        repoPath={repoPath}
        client={client}
        appState={appState}
        announce={announce}
        pushToast={pushToast}
        operationDisabled={repositoryOperationDisabled}
        operationDisabledReason={operationDisabledReason}
        runContinueSequence={runContinueSequence}
        retryWithCurrentMessage={retryWithCurrentMessage}
        terminalAbortRequest={terminalAbortRequest}
        onTerminalAbortRequestChange={setTerminalAbortRequest}
        runTerminalAbort={runTerminalAbort}
        commitDialog={commitDialog}
        onCommitDialogChange={setCommitDialog}
        onCherryPick={cherryPickCommit}
        onRevert={revertCommit}
        onReset={resetToCommit}
        paletteOpen={paletteOpen}
        onPaletteOpenChange={setPaletteOpen}
        paletteCommands={paletteCommands}
        shortcutsOpen={shortcutsOpen}
        onShortcutsOpenChange={setShortcutsOpen}
      />
      <SplitView
        storageKey="sidebar-width"
        defaultWidth={260}
        minWidth={200}
        maxWidth={420}
        collapsible
        forceCollapsed={narrow}
        label="Sidebar width"
        left={
          <WorkspaceSidebar
            appState={appState}
            announce={announce}
            operationDisabled={repositoryOperationDisabled}
            operationDisabledReason={operationDisabledReason}
            panelVisibility={panelVisibility}
            compare={compare}
            goTo={goTo}
            onSelectRow={selectRowClosingCompare}
            onDropStash={dropStash}
            stashBlockers={storedStashBlockers}
            onOpenRepoTab={onOpenRepoTab}
          />
        }
        right={
          <WorkspaceMain
            repoPath={repoPath}
            client={client}
            appState={appState}
            announce={announce}
            operationDisabled={repositoryOperationDisabled}
            operationDisabledReason={operationDisabledReason}
            lastFetchedAt={lastFetchedAt}
            autoFetchMinutes={autoFetchMinutes}
            onUndo={handleUndo}
            onRedo={handleRedo}
            goTo={goTo}
            fileHistory={fileHistory}
            commitSearch={commitSearch}
            compare={compare}
            searchFocusRequested={searchFocusRequested}
            onSearchFocusHandled={clearSearchFocusRequest}
            onRequestSearch={requestSearchFocus}
            onSelectRow={selectRowClosingCompare}
            revealRequest={revealRequest}
            onCherryPick={cherryPickCommit}
            onRevert={revertCommit}
            runPickAction={runPickAction}
            onOpenCommitDialog={openCommitDialog}
            onEditHeadMessage={() => {
              appState.selectRow("uncommitted");
              appState.setAmendMode(true);
              setFocusCommitMessage(true);
            }}
            onOpenFolderHistory={openFolderHistory}
            externalTools={externalTools}
            discardRequest={discardRequest}
            onDiscardRequestChange={setDiscardRequest}
            initialCommitDraft={getCommitDraft(repoPath)}
            discardBlockedReason={discardDropBlockedReason}
            focusCommitMessage={focusCommitMessage}
          />
        }
      />
    </div>
    </WorkspaceActionsContext.Provider>
    </CurrentDiffContext.Provider>
  );
}
