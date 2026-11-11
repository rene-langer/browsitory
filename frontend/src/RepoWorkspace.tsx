import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { CommitDraft } from "./components/CommitBox";
import { folderSearchQuery } from "./lib/folderHistory";
import { repoPanelId, repoTabId } from "./components/repoTabIds";
import { OperationStatusStrip } from "./components/OperationStatusStrip";
import { HookRunStrip } from "./components/HookRunStrip";
import type { PushToast } from "./state/useToasts";
import { InlineError } from "./components/primitives/InlineError";
import { SplitView, type SplitViewLeftState, type SplitViewRequest } from "./components/primitives/SplitView";
import { RepoToolbar } from "./components/RepoToolbar";
import { RepoActivity } from "./components/RepoActivity";
import type { GraphCommit, RepoClient, StatusEntry } from "./ipc/RepoClient";
import { buildPaletteCommands, SWITCH_BRANCH_PICKER_ID, type AppCommands, type CommitCommands } from "./lib/commands";
import { effectiveFetchPrune } from "./lib/fetchPrune";
import { readRecentBranches, recordRecentBranch } from "./lib/recentBranches";
import { useExternalTools } from "./state/useExternalTools";
import { useRootFiles } from "./state/useRootFiles";
import { rootFileItems } from "./lib/rootFiles";
import {
  effectiveAutoFetchMinutes,
  lastFetchAt,
  readAutoFetchOverride,
  subscribeAutoFetchOverride,
  subscribeLastFetch,
} from "./lib/autoFetch";
import { useAutoFetch } from "./state/useAutoFetch";
import { useExternalChanges } from "./state/useExternalChanges";
import { useScrollMemory } from "./state/useScrollMemory";
import { useFetchSettingsContext } from "./state/useFetchSettings";
import { useAppState } from "./state/useAppState";
import { useFileHistory } from "./state/useFileHistory";
import { busyReason, operationBlockers, pickToast, stashBlockReason } from "./lib/operationStatus";
import { stashBlockers } from "./lib/actions/stashActions";
import { useCommitSearch } from "./state/useCommitSearch";
import { useCompare } from "./state/useCompare";
import { CurrentDiffContext, useCurrentDiffRegistry } from "./state/useCurrentDiff";
import { MergeEditorContext, useMergeEditorRegistry } from "./state/useCurrentMergeEditor";
import { useGoTo } from "./state/useGoTo";
import { useCommitNavigation } from "./state/useCommitNavigation";
import type { VisitKind } from "./lib/navigationHistory";
import type { SelectedRow } from "./state/useAppState";
import { useDiffSettingsContext } from "./state/useDiffSettings";
import type { DiffPanePatchActions, DiscardRequest, ReportedFileSelection } from "./components/DiffPane";
import type { OpenRepo } from "./state/useOpenRepos";
import { useSidebarPanelVisibility } from "./state/useSidebarPanelVisibility";
import { useSidebarReveal } from "./state/useSidebarReveal";
import styles from "./App.module.css";
import { useAnnounce } from "./state/useAnnounce";
import { CopyToClipboardContext, useClipboardFeedback } from "./state/useCopyToClipboard";
import { useEarlierErrors } from "./state/useEarlierErrors";
import { useCommitDraftActions } from "./state/useCommitDraftActions";
import { useCommitFlow } from "./state/useCommitFlow";
import { useNarrowLayout } from "./state/useNarrowLayout";
import { useRemoteBranchList } from "./state/useRemoteBranchList";
import { usePullStrategy } from "./state/usePullStrategy";
import { useStashSwitchToasts } from "./state/useStashSwitchToasts";
import { useOutcomeToasts } from "./state/useOutcomeToasts";
import { useSequenceActions } from "./state/useSequenceActions";
import { useUndoRedoActions } from "./state/useUndoRedoActions";
import { useWorkspaceShortcuts } from "./state/useWorkspaceShortcuts";
import { runShortcutCommand } from "./lib/shortcutCommands";
import { onHistoryRequest, onWorkspaceRequest, takeHistoryRequest } from "./lib/workspaceRequests";
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
  onStatusChange,
}: {
  repoPath: string;
  client: RepoClient;
  active: boolean;
  // `Promise<void>` rather than the plan's `void`: `WorktreePanel`'s `onOpenWorktree` is typed
  // `(path: string) => Promise<void>`, and this is the only value passed to it. It stays
  // assignable to `TabCommands.onOpenRepoTab`'s `(path: string) => void` (Task 8).
  onOpenRepoTab: (path: string) => Promise<void>;
  onBusyChange: (repoPath: string, busy: boolean) => void;
  openRepos: OpenRepo[];
  onSwitchRepoTab: (path: string) => void;
  onCloseRepoTab: (path: string) => void;
  // `App`'s per-repo commit-message drafts (see `useCommitDrafts`): `App` unmounts the
  // workspaces of tabs not shown for a while (#642), so `CommitBox`'s own state can't be relied
  // on to outlive a tab switch.
  getCommitDraft: (repoPath: string) => CommitDraft | undefined;
  onCommitDraftChange: (repoPath: string, draft: CommitDraft) => void;
  appCommands?: AppCommands;
  // The single app-level toast stack (`App` owns the one `ToastRegion`).
  pushToast: PushToast;
  // The live status, for the tab strip's "uncommitted changes" marker (#613).
  onStatusChange?: (repoPath: string, status: StatusEntry[]) => void;
}) {
  const appState = useAppState(client, repoPath);
  // Reported once loaded: the initial empty list is no answer yet.
  const initialStatus = useRef(appState.state.status);
  const liveStatus = appState.state.status;
  useEffect(() => {
    if (liveStatus === initialStatus.current) return;
    onStatusChange?.(repoPath, liveStatus);
  }, [liveStatus, onStatusChange, repoPath]);
  const fileHistory = useFileHistory(
    client,
    repoPath,
    appState.state.refreshGeneration,
    appState.state.selectedRow,
    appState.selectRow,
  );
  // "Show File History" / "Show Line History in Browsitory" from VSCode's menus (#678): taken
  // when this workspace mounts (the request may have opened its tab) or when one arrives.
  const { open: openFileHistory, openLines: openLineHistory } = fileHistory;
  useEffect(() => {
    if (!active) return;
    const take = () => {
      const request = takeHistoryRequest(repoPath);
      if (request === null) return;
      if (request.lines === undefined) openFileHistory(request.path);
      else openLineHistory(request.path, "HEAD", request.lines.startLine, request.lines.endLine);
    };
    take();
    return onHistoryRequest(take);
  }, [active, repoPath, openFileHistory, openLineHistory]);
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
  const rootFiles = useRootFiles(client, repoPath, appState.state.refreshGeneration);
  // A click on a row closes an open compare even when it is the row already selected (the
  // compare otherwise only closes when `selectedRow` changes), so clicking a stash or commit
  // always shows it.
  const closeCompare = compare.close;
  const selectRow = appState.selectRow;
  const selectRowClosingCompare = useCallback(
    (row: SelectedRow, kind?: VisitKind) => {
      closeCompare();
      selectRow(row, kind);
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
  // Back/Forward through the visited commits (#618).
  const navigation = useCommitNavigation(appState, fileHistory, commitSearch, reveal, selectRowClosingCompare);
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
  // The palette's sidebar commands ("Go to Tags", "Browse branches on origin", #494).
  const sidebarReveal = useSidebarReveal();
  const [focusCommitMessage, setFocusCommitMessage] = useState(false);
  const clearFocusCommitMessage = useCallback(() => setFocusCommitMessage(false), []);
  // Discard confirmations live here (not in DiffPane) so the palette can open the same dialogs.
  const [discardRequest, setDiscardRequest] = useState<DiscardRequest | null>(null);
  // The operation strip's "Go to conflict" (#665): the working tree shows its first conflict.
  const [conflictRequest, setConflictRequest] = useState(false);
  const goToConflict = useCallback(() => {
    appState.selectRow("uncommitted");
    setConflictRequest(true);
  }, [appState]);
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
  // The tab's merge editor, for the palette's conflict moves and choices (#764).
  const mergeEditor = useMergeEditorRegistry();
  // The toolbar and the panes below it; its width, not the window's, decides the narrow layout.
  const [bodyElement, setBodyElement] = useState<HTMLDivElement | null>(null);
  const narrow = useNarrowLayout(bodyElement);
  const sidebarId = `${repoPanelId(repoPath)}-sidebar`;
  // The sidebar toggle (toolbar, shortcut, palette): docked collapse/expand, or the drawer while
  // there is no room for it (#635).
  const [sidebarState, setSidebarState] = useState<SplitViewLeftState>("docked");
  const [sidebarRequest, setSidebarRequest] = useState<SplitViewRequest | undefined>(undefined);
  const requestSidebar = useCallback(
    (kind: SplitViewRequest["kind"], returnFocusTo?: HTMLElement) =>
      setSidebarRequest((prev) => ({ kind, nonce: (prev?.nonce ?? 0) + 1, returnFocusTo })),
    [],
  );
  const toggleSidebar = useCallback((returnFocusTo?: HTMLElement) => requestSidebar("toggle", returnFocusTo), [requestSidebar]);
  // The inline New branch and Add remote forms live in the sidebar: opening one shows it.
  const sidebarDraftOpen = appState.state.createBranchDraft !== null || appState.state.addRemoteDraftOpen;
  const [seenSidebarDraftOpen, setSeenSidebarDraftOpen] = useState(sidebarDraftOpen);
  if (sidebarDraftOpen !== seenSidebarDraftOpen) {
    setSeenSidebarDraftOpen(sidebarDraftOpen);
    if (sidebarDraftOpen) requestSidebar("show");
  }
  // The recent branches the switcher lists first (#638): every change of the current branch,
  // wherever it came from, puts the old and the new one in front.
  const currentBranch = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null;
  const previousBranch = useRef(currentBranch);
  useEffect(() => {
    const previous = previousBranch.current;
    previousBranch.current = currentBranch;
    if (previous === null || currentBranch === null || previous === currentBranch) return;
    recordRecentBranch(repoPath, previous);
    recordRecentBranch(repoPath, currentBranch);
  }, [currentBranch, repoPath]);

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
  const { mounted, handleUndo, handleRedo, undoToastAction } = useUndoRedoActions(appState, active, pushToast);
  const { onCommit: commitWithToast, commitFlow } = useCommitFlow(appState, commitAndClearDraft, pushToast, undoToastAction);

  const { paletteOpen, setPaletteOpen, shortcutsOpen, setShortcutsOpen } = useWorkspaceShortcuts({
    active,
    onToggleSidebar: toggleSidebar,
    repositoryOperationDisabled,
    onCloseRepoTab,
    onReopenClosedTab: appCommands?.onReopenClosedTab,
    onNavigateBack: navigation.goBack,
    onNavigateForward: navigation.goForward,
    repoPath,
    handleUndo,
    handleRedo,
    pending,
    transfer,
    // Refresh, Fetch, Pull, Push, Stage all and the commit message run the palette's commands
    // (#646), so they follow its rules and say why they can't run.
    runCommand: (shortcut) => {
      const problem = runShortcutCommand(paletteCommands(), shortcut, pending || transfer !== null);
      if (problem !== null) pushToast(problem, { tone: "info" });
    },
    tabs: { paths: openRepos.map((repo) => repo.path), onSwitch: onSwitchRepoTab },
    workspaceElement,
  });

  // The toolbar's branch chip opens the palette straight into "Switch branch…" (#638).
  const [paletteInitialPicker, setPaletteInitialPicker] = useState<string | null>(null);
  if (!paletteOpen && paletteInitialPicker !== null) setPaletteInitialPicker(null);
  // The toolbar's Pull menu (#652).
  const pullStrategy = usePullStrategy(appState);
  // The switcher's remote-only branches (#726), read while the palette is open.
  const remoteBranches = useRemoteBranchList(client, repoPath, appState.state.remotes, paletteOpen);
  const openBranchSwitcher = () => {
    setPaletteInitialPicker(SWITCH_BRANCH_PICKER_ID);
    setPaletteOpen(true);
  };

  // A stored stash's Apply/Pop and Drop rules, shared by the stash list, the stash view and the
  // palette (#509).
  const storedStashBlockers = stashBlockers(appState.state);

  const announce = useAnnounce(appState, pushToast, undoToastAction);
  // The context menus' "Copy …" items (#614).
  const clipboard = useClipboardFeedback(pushToast);
  // The errors the banner's current one replaced, shown under it until it is dismissed (#663).
  const earlierErrors = useEarlierErrors(appState.state.error, appState.dismissError);

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
    // Only the shown tab fetches; a tab kept mounted in the background (#642) waits.
    minutes: active ? autoFetchMinutes : 0,
    prune: appState.state.fetchPrune,
    busy: pending || transfer !== null,
    refresh: appState.refresh,
    notify: pushToast,
    openCredentials,
  });
  // A tab switched back to shows its lists where they were (#642).
  useScrollMemory(workspaceElement, active);
  // Changes made outside the app show without a manual refresh (#650).
  useExternalChanges({
    client,
    repoPath,
    active,
    busy: discardDropBlockedReason !== null,
    checkForChanges: appState.checkForChanges,
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
  } = useSequenceActions(appState, pushToast, mounted, announce, undoToastAction);
  // Cherry-pick, revert and reset of one commit, shared by the commit menu and the palette
  // (#507): the toast comes from the outcome (#107).
  const cherryPickCommit = (commitId: string, mainline: number | null) =>
    void runPickAction(
      () => appState.cherryPick(commitId, mainline),
      (outcome) => pickToast("CherryPick", outcome, commitId),
      (outcome) => outcome.kind === "Committed",
    );
  const revertCommit = (commitId: string, mainline: number | null) =>
    void runPickAction(
      () => appState.revertCommit(commitId, mainline),
      (outcome) => pickToast("Revert", outcome, commitId),
      (outcome) => outcome.kind === "Committed",
    );
  const currentBranchName = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? "HEAD";
  const resetToCommit = announce(
    (...[commitId]) => `Reset ${currentBranchName} to ${commitId.slice(0, 7)}`,
    appState.resetToCommit,
    { undoable: true },
  );
  // The commits the open search results or file history list, which the loaded graph may lack
  // (#515): the palette offers their commit actions too.
  const listedCommits = (): GraphCommit[] => [
    ...(commitSearch.results?.commits ?? []),
    ...(fileHistory.history?.entries.map((entry) => entry.commit) ?? []),
  ];
  // The dialogs return focus to the list showing the selection (#146): the commit menu passes
  // its list; from the palette, which is gone by then, it is looked up.
  const openCommitDialog = (
    kind: "Cherry-pick" | "Revert" | "Reset",
    commitId: string,
    returnFocusTo?: HTMLElement | null,
  ) => {
    const isTarget = (candidate: GraphCommit) => candidate.id === commitId;
    const commit = appState.state.commits.find(isTarget) ?? listedCommits().find(isTarget);
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
  // A fetch's toast offers Pull when it left the branch behind (#661). Checked when clicked, not
  // when the toast appeared: the repository may have become busy since, and then it says why.
  const pullBlockedReason =
    operationDisabledReason ??
    (appState.state.pendingPull !== null ? "Choose how to finish the diverged pull first." : null);
  const latestPullBlock = useRef(pullBlockedReason);
  useEffect(() => {
    latestPullBlock.current = pullBlockedReason;
  });
  const { pullCurrentUpstream } = appState;
  const pullFromToast = useCallback(() => {
    const reason = latestPullBlock.current;
    if (reason !== null) pushToast(`Can't pull now. ${reason}`, { tone: "info" });
    else void pullCurrentUpstream();
  }, [pullCurrentUpstream, pushToast]);
  useOutcomeToasts(appState.state, pushToast, undoToastAction, pullFromToast);
  useStashSwitchToasts(appState.state, pushToast, undoToastAction);

  // The palette's commands, built only while it is open.
  const paletteCommands = () =>
    buildPaletteCommands({
      appState: { ...appState, abortRebase, abortSequence: paletteAbortSequence, abortMerge: paletteAbortMerge },
      onShowShortcuts: () => setShortcutsOpen(true),
      onFocusCommitMessage: () => {
        selectRowClosingCompare("uncommitted");
        setFocusCommitMessage(true);
      },
      app: appCommands,
      tabs: {
        onOpenRepoTab: (path) => void onOpenRepoTab(path),
        otherOpenRepos: openRepos.filter((repo) => repo.path !== repoPath),
        onSwitchRepoTab,
        onCloseActiveRepoTab: () => onCloseRepoTab(repoPath),
      },
      sidebar: {
        panelVisibility: panelVisibility.visibility,
        // A section or remote the palette goes to shows the sidebar (a drawer when narrow).
        revealSection: (section) => {
          requestSidebar("show");
          sidebarReveal.revealSection(section);
        },
        revealRemote: (remoteName) => {
          requestSidebar("show");
          sidebarReveal.revealRemote(remoteName);
        },
        toggle: { shown: sidebarState !== "collapsed", run: () => toggleSidebar() },
      },
      branchSwitch: {
        recentBranches: readRecentBranches(repoPath),
        onSwitchBranch: (name) => {
          recordRecentBranch(repoPath, name);
          void announce("Switched branch", appState.switchBranch)(name);
        },
        remoteBranches,
        // Like the Branches tree's remote Checkout: a local branch from the remote one, then
        // its upstream (#726).
        onCheckoutRemoteBranch: (remoteName, branchName) =>
          void (async () => {
            const failure = await announce(`Switched to ${branchName}, tracking ${remoteName}/${branchName}`, appState.createBranch, {
              undoable: true,
            })(branchName, `${remoteName}/${branchName}`, true);
            if (failure !== null) {
              pushToast(failure);
              return;
            }
            await appState.setCurrentUpstream(remoteName, branchName);
            recordRecentBranch(repoPath, branchName);
          })(),
        // One step that creates and checks out (#656), so one Undo takes it back.
        onCreateAndSwitch: (name) =>
          void (async () => {
            const failure = await announce(`Created and switched to ${name}`, appState.createBranch, {
              undoable: true,
            })(name, "HEAD", true);
            if (failure !== null) {
              pushToast(failure);
              return;
            }
            recordRecentBranch(repoPath, name);
          })(),
      },
      diff: {
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
        mergeEditor: mergeEditor.current,
        selection: fileSelection,
        requestDiscardSelection: () => {
          if (fileSelection === null || fileSelection.staged) return;
          appState.selectRow("uncommitted");
          setDiscardRequest({ kind: "files", paths: fileSelection.paths });
        },
      },
      undo: {
        undoLabel: appState.state.undoState.undo,
        redoLabel: appState.state.undoState.redo,
        onUndo: handleUndo,
        onRedo: handleRedo,
      },
      search: { onSearchCommits: requestSearchFocus },
      tools: {
        onOpenTerminal: externalTools.openTerminal,
        onRevealRepository: () => externalTools.reveal(null),
        onOpenCurrentFileInEditor: () => {
          if (currentFile !== null) externalTools.openInEditor(currentFile, null);
        },
        onOpenCurrentFileInDiffTool: () => {
          if (currentFile !== null) externalTools.openDiffTool(currentFile, { kind: "Unstaged" });
        },
        rootFiles: rootFileItems(rootFiles),
        onOpenRootFile: externalTools.openRootFile,
      },
      goTo: {
        onGoToCommit: (revision) => void goTo.goToRevision(revision),
        onSearchHistory: searchHistory,
        onGoToBranch: (kind, name) => void goTo.goToBranch(kind, name),
        onGoToTag: (name, commitId) => void goTo.goToTag(name, commitId),
        onGoToStash: (commitId) => reveal({ commitId }),
        onGoBack: navigation.canGoBack ? navigation.goBack : undefined,
        onGoForward: navigation.canGoForward ? navigation.goForward : undefined,
      },
      commit: { ...commitCommands, listedCommits: listedCommits() },
    });

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
    // One press, like every undoable action: the toast offers Undo (#662).
    onDiscardHunk: (path, oldStart, newStart, hunk) =>
      announce(`Discarded a hunk in ${path}`, appState.discardHunk, { undoable: true })(path, oldStart, newStart, hunk),
    onStageLines: (p, o, n, c, idx, shown) =>
      announce(linesToast("Staged", idx.length), appState.stageLines, { undoable: true })(p, o, n, c, idx, shown),
    onUnstageLines: (p, o, n, c, idx, shown) =>
      announce(linesToast("Unstaged", idx.length), appState.unstageLines, { undoable: true })(p, o, n, c, idx, shown),
    onDiscardLines: (p, o, n, c, idx, shown) =>
      announce(linesToast("Discarded", idx.length), appState.discardLines, { undoable: true })(p, o, n, c, idx, shown),
    // Re-checked: the repository can become busy while a confirmation is open (#521).
    onDiscardFile: (path) =>
      discardDropBlockedReason !== null
        ? undefined
        : announce(`Discarded changes to ${path}`, appState.discardFile, { undoable: true })(path),
    onDiscardAllUnstaged: (paths) => {
      if (discardDropBlockedReason === null) void announce("Discarded all unstaged changes", appState.discardAllUnstaged, { undoable: true })(paths);
    },
    onDiscardFiles: (paths) => {
      if (discardDropBlockedReason !== null) return;
      void announce(`Discarded changes to ${paths.length} ${paths.length === 1 ? "file" : "files"}`, appState.discardAllUnstaged, {
        undoable: true,
      })(paths);
    },
    // `discardAllUnstaged` deletes the untracked paths it is given (#622).
    onDeleteUntracked: (paths) => {
      if (discardDropBlockedReason !== null) return;
      void announce(`Deleted ${paths.length} untracked ${paths.length === 1 ? "file" : "files"}`, appState.discardAllUnstaged, {
        undoable: true,
      })(paths);
    },
    onIgnorePath: (pattern) => announce(`Added ${pattern} to .gitignore`, appState.ignorePath, { undoable: true })(pattern),
    onIgnorePaths: (patterns) =>
      announce(`Added ${patterns.length} ${patterns.length === 1 ? "pattern" : "patterns"} to .gitignore`, appState.ignorePaths)(patterns),
    onResolveConflict: appState.resolveConflict,
    onResolveAddDeleteConflict: appState.resolveAddDeleteConflict,
    onMarkResolved: appState.markResolved,
    onResolveWholeFile: (path, side) => void appState.resolveWholeFile(path, side),
    onCommit: commitWithToast,
    commitFlow,
    onAmendCommit: (message, options) => announce("Amended commit", amendAndRecord, { undoable: true })(message, appState.state.skipHooks, options),
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
      announce(`Restored ${path} from ${commitId.slice(0, 7)}`, appState.restoreFileVersion, { undoable: true })(commitId, path),
    patchActions: diffPanePatchActions,
    stashActions: {
      onApply: (index, restoreIndex) => void appState.applyStash(index, restoreIndex),
      onPop: (index, restoreIndex) => void appState.popStash(index, restoreIndex),
      onDrop: (index) => void dropStash(index),
      blockers: storedStashBlockers,
    },
    onCurrentFileChange: setCurrentFile,
    onSelectionChange: setFileSelection,
  };

  // Patch files dropped on the window (#649), applied like "Apply patch…": behind the repository
  // guard, whose reason a warning names instead.
  const { applyPatchFiles } = appState;
  const dropBlockedReason = repositoryOperationDisabled ? (operationDisabledReason ?? "Another action is running.") : null;
  useEffect(() => {
    if (!active) return;
    return onWorkspaceRequest((request) => {
      if (typeof request !== "object" || request.kind !== "applyPatches") return;
      if (dropBlockedReason !== null) pushToast(`Can't apply the patch now. ${dropBlockedReason}`, { tone: "warning" });
      else void applyPatchFiles(request.paths);
    });
  }, [active, applyPatchFiles, dropBlockedReason, pushToast]);

  // Closing this tab while a transfer/merge/rebase is in progress would orphan it mid-operation
  // — report busy status up so `App`'s `RepoTabs` can disable this tab's close button, the same
  // rule that already disables every other mutating action while this is true.
  // A tab in the background (#642) counts only while something of it runs: a paused merge or
  // rebase lives on disk, so closing that tab loses nothing, as when it was unmounted.
  const reportedBusy = active ? repositoryOperationDisabled : discardDropBlockedReason !== null;
  useEffect(() => {
    onBusyChange(repoPath, reportedBusy);
  }, [repoPath, reportedBusy, onBusyChange]);
  // `App` unmounts a background tab's workspace once enough other tabs were shown since (#642),
  // possibly mid-operation, before its "no longer busy" report can ever fire — which would leave
  // that tab's close button disabled for good. Clear it on the way out: the worker
  // finishes its current command and exits cleanly, and every paused operation lives on disk
  // (`MERGE_HEAD`, `sequence.json`, `rebase.json`), so `refresh()` re-reads it on remount.
  useEffect(() => () => onBusyChange(repoPath, false), [repoPath, onBusyChange]);

  return (
    // `data-active-repo` marks which workspace is the visible one. `App` keeps the workspaces of
    // the last few shown tabs mounted (#642), hidden with `display: none` while another tab is
    // shown, so their selection, scroll position and open panels survive a tab switch. `active`
    // gates everything that listens globally (shortcuts, palette and drop requests, focus
    // checks, automatic fetch), so a hidden workspace never reacts.
    <CurrentDiffContext.Provider value={currentDiff}>
    <MergeEditorContext.Provider value={mergeEditor}>
    <WorkspaceActionsContext.Provider value={workspaceActions}>
    <CopyToClipboardContext.Provider value={clipboard.copy}>
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
        mergeMessage={appState.state.mergeMessage}
        currentBranch={appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null}
        onGoToConflict={goToConflict}
        onContinueRebase={() => void appState.rebaseContinue()}
      />
      <HookRunStrip running={appState.state.hookRun} onCancel={() => void appState.cancelHookRun()} />
      {externalTools.error !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={externalTools.error} copyDetails onDismiss={externalTools.dismissError} />
        </div>
      )}
      {clipboard.error !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={clipboard.error} copyDetails onDismiss={clipboard.dismissError} />
        </div>
      )}
      {appState.state.error !== null && (
        <div className={styles.errorLayer}>
          <InlineError
            message={appState.state.error}
            hint={appState.state.errorHint ?? undefined}
            copyDetails
            earlier={earlierErrors.earlier}
            onDismiss={earlierErrors.dismiss}
          />
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
        paletteInitialPicker={paletteInitialPicker}
        shortcutsOpen={shortcutsOpen}
        onShortcutsOpenChange={setShortcutsOpen}
      />
      {/* Busy while the first reads are out, or while an action other than a transfer runs: a
          transfer leaves browsing to the user and says so in the toolbar instead (#659, #660). */}
      <div
        ref={setBodyElement}
        className={styles.workspaceBody}
        aria-busy={!appState.state.loaded || (pending && appState.state.transferSubject === null)}
      >
      <RepoToolbar
        sidebar={{ state: sidebarState, controls: sidebarId, onToggle: toggleSidebar }}
        branch={{
          current: currentBranch,
          detachedAt: appState.state.headInfo.detachedAt,
          upstream: appState.state.upstream,
          onOpenSwitcher: openBranchSwitcher,
        }}
        navigation={{
          canGoBack: navigation.canGoBack,
          canGoForward: navigation.canGoForward,
          onGoBack: navigation.goBack,
          onGoForward: navigation.goForward,
        }}
        sync={{
          remotes: appState.state.remotes,
          upstream: appState.state.upstream,
          detachedAt: appState.state.headInfo.detachedAt,
          operationDisabled: repositoryOperationDisabled,
          operationDisabledReason,
          onFetch: appState.fetchRemote,
          onPull: appState.pullCurrentUpstream,
          pullPending: appState.state.pendingPull !== null,
          pullStrategy,
          onPush: appState.pushCurrentBranch,
          prune: effectiveFetchPrune(appState.state.fetchPrune, appState.state.remotes, appState.state.upstream),
          onFetchAll: appState.fetchAllRemotes,
          onTogglePrune: appState.setFetchPrune,
          lastFetchedAt,
          autoFetchMinutes,
          activity: (
            <RepoActivity
              subject={appState.state.transferSubject}
              progress={transfer}
              pending={pending}
              // Fire-and-forget: the backend records the request, and the transfer's own terminal
              // event (errorKind "Cancelled") ends it.
              onCancel={(operationId) => void client.cancelTransfer(repoPath, operationId)}
            />
          ),
          onPublish: () => {
            if (currentBranch !== null) openDialog({ kind: "push", localBranch: currentBranch });
          },
          stash: {
            reason: stashBlockReason(appState.state),
            onStash: announce("Changes stashed", () => appState.saveStash()),
            onStashWithOptions: () => openDialog({ kind: "stash", paths: [] }),
          },
        }}
        undo={{
          undoLabel: appState.state.undoState.undo,
          redoLabel: appState.state.undoState.redo,
          disabled: pending || transfer !== null,
          disabledReason: operationDisabledReason,
          onUndo: handleUndo,
          onRedo: handleRedo,
        }}
        applyPatch={{
          // Like the palette's "Apply patch…": behind the repository guard.
          disabledReason: repositoryOperationDisabled ? (operationDisabledReason ?? "Another action is running.") : null,
          onApplyPatch: () => void appState.applyPatchFile(),
        }}
        onRefresh={() => void appState.refresh({ full: true })}
      />
      <SplitView
        storageKey="sidebar-width"
        defaultWidth={260}
        minWidth={200}
        maxWidth={420}
        collapsible
        forceCollapsed={narrow}
        label="Sidebar width"
        leftRegion="Sidebar"
        leftId={sidebarId}
        request={sidebarRequest}
        onLeftStateChange={setSidebarState}
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
            revealRequest={sidebarReveal.request}
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
            conflictRequest={conflictRequest}
            onConflictRequestHandled={() => setConflictRequest(false)}
            initialCommitDraft={getCommitDraft(repoPath)}
            discardBlockedReason={discardDropBlockedReason}
            focusCommitMessage={focusCommitMessage}
          />
        }
      />
      </div>
    </div>
    </CopyToClipboardContext.Provider>
    </WorkspaceActionsContext.Provider>
    </MergeEditorContext.Provider>
    </CurrentDiffContext.Provider>
  );
}
