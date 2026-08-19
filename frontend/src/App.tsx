import { useEffect, useState } from "react";
import { Moon, Sun } from "lucide-react";
import { BranchSwitcher } from "./components/BranchSwitcher";
import { CommandPalette } from "./components/CommandPalette";
import { CommitGraph } from "./components/CommitGraph";
import { DiffPane } from "./components/DiffPane";
import { LaneBraid } from "./components/LaneBraid";
import { RebasePlanner } from "./components/RebasePlanner";
import { ReflogPanel } from "./components/ReflogPanel";
import { RepoPicker } from "./components/RepoPicker";
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
import styles from "./App.module.css";

export default function App() {
  const appState = useAppState(tauriRepoClient);
  const [theme, setTheme] = useState<Theme>(() =>
    resolveTheme(
      loadStoredTheme(),
      window.matchMedia("(prefers-color-scheme: dark)").matches,
    ),
  );
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const [paletteOpen, setPaletteOpen] = useState(false);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        if (appState.state.repoPath === null) return;
        event.preventDefault();
        setPaletteOpen((prev) => !prev);
      }
    }
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [appState.state.repoPath]);

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
  const repositoryOperationDisabled =
    appState.state.pending ||
    appState.state.transfer !== null ||
    appState.state.mergeMessage !== null ||
    appState.state.rebaseProgress !== null;

  // E2E-only auto-open: `RepoPicker`'s native folder dialog can't be driven through WebDriver,
  // so the E2E build points at a fixture repo via this Vite env var instead. Statically absent
  // from a normal production build unless VITE_E2E_REPO_PATH is set at build time.
  useEffect(() => {
    const autoOpenPath = import.meta.env.VITE_E2E_REPO_PATH;
    if (typeof autoOpenPath === "string" && autoOpenPath.length > 0) {
      appState.openRepo(autoOpenPath);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (appState.state.repoPath === null) {
    return (
      <main>
        <header className={styles.headerRow}>
          <h1>Browsitory</h1>
          {themeToggle}
        </header>
        <LaneBraid />
        {/* `RepoPicker` only surfaces errors from its own `pickRepoFolder`/`listRecentRepos`
            calls; an `onOpenRepo` rejection (bad path, a stale recent-repo entry, permissions)
            lands in `useAppState`'s `state.error`, which is otherwise only rendered in the
            post-open branch below — leaving a failed open looking like nothing happened. */}
        {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
        <RepoPicker client={tauriRepoClient} onOpenRepo={appState.openRepo} />
      </main>
    );
  }

  return (
    <main>
      <header className={styles.headerRow}>
        <h1>Browsitory</h1>
        {themeToggle}
      </header>
      <LaneBraid />
      {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
      {appState.state.transfer !== null && (
        <Overlay>
          <TransferPanel progress={appState.state.transfer} />
        </Overlay>
      )}
      {paletteOpen && (
        <Overlay onClose={() => setPaletteOpen(false)}>
          <CommandPalette commands={buildCommands(appState)} onRun={() => setPaletteOpen(false)} />
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
              onOpenWorktree={appState.openRepo}
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
            client={tauriRepoClient}
            onto={appState.state.rebaseOnto}
            onStartRebase={appState.startRebase}
            onCancel={appState.closeRebasePlanner}
            operationDisabled={repositoryOperationDisabled}
          />
        </Overlay>
      )}
    </main>
  );
}
