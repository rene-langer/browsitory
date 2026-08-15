import { useEffect } from "react";
import { BranchSwitcher } from "./components/BranchSwitcher";
import { CommitGraph } from "./components/CommitGraph";
import { DiffPane } from "./components/DiffPane";
import { RebasePlanner } from "./components/RebasePlanner";
import { RepoPicker } from "./components/RepoPicker";
import { RemotePanel } from "./components/RemotePanel";
import { TagPanel } from "./components/TagPanel";
import { TransferPanel } from "./components/TransferPanel";
import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { useAppState } from "./state/useAppState";

export default function App() {
  const appState = useAppState(tauriRepoClient);
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
        <h1>Browsitory</h1>
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
      <h1>Browsitory</h1>
      {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
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
      <RemotePanel
        remotes={appState.state.remotes}
        upstream={appState.state.upstream}
        remoteUpstreams={appState.state.remoteUpstreams}
        onAddRemote={appState.addRemote}
        onRenameRemote={appState.renameRemote}
        onUpdateRemoteUrls={appState.updateRemoteUrls}
        onRemoveRemote={appState.removeRemote}
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
      <TransferPanel progress={appState.state.transfer} />
      <div className="app-layout">
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
      </div>
      {appState.state.rebaseOnto !== null && (
        <RebasePlanner
          client={tauriRepoClient}
          onto={appState.state.rebaseOnto}
          onStartRebase={appState.startRebase}
          onCancel={appState.closeRebasePlanner}
          operationDisabled={repositoryOperationDisabled}
        />
      )}
    </main>
  );
}
