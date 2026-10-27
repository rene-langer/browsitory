import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Moon, Sparkles, Sun } from "lucide-react";
import { BranchTree } from "./components/BranchTree";
import { CommandPalette } from "./components/CommandPalette";
import type { CommitDraft } from "./components/CommitBox";
import { ShortcutHint } from "./components/ShortcutHint";
import { ShortcutSheet } from "./components/ShortcutSheet";

const NARROW_BREAKPOINT = 900;
import { CommitGraph } from "./components/CommitGraph";
import { FileHistoryList } from "./components/FileHistoryList";
import { CommitSearchBar } from "./components/CommitSearchBar";
import { CommitSearchResults } from "./components/CommitSearchResults";
import { SyncBar } from "./components/SyncBar";
import { UndoRedoButtons } from "./components/UndoRedoButtons";
import { DiffPane } from "./components/DiffPane";
import { RebasePlanner } from "./components/RebasePlanner";
import { ReflogPanel } from "./components/ReflogPanel";
import { RepoPicker } from "./components/RepoPicker";
import { RepoTabs } from "./components/RepoTabs";
import { repoPanelId, repoTabId } from "./components/repoTabIds";
import { ReleaseNotesModal, type ReleaseNotesEntry } from "./components/ReleaseNotesModal";
import { OperationStatusStrip } from "./components/OperationStatusStrip";
import { ToastRegion } from "./components/primitives/ToastRegion";
import { useToasts, type ToastAction } from "./state/useToasts";
import { InlineError } from "./components/primitives/InlineError";
import { ConfirmDialog } from "./components/primitives/ConfirmDialog";
import { CloneDialog } from "./components/CloneDialog";
import { Overlay } from "./components/primitives/Overlay";
import { Sidebar } from "./components/primitives/Sidebar";
import { SplitView } from "./components/primitives/SplitView";
import { PullRequestPanel } from "./components/PullRequestPanel";
import { StashPanel } from "./components/StashPanel";
import { TagPanel } from "./components/TagPanel";
import { SubmodulePanel } from "./components/SubmodulePanel";
import { TransferPanel } from "./components/TransferPanel";
import { WorktreePanel } from "./components/WorktreePanel";
import { PushBranchDialog } from "./components/PushBranchDialog";
import { DeleteRemoteBranchDialog } from "./components/DeleteRemoteBranchDialog";
import type { RepoClient } from "./ipc/RepoClient";
import { subscribeTransportStatus } from "./ipc/transportStatus";
import { buildCommands, type AppCommands } from "./lib/commands";
import { effectiveFetchPrune } from "./lib/fetchPrune";
import { upstreamPushBranch } from "./lib/pushTarget";
import { cloneToastMessage, createNewRepository } from "./lib/newRepository";
import { repoNameOf } from "./lib/repoName";
import { applyTheme, loadStoredTheme, persistTheme, resolveTheme, type Theme } from "./lib/theme";
import { useAppState } from "./state/useAppState";
import { useFileHistory } from "./state/useFileHistory";
import { useCommitSearch } from "./state/useCommitSearch";
import { DiffSettingsProvider } from "./state/DiffSettingsProvider";
import { useDiffSettingsContext } from "./state/useDiffSettings";
import type { DiscardRequest } from "./components/DiffPane";
import { useOpenRepos, type OpenRepo } from "./state/useOpenRepos";
import { useSidebarPanelVisibility } from "./state/useSidebarPanelVisibility";
import { useWorkspaces } from "./state/useWorkspaces";
import styles from "./App.module.css";
import releaseNotesData from "./generated/releaseNotes.json";

const allReleaseNotes = releaseNotesData as ReleaseNotesEntry[];

// Toast copy for line-level stage/unstage/discard ("Staged 1 line", "Discarded 3 lines").
const linesToast = (verb: string, n: number) => `${verb} ${n} ${n === 1 ? "line" : "lines"}`;

function RepoWorkspace({
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
  // `App`'s per-repo commit-message drafts (see `commitDrafts` there): this workspace unmounts
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
    appState.state.selectedRow,
    appState.selectRow,
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
  const handleCommitDraftChange = useCallback(
    (draft: CommitDraft) => onCommitDraftChange(repoPath, draft),
    [onCommitDraftChange, repoPath],
  );
  const panelVisibility = useSidebarPanelVisibility();
  const [focusCommitMessage, setFocusCommitMessage] = useState(false);
  const clearFocusCommitMessage = useCallback(() => setFocusCommitMessage(false), []);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  // Discard confirmations live here (not in DiffPane) so the palette can open the same dialogs.
  const [discardRequest, setDiscardRequest] = useState<DiscardRequest | null>(null);
  const [currentFile, setCurrentFile] = useState<string | null>(null);
  const diffSettings = useDiffSettingsContext();
  // Below this width the sidebar auto-collapses so the graph and diff panes both stay usable.
  const [narrow, setNarrow] = useState(() => window.innerWidth < NARROW_BREAKPOINT);
  useEffect(() => {
    const onResize = () => setNarrow(window.innerWidth < NARROW_BREAKPOINT);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // Populate this tab's state as soon as it mounts. `useAppState` no longer fetches anything on
  // its own — its old `openRepo` method (removed when repo-opening moved out to `useOpenRepos`)
  // used to be what triggered the first `refresh()` right after a repo opened, via
  // `runMutation`'s trailing `await refresh()`. `RepoWorkspace` only ever mounts once a repo is
  // already open (`App`'s `openRepos.openRepos.map` below, keyed by `repo.path` so each tab gets
  // exactly one fresh `RepoWorkspace`/`useAppState` instance for its lifetime), so firing this
  // once on mount reproduces that old behavior without needing `useAppState` itself to auto-fetch
  // — which would also fire on every render in hook-level tests that manage refresh manually.
  useEffect(() => {
    void appState.refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const repositoryOperationDisabled =
    appState.state.pending ||
    appState.state.transfer !== null ||
    appState.state.mergeMessage !== null ||
    appState.state.rebaseProgress !== null;

  const { undo: undoStep, redo: redoStep, refreshUndoState } = appState;
  const { pending, transfer } = appState.state;
  // The toast stack is app-level and outlives this workspace (tab switch unmounts it), so a
  // toast action checks it's still mounted and idle, read through refs at click time.
  const mounted = useRef(true);
  const busy = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  useEffect(() => {
    busy.current = pending || transfer !== null;
  }, [pending, transfer]);
  // Each success toast offers the opposite step.
  const runUndoStep = useCallback(
    function run(kind: "undo" | "redo") {
      const step = kind === "undo" ? undoStep : redoStep;
      void step().then((outcome) => {
        if (outcome?.kind !== "Done") return;
        const other = kind === "undo" ? "redo" : "undo";
        pushToast(`${kind === "undo" ? "Undid" : "Redid"} ${outcome.label}`, {
          label: kind === "undo" ? "Redo" : "Undo",
          run: () => {
            if (mounted.current && !busy.current) run(other);
          },
        });
      });
    },
    [undoStep, redoStep, pushToast],
  );
  const handleUndo = useCallback(() => runUndoStep("undo"), [runUndoStep]);
  const handleRedo = useCallback(() => runUndoStep("redo"), [runUndoStep]);

  useEffect(() => {
    if (!active) return;
    const onFocus = () => void refreshUndoState();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [active, refreshUndoState]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (!active) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((prev) => !prev);
        return;
      }
      if (event.key === "?" && !event.metaKey && !event.ctrlKey && !event.altKey) {
        const target = event.target as HTMLElement | null;
        const tag = target?.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) return;
        event.preventDefault();
        setShortcutsOpen(true);
        return;
      }
      const mod = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      const isUndo = mod && !event.altKey && key === "z" && !event.shiftKey;
      const isRedo = mod && !event.altKey && ((key === "z" && event.shiftKey) || (event.ctrlKey && !event.metaKey && key === "y"));
      if (isUndo || isRedo) {
        const target = event.target as HTMLElement | null;
        const tag = target?.tagName;
        // Text fields keep their native undo (commit message, branch-name inputs, …).
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) return;
        if (pending || transfer !== null) return;
        event.preventDefault();
        if (isUndo) handleUndo();
        else handleRedo();
        return;
      }
      // A11Y-003: keyboard equivalent for the per-tab close button, which is mouse-only
      // (`aria-hidden`/`tabIndex={-1}` in `RepoTabs.tsx`) so it doesn't sit in the tablist's
      // accessible children. This `RepoWorkspace` only ever handles its own tab's `repoPath`, and
      // only while `active` (checked above), so "the active repo" is just this one.
      // Best-effort only: Tauri's default macOS app menu binds Cmd+W to "Close Window" (the whole
      // app) before the webview sees the key, and a VSCode webview may forward Ctrl+W to the
      // workbench. The host-independent paths are Delete on a focused tab (`RepoTabs.tsx`) and
      // the palette's "Close tab" command.
      if (event.key.toLowerCase() === "w" && (event.metaKey || event.ctrlKey) && !event.altKey) {
        if (repositoryOperationDisabled) return;
        event.preventDefault();
        onCloseRepoTab(repoPath);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [active, repositoryOperationDisabled, onCloseRepoTab, repoPath, handleUndo, handleRedo, pending, transfer]);

  // A short, human-readable explanation for why `repositoryOperationDisabled` is currently true
  // — threaded into the sidebar mutation panels (`BranchTree`, `WorktreePanel`, `TagPanel`,
  // `PullRequestPanel`) so their disabled buttons carry a `title` explaining the
  // block instead of just going inert with no explanation (issue #31/UX-003). First-match-wins,
  // in the same order `repositoryOperationDisabled` checks them; `null` when nothing is blocking.
  // Deliberately not a fine-grained per-panel lock (e.g. "only disable panels that actually
  // conflict with a rebase") — that's the "Large" effort option the issue explicitly calls out as
  // out of scope for this pass.
  const operationDisabledReason: string | null = appState.state.pending
    ? "Another action is in progress."
    : appState.state.transfer !== null
      ? "A transfer is in progress."
      : appState.state.mergeMessage !== null
        ? appState.state.headInfo.operation === "CherryPick"
          ? "A cherry-pick is in progress."
          : appState.state.headInfo.operation === "Revert"
            ? "A revert is in progress."
            : "A merge is in progress."
        : appState.state.rebaseProgress !== null
          ? "A rebase is in progress."
          : null;

  // Positive feedback (AUD-2026-09-20-FB-003). The mutation runner swallows failures into
  // `state.error`, so success is detected as "pending fell back to false with no error" after a
  // labelled action; `announce` clears any stale error first so that check is unambiguous.
  const pendingLabel = useRef<string | null>(null);
  const { pending: statePending, error: stateError } = appState.state;
  useEffect(() => {
    if (statePending || pendingLabel.current === null) return;
    const label = pendingLabel.current;
    pendingLabel.current = null;
    if (stateError === null) pushToast(label);
  }, [statePending, stateError, pushToast]);
  const { dismissError } = appState;
  const announce = useCallback(
    <A extends unknown[], R,>(label: string, action: (...args: A) => R) =>
      (...args: A): R => {
        dismissError();
        pendingLabel.current = label;
        return action(...args);
      },
    [dismissError],
  );
  const currentBranchName = appState.state.branches.find((branch) => branch.isCurrent)?.name ?? appState.state.upstream?.remoteBranch ?? "";
  // The branch `force_push_current_branch` overwrites on `remoteName`: what push.default resolves
  // for the upstream's remote, else the current branch's own name.
  const forcePushTargetBranch = (remoteName: string) =>
    (appState.state.upstream?.remoteName === remoteName ? upstreamPushBranch(appState.state.upstream) : null) ??
    currentBranchName;
  const pushDialogBranch = appState.state.pushDialog?.localBranch ?? null;
  const pullOutcome = appState.state.pullOutcome;
  useEffect(() => {
    if (pullOutcome === null) return;
    if (pullOutcome.kind === "UpToDate") pushToast("Already up to date");
    else if (pullOutcome.kind === "FastForwarded") pushToast(`Pulled ${pullOutcome.upstreamRef}`);
  }, [pullOutcome, pushToast]);

  // Closing this tab while a transfer/merge/rebase is in progress would orphan it mid-operation
  // — report busy status up so `App`'s `RepoTabs` can disable this tab's close button, the same
  // rule that already disables every other mutating action while this is true.
  useEffect(() => {
    onBusyChange(repoPath, repositoryOperationDisabled);
  }, [repoPath, repositoryOperationDisabled, onBusyChange]);
  // Mirrors `appState.state.rebaseProgress` into a ref so the unmount cleanup below can read its
  // current value instead of a stale one closed over when that effect was first set up.
  const rebaseProgressRef = useRef(appState.state.rebaseProgress);
  useEffect(() => {
    rebaseProgressRef.current = appState.state.rebaseProgress;
  }, [appState.state.rebaseProgress]);
  // Only the active tab's workspace is mounted (PERF-001), so switching away mid-operation
  // unmounts this component before its "no longer busy" report can ever fire — which would leave
  // the backgrounded tab's close button disabled for good. Clear it on the way out for
  // `pending`/`transfer`/merge, which are all safe to lose track of: the worker finishes its
  // current command and exits cleanly, and merge state (`MERGE_HEAD`) lives on disk so
  // `refresh()` re-reads it correctly on remount. Rebase state is the exception — it lives only
  // in the worker's in-memory `rebase_state`, nothing beyond `set_head_detached` is persisted —
  // so unmounting mid-rebase must NOT clear busy, or closing this tab drops the worker (and its
  // rebase state) while HEAD is still detached with a conflicted index and no way to Continue or
  // Abort from the app.
  useEffect(
    () => () => onBusyChange(repoPath, rebaseProgressRef.current !== null),
    [repoPath, onBusyChange],
  );

  return (
    // `data-active-repo` marks which workspace is the visible one. `App` only ever mounts the
    // active tab's `RepoWorkspace` now (PERF-001), so in practice exactly one of these exists at
    // a time — but the attribute (and the `active` prop it mirrors) stays: `commands.ts`'s
    // `goToSidebarSection` still scopes its `document.querySelector` lookup to it, and this
    // component's own internal gates (`active &&` below) still need a real boolean rather than an
    // assumption that they're always mounted-implies-active.
    <div
      style={{ display: active ? "contents" : "none" }}
      data-active-repo={active ? "true" : "false"}
      role="tabpanel"
      id={repoPanelId(repoPath)}
      aria-labelledby={repoTabId(repoPath)}
    >
      <OperationStatusStrip
        merging={appState.state.mergeMessage !== null}
        operation={appState.state.headInfo.operation}
        rebaseProgress={appState.state.rebaseProgress}
        conflictCount={appState.state.status.filter((entry) => entry.kind === "Conflicted").length}
        onAbortMerge={appState.abortMerge}
        onAbortRebase={appState.abortRebase}
      />
      {appState.state.error !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={appState.state.error} onDismiss={appState.dismissError} />
          {appState.state.forcePushOffer !== null && appState.state.forcePushConfirm === null && (
            <button type="button" onClick={() => appState.requestForcePush(appState.state.forcePushOffer!.remoteName)}>
              Force push (with lease)…
            </button>
          )}
        </div>
      )}
      {active && appState.state.forcePushConfirm !== null && (
        <ConfirmDialog
          ariaLabel="Confirm force push"
          message={
            <p>
              Overwrites {appState.state.forcePushConfirm.remoteName}/
              {forcePushTargetBranch(appState.state.forcePushConfirm.remoteName)} with your local branch.
            </p>
          }
          confirmLabel="Force push"
          onConfirm={() =>
            void announce("Force pushed", appState.forcePushCurrentBranch)(appState.state.forcePushConfirm!.remoteName)
          }
          onCancel={appState.cancelForcePush}
        />
      )}
      {active && pushDialogBranch !== null && (
        <PushBranchDialog
          localBranch={pushDialogBranch}
          remotes={appState.state.remotes}
          branchUpstream={appState.state.branches.find((branch) => branch.name === pushDialogBranch)?.upstream}
          onPush={(remoteName, remoteBranch, setUpstream) =>
            void announce("Push complete", appState.pushBranch)(remoteName, pushDialogBranch, remoteBranch, setUpstream)
          }
          onCancel={appState.closePushDialog}
        />
      )}
      {active && appState.state.deleteRemoteBranchDialog !== null && (
        <DeleteRemoteBranchDialog
          remotes={appState.state.remotes}
          initialRemote={appState.state.deleteRemoteBranchDialog.remoteName}
          initialBranch={appState.state.deleteRemoteBranchDialog.branchName}
          onListRemoteBranches={appState.listRemoteBranches}
          onDelete={(remoteName, branchName) =>
            void announce("Remote branch deleted", appState.deleteRemoteBranch)(remoteName, branchName)
          }
          onCancel={appState.closeDeleteRemoteBranchDialog}
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
              appState,
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
              },
              appCommands,
              {
                undoLabel: appState.state.undoState.undo,
                redoLabel: appState.state.undoState.redo,
                onUndo: handleUndo,
                onRedo: handleRedo,
              },
              { onSearchCommits: requestSearchFocus },
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
              { id: "pullRequests", label: "Pull Requests", visible: panelVisibility.visibility.pullRequests, onToggle: (v) => panelVisibility.setPanelVisible("pullRequests", v) },
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
              onMergeBranch={(name) =>
                void appState.getMergeOptions().then((options) => appState.mergeBranch(name, options))
              }
              isMerging={appState.state.mergeMessage !== null}
              isRebasing={appState.state.rebaseProgress !== null}
              operationDisabled={repositoryOperationDisabled}
              operationDisabledReason={operationDisabledReason}
              graphBranchSelection={appState.state.graphBranchSelection}
              onSetGraphBranchSelection={appState.setGraphBranchSelection}
              graphRemoteBranchSelection={appState.state.graphRemoteBranchSelection}
              onSetGraphRemoteBranchSelection={appState.setGraphRemoteBranchSelection}
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
              refreshGeneration={appState.state.refreshGeneration}
            />
            {panelVisibility.visibility.stash && (
              <StashPanel
                stashes={appState.state.stashes}
                onSelectRow={appState.selectRow}
                onApplyStash={appState.applyStash}
                onDropStash={appState.dropStash}
                operationDisabled={repositoryOperationDisabled}
                operationDisabledReason={operationDisabledReason}
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
                operationDisabled={repositoryOperationDisabled}
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
                onCreate={appState.createTag}
                onDelete={appState.deleteTag}
                onPush={appState.pushTags}
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
              {fileHistory.history !== null ? (
                <FileHistoryList
                  history={fileHistory.history}
                  selectedRow={appState.state.selectedRow}
                  onSelectRow={appState.selectRow}
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
                  />
                  {commitSearch.results !== null ? (
                    <CommitSearchResults
                      results={commitSearch.results}
                      selectedRow={appState.state.selectedRow}
                      onSelectRow={appState.selectRow}
                      onClose={commitSearch.close}
                      onLoadMore={commitSearch.loadMore}
                      onRetry={commitSearch.retry}
                      onRequestSearch={requestSearchFocus}
                      listRef={searchResultsRef}
                    />
                  ) : (
                    <CommitGraph
                      onRequestSearch={requestSearchFocus}
                      hasMore={appState.state.hasMoreHistory}
                      onLoadMore={() => void appState.loadMoreHistory()}
                      status={appState.state.status}
                      commits={appState.state.commits}
                      selectedRow={appState.state.selectedRow}
                      pending={repositoryOperationDisabled}
                      onSelectRow={appState.selectRow}
                      onBranchFromCommit={appState.openCreateBranchDraft}
                      onRebaseFromCommit={appState.openRebasePlanner}
                      onRebaseSelection={appState.openRebaseSelection}
                      headInfo={appState.state.headInfo}
                      currentBranch={appState.state.branches.find((branch) => branch.isCurrent)?.name ?? null}
                      onCherryPick={announce("Cherry-picked", appState.cherryPick)}
                      onRevert={announce("Reverted", appState.revertCommit)}
                      onCheckoutCommit={announce("Checked out commit", appState.checkoutCommit)}
                      onResetToCommit={announce("Reset branch", appState.resetToCommit)}
                      onEditHeadMessage={() => {
                        appState.selectRow("uncommitted");
                        appState.setAmendMode(true);
                        setFocusCommitMessage(true);
                      }}
                      graphRemoteBranchSelection={appState.state.graphRemoteBranchSelection}
                      remotes={appState.state.remotes}
                      onPushTag={(remoteName, tag) => void announce("Push complete", appState.pushTags)(remoteName, [tag])}
                      onDeleteTag={appState.deleteTag}
                    />
                  )}
                </>
              )}
              </>
            }
            right={
              <DiffPane
                repoPath={repoPath}
                client={client}
                selectedRow={appState.state.selectedRow}
                commits={diffPaneCommits}
                fileHistoryFocus={historyEntry === null ? null : { path: historyEntry.path, oldPath: historyEntry.oldPath }}
                onOpenFileHistory={fileHistory.open}
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
                onDiscardFile={(path) => announce(`Discarded changes to ${path}`, appState.discardFile)(path)}
                onDiscardAllUnstaged={announce("Discarded all unstaged changes", appState.discardAllUnstaged)}
                onIgnorePath={(pattern) => announce(`Added ${pattern} to .gitignore`, appState.ignorePath)(pattern)}
                onCommit={announce("Committed", appState.commit)}
                onSaveStash={announce("Changes stashed", appState.saveStash)}
                onSelectRow={appState.selectRow}
                onResolveConflict={appState.resolveConflict}
                onResolveAddDeleteConflict={appState.resolveAddDeleteConflict}
                mergeMessage={appState.state.mergeMessage}
                onAbortMerge={appState.abortMerge}
                amendMode={appState.state.amendMode}
                headInfo={appState.state.headInfo}
                onSetAmendMode={appState.setAmendMode}
                onAmendCommit={announce("Amended commit", appState.amendCommit)}
                focusCommitMessage={focusCommitMessage}
                onCommitMessageFocused={clearFocusCommitMessage}
                rebaseProgress={appState.state.rebaseProgress}
                onRebaseContinue={appState.rebaseContinue}
                onRebaseAbort={appState.abortRebase}
              />
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
            onStartRebase={appState.startRebase}
            onCancel={appState.closeRebasePlanner}
            operationDisabled={repositoryOperationDisabled}
            presetActions={appState.state.rebasePreset ?? undefined}
          />
        </Overlay>
      )}
    </div>
  );
}

export default function App(props: { client: RepoClient; updateBanner?: ReactNode }) {
  return (
    <DiffSettingsProvider client={props.client}>
      <AppContent {...props} />
    </DiffSettingsProvider>
  );
}

function AppContent({
  client,
  updateBanner = null,
}: {
  client: RepoClient;
  updateBanner?: ReactNode;
}) {
  const openRepos = useOpenRepos(client);
  const workspaces = useWorkspaces(client);
  const workspaceNames = useMemo(
    () => Object.fromEntries(workspaces.workspaces.map((workspace) => [workspace.id, workspace.name])),
    [workspaces.workspaces],
  );
  const [theme, setTheme] = useState<Theme>(() =>
    resolveTheme(
      loadStoredTheme(),
      window.matchMedia("(prefers-color-scheme: dark)").matches,
    ),
  );
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const [busyByPath, setBusyByPath] = useState<Record<string, boolean>>({});
  const onBusyChange = useCallback((repoPath: string, busy: boolean) => {
    setBusyByPath((prev) => (prev[repoPath] === busy ? prev : { ...prev, [repoPath]: busy }));
  }, []);
  const busyPaths = useMemo(
    () => new Set(Object.entries(busyByPath).filter(([, busy]) => busy).map(([path]) => path)),
    [busyByPath],
  );

  // Per-repo commit-message drafts, keyed by repo path. A ref rather than state: nothing renders
  // from it — `CommitBox` reads its entry once on mount and writes through on every change — so
  // keeping it out of state avoids re-rendering the whole app on each keystroke. Entries outlive
  // their tab on purpose (reopening a repo in the same session restores its draft), and cost one
  // short string per repo ever opened.
  const commitDrafts = useRef<Record<string, CommitDraft>>({});
  const getCommitDraft = useCallback((repoPath: string) => commitDrafts.current[repoPath], []);
  const onCommitDraftChange = useCallback((repoPath: string, draft: CommitDraft) => {
    commitDrafts.current[repoPath] = draft;
  }, []);

  const [pickingRepo, setPickingRepo] = useState(false);

  // `RepoPicker` only surfaces errors from its own `pickRepoFolder`/`listRecentRepos` calls, and
  // `useOpenRepos.openRepo` deliberately rejects rather than opening a tab for a repo that failed
  // to open (bad path, a stale recent-repo entry, permissions). Nothing else catches that now
  // that `App` has no `useAppState` of its own, so a failed open would look like nothing
  // happened — the same trap the pre-tabs `App` carried a comment about.
  const [openError, setOpenError] = useState<{ message: string; path: string } | null>(null);
  const [transportError, setTransportError] = useState<string | null>(null);
  useEffect(
    () => subscribeTransportStatus((status) => setTransportError(status.message)),
    [],
  );
  const { openRepo } = openRepos;
  const openRepoTab = useCallback(
    // Both branches settle asynchronously on purpose: the E2E auto-open effect below calls this
    // directly, and a synchronous `setOpenError` in the body would be a setState-in-effect.
    (path: string) => {
      return openRepo(path).then(
        () => setOpenError(null),
        (error: unknown) => setOpenError({ message: String(error), path }),
      );
    },
    [openRepo],
  );

  const appToasts = useToasts();
  const [cloning, setCloning] = useState(false);
  const cloneRunning = useRef(false);
  const [existingRepoPrompt, setExistingRepoPrompt] = useState<string | null>(null);
  const { push: pushAppToast } = appToasts;
  const startClone = useCallback(() => {
    setPickingRepo(false);
    setCloning(true);
  }, []);
  const startNewRepository = useCallback(() => {
    setPickingRepo(false);
    createNewRepository(client).then(
      (result) => {
        if (result.kind === "created") {
          void openRepoTab(result.path);
          pushAppToast(`Created repository ${repoNameOf(result.path)}`);
        } else if (result.kind === "exists") {
          setExistingRepoPrompt(result.path);
        }
      },
      (error: unknown) => setOpenError({ message: String(error instanceof Error ? error.message : error), path: "" }),
    );
  }, [client, openRepoTab, pushAppToast]);
  const appCommands = useMemo(
    () => (client.supportsCloneAndInit ? { onCloneRepository: startClone, onNewRepository: startNewRepository } : undefined),
    [client.supportsCloneAndInit, startClone, startNewRepository],
  );

  const [releaseNotesView, setReleaseNotesView] = useState<
    { mode: "auto" | "all"; entries: ReleaseNotesEntry[] } | null
  >(null);

  useEffect(() => {
    // Skip in E2E: a fresh run always has no last-seen version, so this would auto-open the
    // modal (a native <dialog>) on every launch and block interaction with the rest of the UI
    // that the E2E specs are actually testing.
    if (typeof import.meta.env.VITE_E2E_REPO_PATH === "string") return;
    let cancelled = false;
    async function checkVersion() {
      try {
        const [appVersion, lastSeenVersion] = await Promise.all([
          client.getAppVersion(),
          client.getLastSeenVersion(),
        ]);
        if (cancelled || appVersion === lastSeenVersion) return;
        const seenIndex =
          lastSeenVersion === null
            ? -1
            : allReleaseNotes.findIndex((entry) => entry.version === lastSeenVersion);
        const entries = seenIndex === -1 ? allReleaseNotes.slice(0, 1) : allReleaseNotes.slice(0, seenIndex);
        setReleaseNotesView({ mode: "auto", entries });
      } catch (error) {
        console.error("Release notes version check failed", error);
      }
    }
    void checkVersion();
    return () => {
      cancelled = true;
    };
  }, [client]);

  function closeReleaseNotes() {
    const wasAuto = releaseNotesView?.mode === "auto";
    setReleaseNotesView(null);
    if (!wasAuto) return;
    void client
      .getAppVersion()
      .then((version) => client.setLastSeenVersion(version))
      .catch((error) => console.error("Failed to record last-seen version", error));
  }

  const themeToggle = (
    <button
      type="button"
      className={styles.iconButton}
      aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
      onClick={() => {
        const next = theme === "dark" ? "light" : "dark";
        setTheme(next);
        persistTheme(next);
      }}
    >
      {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
    </button>
  );

  // E2E-only auto-open: `RepoPicker`'s native folder dialog can't be driven through WebDriver,
  // so the E2E build points at a fixture repo via this Vite env var instead — opened as this
  // session's first tab. Statically absent from a normal production build unless
  // VITE_E2E_REPO_PATH is set at build time.
  // Guards the E2E reconciliation effect below so it only forces a deterministic starting repo
  // once, at launch — not on every later `openRepos.openRepos` change, which would otherwise
  // also fire for (and undo) a tab the test itself opens afterward.
  // The workspace E2E uses a one-shot local-storage marker before restarting the real app so
  // that one launch can prove persisted workspace grouping is restored. Normal E2E launches
  // still reconcile to the single fixture repo, preserving cross-test isolation.
  const e2eReconciledRef = useRef(false);
  useEffect(() => {
    if (
      typeof import.meta.env.VITE_E2E_REPO_PATH === "string" &&
      window.localStorage.getItem("browsitory-e2e-restore-open-repos-once") === "true"
    ) {
      e2eReconciledRef.current = true;
      window.localStorage.removeItem("browsitory-e2e-restore-open-repos-once");
    }
  }, []);
  useEffect(() => {
    // Wait for the persisted-tab restore to settle first: on the mount pass `openRepos` is still
    // the empty initial value, so without this the fixture would be opened concurrently with
    // `listOpenRepos`, and whichever promise resolved last would clobber the other's tab list.
    if (openRepos.loading || e2eReconciledRef.current) return;
    const autoOpenPath = import.meta.env.VITE_E2E_REPO_PATH;
    if (typeof autoOpenPath !== "string" || autoOpenPath.length === 0) {
      e2eReconciledRef.current = true;
      return;
    }
    // E2E fixtures need a deterministic single starting repo, not whatever tabs happen to be
    // persisted from a prior run — e.g. a previous spec that crashed after opening a second tab
    // but before closing it would otherwise leak a two-tab restore into the next spec's launch.
    // So this bypasses the persisted-tab restore entirely rather than only firing when it
    // happened to already be empty: close anything that isn't the fixture first (the effect
    // re-runs once that settles, since `openRepos.openRepos` is a dependency), then open the
    // fixture if it isn't already the sole tab. Once exactly the fixture is open, mark
    // reconciliation done so this stops touching tabs — the spec itself opens more afterward.
    const stale = openRepos.openRepos.filter((repo) => repo.path !== autoOpenPath);
    if (stale.length > 0) {
      stale.forEach((repo) => openRepos.closeRepo(repo.path));
      return;
    }
    if (openRepos.openRepos.length === 0) {
      void openRepoTab(autoOpenPath);
      return;
    }
    e2eReconciledRef.current = true;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openRepos.loading, openRepos.openRepos]);

  if (openRepos.loading) {
    return (
      <main>
        <header className={styles.headerRow}>
          <h1>Browsitory</h1>
          <div className={styles.headerSpacer} />
          {themeToggle}
        </header>
        <p role="status" className={styles.loading}>
          Loading…
        </p>
      </main>
    );
  }

  return (
    <main>
      {updateBanner}
      <header className={styles.headerRow}>
        <h1>Browsitory</h1>
        <RepoTabs
          openRepos={openRepos.openRepos}
          activePath={openRepos.activePath}
          busyPaths={busyPaths}
          workspaceNames={workspaceNames}
          onSwitchTo={openRepos.switchTo}
          onClose={openRepos.closeRepo}
          onCloseGroup={(paths) => paths.forEach((path) => openRepos.closeRepo(path))}
          onAddTab={() => setPickingRepo(true)}
        />
        {openRepos.openRepos.length === 0 && <div className={styles.headerSpacer} />}
        {openRepos.openRepos.length > 0 && <ShortcutHint />}
        {themeToggle}
        <button
          type="button"
          className={styles.iconButton}
          aria-label="Release notes"
          title="What’s new"
          onClick={() => setReleaseNotesView({ mode: "all", entries: allReleaseNotes })}
        >
          <Sparkles size={16} aria-hidden="true" />
        </button>
      </header>
      {openRepos.restoreError !== null && (
        <div className={styles.errorLayer}>
          <InlineError message={openRepos.restoreError} onDismiss={openRepos.dismissRestoreError} />
        </div>
      )}
      {(transportError !== null || openError !== null) && (
        <div className={styles.errorLayer}>
          {transportError !== null && (
            <InlineError
              message={transportError}
              hint="Browsitory lost its connection to the backend. Restart the app if this keeps happening."
              onDismiss={() => setTransportError(null)}
            />
          )}
          {openError !== null && (
            <InlineError
              message={openError.message}
              hint={openError.path !== "" ? "Check that the folder exists and is a Git repository." : undefined}
              onRetry={openError.path !== "" ? () => void openRepoTab(openError.path) : undefined}
              onDismiss={() => setOpenError(null)}
            />
          )}
        </div>
      )}
      {releaseNotesView !== null && (
        <ReleaseNotesModal entries={releaseNotesView.entries} onClose={closeReleaseNotes} />
      )}
      {pickingRepo && (
        <Overlay onClose={() => setPickingRepo(false)}>
          <RepoPicker
            client={client}
            onOpenRepo={(path) => {
              void openRepoTab(path);
              setPickingRepo(false);
            }}
            onOpenWorkspace={(workspace) => {
              void openRepos.openWorkspace(workspace);
              setPickingRepo(false);
            }}
            workspaces={workspaces.workspaces}
            workspacesLoading={workspaces.loading}
            workspacesError={workspaces.error}
            onDismissWorkspacesError={workspaces.dismissError}
            onCreateWorkspace={workspaces.createWorkspace}
            onEditWorkspace={workspaces.editWorkspace}
            onDeleteWorkspace={workspaces.deleteWorkspace}
            onCloneRepository={appCommands?.onCloneRepository}
            onNewRepository={appCommands?.onNewRepository}
          />
        </Overlay>
      )}
      {cloning && (
        // FormDialog already blocks Escape while running; this guard is belt-and-braces for the
        // backdrop path so a running clone is never dismissed invisibly.
        <Overlay
          onClose={() => {
            if (!cloneRunning.current) setCloning(false);
          }}
        >
          <CloneDialog
            client={client}
            onCancel={() => setCloning(false)}
            onRunningChange={(running) => (cloneRunning.current = running)}
            onCloned={(outcome, name) => {
              setCloning(false);
              void openRepoTab(outcome.path);
              pushAppToast(cloneToastMessage(outcome, name));
            }}
          />
        </Overlay>
      )}
      {existingRepoPrompt !== null && (
        <ConfirmDialog
          ariaLabel="Repository already exists"
          message={<p>{existingRepoPrompt} is already a repository. Open it?</p>}
          confirmLabel="Open"
          onConfirm={() => {
            void openRepoTab(existingRepoPrompt);
            setExistingRepoPrompt(null);
          }}
          onCancel={() => setExistingRepoPrompt(null)}
        />
      )}
      <ToastRegion toasts={appToasts.toasts} onDismiss={appToasts.dismiss} />
      {openRepos.openRepos.length === 0 ? (
        <RepoPicker
          client={client}
          onOpenRepo={openRepoTab}
          onOpenWorkspace={(workspace) => void openRepos.openWorkspace(workspace)}
          workspaces={workspaces.workspaces}
          workspacesLoading={workspaces.loading}
          workspacesError={workspaces.error}
          onDismissWorkspacesError={workspaces.dismissError}
          onCreateWorkspace={workspaces.createWorkspace}
          onEditWorkspace={workspaces.editWorkspace}
          onDeleteWorkspace={workspaces.deleteWorkspace}
          onCloneRepository={appCommands?.onCloneRepository}
          onNewRepository={appCommands?.onNewRepository}
        />
      ) : (
        // PERF-001: only the active tab's workspace is mounted — an inactive one renders nothing
        // rather than staying mounted `display: none`. `RepoTabs` above still renders every open
        // tab (so switching away and back is possible); this just avoids paying N tabs' worth of
        // steady-state DOM/IPC cost while only one is ever visible. Switching back remounts a
        // fresh `RepoWorkspace`, which re-fetches its own status/graph on mount — already the
        // existing behavior for a newly-opened repo, so no new loading-state code is needed.
        openRepos.openRepos
          .filter((repo) => repo.path === openRepos.activePath)
          .map((repo) => (
            <RepoWorkspace
              key={repo.path}
              repoPath={repo.path}
              active
              client={client}
              onOpenRepoTab={openRepoTab}
              onBusyChange={onBusyChange}
              openRepos={openRepos.openRepos}
              onSwitchRepoTab={openRepos.switchTo}
              onCloseRepoTab={openRepos.closeRepo}
              getCommitDraft={getCommitDraft}
              onCommitDraftChange={onCommitDraftChange}
              appCommands={appCommands}
              pushToast={pushAppToast}
            />
          ))
      )}
    </main>
  );
}
