import { useCallback, useEffect, useMemo, useState } from "react";
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
import { WorktreePanel } from "./components/WorktreePanel";
import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { buildCommands } from "./lib/commands";
import { applyTheme, loadStoredTheme, persistTheme, resolveTheme, type Theme } from "./lib/theme";
import { useAppState } from "./state/useAppState";
import { useOpenRepos, type OpenRepo } from "./state/useOpenRepos";
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
    <div style={{ display: active ? "contents" : "none" }}>
      {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
      {appState.state.transfer !== null && (
        <Overlay>
          <TransferPanel progress={appState.state.transfer} />
        </Overlay>
      )}
      {paletteOpen && (
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
                stashes={appState.state.stashes}
                selectedRow={appState.state.selectedRow}
                pending={repositoryOperationDisabled}
                onSelectRow={appState.selectRow}
                onBranchFromCommit={appState.openCreateBranchDraft}
                onRebaseFromCommit={appState.openRebasePlanner}
                onApplyStash={appState.applyStash}
                onDropStash={appState.dropStash}
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
      {appState.state.rebaseOnto !== null && (
        <Overlay onClose={appState.closeRebasePlanner}>
          <RebasePlanner
            repoPath={repoPath}
            client={tauriRepoClient}
            onto={appState.state.rebaseOnto}
            onStartRebase={appState.startRebase}
            onCancel={appState.closeRebasePlanner}
            operationDisabled={repositoryOperationDisabled}
          />
        </Overlay>
      )}
    </div>
  );
}

export default function App() {
  const openRepos = useOpenRepos(tauriRepoClient);
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
  useEffect(() => {
    // Wait for the persisted-tab restore to settle first: on the mount pass `openRepos` is still
    // the empty initial value, so without this the fixture would be opened concurrently with
    // `listOpenRepos`, and whichever promise resolved last would clobber the other's tab list.
    if (openRepos.loading) return;
    const autoOpenPath = import.meta.env.VITE_E2E_REPO_PATH;
    if (typeof autoOpenPath === "string" && autoOpenPath.length > 0 && openRepos.openRepos.length === 0) {
      void openRepoTab(autoOpenPath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openRepos.loading]);

  if (openRepos.loading) {
    return null;
  }

  return (
    <main>
      <header className={styles.headerRow}>
        <h1>Browsitory</h1>
        <RepoTabs
          openRepos={openRepos.openRepos}
          activePath={openRepos.activePath}
          busyPaths={busyPaths}
          onSwitchTo={openRepos.switchTo}
          onClose={openRepos.closeRepo}
          onAddTab={() => setPickingRepo(true)}
        />
        {themeToggle}
      </header>
      <LaneBraid />
      {openError !== null && <p role="alert">{openError}</p>}
      {pickingRepo && (
        <Overlay onClose={() => setPickingRepo(false)}>
          <RepoPicker
            client={tauriRepoClient}
            onOpenRepo={(path) => {
              void openRepoTab(path);
              setPickingRepo(false);
            }}
          />
        </Overlay>
      )}
      {openRepos.openRepos.length === 0 ? (
        <RepoPicker client={tauriRepoClient} onOpenRepo={openRepoTab} />
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
