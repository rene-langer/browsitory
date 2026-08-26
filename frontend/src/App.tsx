import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Moon, Sun } from "lucide-react";
import { BranchSwitcher } from "./components/BranchSwitcher";
import { CommandPalette } from "./components/CommandPalette";
import { CommitGraph } from "./components/CommitGraph";
import { DiffPane } from "./components/DiffPane";
import { LaneBraid } from "./components/LaneBraid";
import { RebasePlanner } from "./components/RebasePlanner";
import { ReflogPanel } from "./components/ReflogPanel";
import { RepoPicker } from "./components/RepoPicker";
import { RepoTabs } from "./components/RepoTabs";
import { Overlay } from "./components/primitives/Overlay";
import { Sidebar } from "./components/primitives/Sidebar";
import { SplitView } from "./components/primitives/SplitView";
import { PullRequestPanel } from "./components/PullRequestPanel";
import { RemotePanel } from "./components/RemotePanel";
import { TagPanel } from "./components/TagPanel";
import { SubmodulePanel } from "./components/SubmodulePanel";
import { TransferPanel } from "./components/TransferPanel";
import { UpdateBanner } from "./components/UpdateBanner";
import { WorktreePanel } from "./components/WorktreePanel";
import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { buildCommands } from "./lib/commands";
import { applyTheme, loadStoredTheme, persistTheme, resolveTheme, type Theme } from "./lib/theme";
import { useAppState } from "./state/useAppState";
import { useOpenRepos, type OpenRepo } from "./state/useOpenRepos";
import { useWorkspaces } from "./state/useWorkspaces";
import styles from "./App.module.css";

function RepoWorkspace({
  repoPath,
  active,
  onOpenRepoTab,
  onBusyChange,
  openRepos,
  onSwitchRepoTab,
}: {
  repoPath: string;
  active: boolean;
  // `Promise<void>` rather than the plan's `void`: `WorktreePanel`'s `onOpenWorktree` is typed
  // `(path: string) => Promise<void>`, and this is the only value passed to it. It stays
  // assignable to `buildCommands`' `(path: string) => void` parameter (Task 8).
  onOpenRepoTab: (path: string) => Promise<void>;
  onBusyChange: (repoPath: string, busy: boolean) => void;
  openRepos: OpenRepo[];
  onSwitchRepoTab: (path: string) => void;
}) {
  const appState = useAppState(tauriRepoClient, repoPath);
  const [paletteOpen, setPaletteOpen] = useState(false);

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

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (!active) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((prev) => !prev);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [active]);

  const repositoryOperationDisabled =
    appState.state.pending ||
    appState.state.transfer !== null ||
    appState.state.mergeMessage !== null ||
    appState.state.rebaseProgress !== null;

  // Closing this tab while a transfer/merge/rebase is in progress would orphan it mid-operation
  // — report busy status up so `App`'s `RepoTabs` can disable this tab's close button, the same
  // rule that already disables every other mutating action while this is true.
  useEffect(() => {
    onBusyChange(repoPath, repositoryOperationDisabled);
  }, [repoPath, repositoryOperationDisabled, onBusyChange]);

  return (
    // `data-active-repo` marks which workspace is the visible one. Every tab's `RepoWorkspace`
    // stays mounted (only CSS-hidden when inactive), so a document-wide `querySelector` would
    // always hit whichever tab is first in document order — `commands.ts`'s `goToSidebarSection`
    // scopes its lookup to this attribute so "Go to <section>" targets the tab the user is
    // actually looking at.
    <div style={{ display: active ? "contents" : "none" }} data-active-repo={active ? "true" : "false"}>
      {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
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
          <TransferPanel progress={appState.state.transfer} />
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
            )}
            onRun={() => setPaletteOpen(false)}
          />
        </Overlay>
      )}
      <SplitView
        storageKey="sidebar-width"
        defaultWidth={260}
        minWidth={200}
        maxWidth={420}
        collapsible
        label="Sidebar width"
        left={
          <Sidebar>
            <BranchSwitcher
              branches={appState.state.branches}
              createBranchDraft={appState.state.createBranchDraft}
              onSwitchBranch={appState.switchBranch}
              onCreateBranch={appState.createBranch}
              onDeleteBranch={appState.deleteBranch}
              onRenameBranch={appState.renameBranch}
              onOpenCreateBranchDraft={appState.openCreateBranchDraft}
              onCloseCreateBranchDraft={appState.closeCreateBranchDraft}
              onMergeBranch={appState.mergeBranch}
              isMerging={appState.state.mergeMessage !== null}
              isRebasing={appState.state.rebaseProgress !== null}
              operationDisabled={repositoryOperationDisabled}
              stashes={appState.state.stashes}
              onSelectRow={appState.selectRow}
              onApplyStash={appState.applyStash}
              onDropStash={appState.dropStash}
              graphBranchSelection={appState.state.graphBranchSelection}
              onSetGraphBranchSelection={appState.setGraphBranchSelection}
            />
            <WorktreePanel
              worktrees={appState.state.worktrees}
              branches={appState.state.branches}
              onOpenWorktree={onOpenRepoTab}
              onCreateWorktree={appState.createWorktree}
              onRemoveWorktree={appState.removeWorktree}
              onPruneWorktrees={appState.pruneWorktrees}
              operationDisabled={repositoryOperationDisabled}
            />
            <SubmodulePanel
              submodules={appState.state.submodules}
              onInit={appState.initSubmodule}
              onUpdate={appState.updateSubmodule}
              operationDisabled={repositoryOperationDisabled}
            />
            <ReflogPanel
              references={appState.state.reflogRefs}
              selectedReference={appState.state.selectedReflogReference}
              entries={appState.state.reflog}
              onSelectReference={appState.selectReflogReference}
              onRestore={appState.restoreReflogEntry}
              operationDisabled={repositoryOperationDisabled}
            />
            <RemotePanel
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
              onFetchRemote={appState.fetchRemote}
              fetchDisabled={repositoryOperationDisabled}
              onPushCurrentBranch={appState.pushCurrentBranch}
              pushDisabled={repositoryOperationDisabled}
              onPull={appState.pullCurrentUpstream}
              pullDisabled={repositoryOperationDisabled}
              pendingPull={appState.state.pendingPull}
              pullOutcome={appState.state.pullOutcome}
              onMergePull={async (upstreamRef) => {
                appState.clearPendingPull();
                await appState.mergeBranch(upstreamRef);
              }}
              onRebasePull={(upstreamRef) => {
                appState.clearPendingPull();
                appState.openRebasePlanner(upstreamRef);
              }}
              onCancelPull={appState.clearPendingPull}
            />
            <TagPanel
              tags={appState.state.tags}
              remotes={appState.state.remotes}
              onCreate={appState.createTag}
              onDelete={appState.deleteTag}
              onPush={appState.pushTags}
              pushDisabled={repositoryOperationDisabled}
            />
            <PullRequestPanel
              forgeRepositories={appState.state.forgeRepositories}
              pullRequests={appState.state.pullRequests}
              onListPullRequests={appState.listPullRequests}
              onSaveForgeToken={appState.saveForgeToken}
              onForgetForgeToken={appState.forgetForgeToken}
              onCreatePullRequest={appState.createPullRequest}
              onOpenExternalUrl={appState.openExternalUrl}
              operationDisabled={repositoryOperationDisabled}
            />
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
              <CommitGraph
                status={appState.state.status}
                commits={appState.state.commits}
                selectedRow={appState.state.selectedRow}
                pending={repositoryOperationDisabled}
                onSelectRow={appState.selectRow}
                onBranchFromCommit={appState.openCreateBranchDraft}
                onRebaseFromCommit={appState.openRebasePlanner}
                onSquashCommits={appState.openSquashPlanner}
              />
            }
            right={
              <DiffPane
                repoPath={repoPath}
                client={tauriRepoClient}
                selectedRow={appState.state.selectedRow}
                status={appState.state.status}
                onStageFile={appState.stageFile}
                onUnstageFile={appState.unstageFile}
                onStageAllFiles={appState.stageAllFiles}
                onUnstageAllFiles={appState.unstageAllFiles}
                onStageHunk={appState.stageHunk}
                onUnstageHunk={appState.unstageHunk}
                onDiscardHunk={appState.discardHunk}
                onCommit={appState.commit}
                onSaveStash={appState.saveStash}
                onSelectRow={appState.selectRow}
                onResolveConflict={appState.resolveConflict}
                onResolveAddDeleteConflict={appState.resolveAddDeleteConflict}
                mergeMessage={appState.state.mergeMessage}
                onAbortMerge={appState.abortMerge}
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
            client={tauriRepoClient}
            onto={appState.state.rebaseOnto}
            onStartRebase={appState.startRebase}
            onCancel={appState.closeRebasePlanner}
            operationDisabled={repositoryOperationDisabled}
            presetSquashIds={appState.state.squashPreset ?? undefined}
          />
        </Overlay>
      )}
    </div>
  );
}

export default function App() {
  const openRepos = useOpenRepos(tauriRepoClient);
  const workspaces = useWorkspaces(tauriRepoClient);
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

  const [pickingRepo, setPickingRepo] = useState(false);

  // `RepoPicker` only surfaces errors from its own `pickRepoFolder`/`listRecentRepos` calls, and
  // `useOpenRepos.openRepo` deliberately rejects rather than opening a tab for a repo that failed
  // to open (bad path, a stale recent-repo entry, permissions). Nothing else catches that now
  // that `App` has no `useAppState` of its own, so a failed open would look like nothing
  // happened — the same trap the pre-tabs `App` carried a comment about.
  const [openError, setOpenError] = useState<string | null>(null);
  const { openRepo } = openRepos;
  const openRepoTab = useCallback(
    // Both branches settle asynchronously on purpose: the E2E auto-open effect below calls this
    // directly, and a synchronous `setOpenError` in the body would be a setState-in-effect.
    (path: string) =>
      openRepo(path).then(
        () => setOpenError(null),
        (error: unknown) => setOpenError(String(error)),
      ),
    [openRepo],
  );

  const themeToggle = (
    <button
      type="button"
      className={styles.themeToggle}
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
    return null;
  }

  return (
    <main>
      <UpdateBanner />
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
        {themeToggle}
      </header>
      <LaneBraid />
      {openRepos.restoreError !== null && <p role="alert">{openRepos.restoreError}</p>}
      {openError !== null && <p role="alert">{openError}</p>}
      {pickingRepo && (
        <Overlay onClose={() => setPickingRepo(false)}>
          <RepoPicker
            client={tauriRepoClient}
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
            onCreateWorkspace={workspaces.createWorkspace}
            onEditWorkspace={workspaces.editWorkspace}
            onDeleteWorkspace={workspaces.deleteWorkspace}
          />
        </Overlay>
      )}
      {openRepos.openRepos.length === 0 ? (
        <RepoPicker
          client={tauriRepoClient}
          onOpenRepo={openRepoTab}
          onOpenWorkspace={(workspace) => void openRepos.openWorkspace(workspace)}
          workspaces={workspaces.workspaces}
          workspacesLoading={workspaces.loading}
          workspacesError={workspaces.error}
          onCreateWorkspace={workspaces.createWorkspace}
          onEditWorkspace={workspaces.editWorkspace}
          onDeleteWorkspace={workspaces.deleteWorkspace}
        />
      ) : (
        openRepos.openRepos.map((repo) => (
          <RepoWorkspace
            key={repo.path}
            repoPath={repo.path}
            active={repo.path === openRepos.activePath}
            onOpenRepoTab={openRepoTab}
            onBusyChange={onBusyChange}
            openRepos={openRepos.openRepos}
            onSwitchRepoTab={openRepos.switchTo}
          />
        ))
      )}
    </main>
  );
}
