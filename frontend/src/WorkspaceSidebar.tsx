import type { CommitGraphLayout } from "./lib/useCommitGraphLayout";
import { BranchTree } from "./components/BranchTree";
import { Sidebar } from "./components/primitives/Sidebar";
import { SkeletonRows } from "./components/primitives/SkeletonRows";
import { PullRequestPanel } from "./components/PullRequestPanel";
import { ReflogPanel } from "./components/ReflogPanel";
import { StashPanel } from "./components/StashPanel";
import { SubmodulePanel } from "./components/SubmodulePanel";
import { TagPanel } from "./components/TagPanel";
import { WorktreePanel } from "./components/WorktreePanel";
import type { StashBlockers } from "./lib/actions/stashActions";
import type { useAnnounce } from "./state/useAnnounce";
import type { SelectedRow, UseAppStateResult } from "./state/useAppState";
import type { UseCompareResult } from "./state/useCompare";
import type { UseGoToResult } from "./state/useGoTo";
import type { useSidebarPanelVisibility } from "./state/useSidebarPanelVisibility";
import { sectionRevealRequest, type SidebarRevealRequest } from "./state/useSidebarReveal";

/** `RepoWorkspace`'s sidebar (#491): the branch tree and the panels the user has switched on. */
export function WorkspaceSidebar({
  appState,
  announce,
  operationDisabled: repositoryOperationDisabled,
  operationDisabledReason,
  panelVisibility,
  compare,
  goTo,
  onSelectRow: selectRowClosingCompare,
  onDropStash: dropStash,
  stashBlockers: storedStashBlockers,
  onOpenRepoTab,
  revealRequest,
  graphLayout,
}: {
  appState: UseAppStateResult;
  announce: ReturnType<typeof useAnnounce>;
  // The one "repository busy" rule and its reason (`operationBlockers`, #495).
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  panelVisibility: ReturnType<typeof useSidebarPanelVisibility>;
  compare: UseCompareResult;
  goTo: UseGoToResult;
  // Selects a row, closing an open compare.
  onSelectRow: (row: SelectedRow) => void;
  onDropStash: (index: number) => Promise<void>;
  stashBlockers: StashBlockers;
  onOpenRepoTab: (path: string) => Promise<void>;
  // The palette's sidebar reveal request (`useSidebarReveal`, #494), `null` when none.
  revealRequest: SidebarRevealRequest | null;
  // The graph's lanes, for the branch swatches' colours (#673).
  graphLayout: CommitGraphLayout;
}) {
  const { openDialog } = appState;
  return (
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
      {!appState.state.loaded ? (
        // The first reads are still out (#660): placeholder rows, not "no branches".
        <SkeletonRows label="Loading branches…" rows={6} />
      ) : (
        <BranchTree
          branches={appState.state.branches}
          createBranchDraft={appState.state.createBranchDraft}
          onSwitchBranch={announce((name) => `Switched to ${name}`, appState.switchBranch, { undoable: true })}
          onCreateBranch={appState.createBranch}
          onDeleteBranch={announce((...[name]) => `Deleted branch ${name}`, appState.deleteBranch, { undoable: true })}
          onRenameBranch={appState.renameBranch}
          onOpenCreateBranchDraft={appState.openCreateBranchDraft}
          onCloseCreateBranchDraft={appState.closeCreateBranchDraft}
          onMergeBranch={(branchName) => openDialog({ kind: "merge", branchName })}
          onRebaseOnto={appState.openRebasePlanner}
          operationDisabled={repositoryOperationDisabled}
          operationDisabledReason={operationDisabledReason}
          graphBranchSelection={appState.state.graphSelection.local}
          onSetGraphBranchSelection={appState.setGraphBranchSelection}
          onClearGraphBranchSelection={appState.clearGraphBranchSelection}
          onShowAllGraphBranches={appState.showAllGraphBranches}
          graphRemoteBranchSelection={appState.state.graphSelection.remote}
          onSetGraphRemoteBranchSelection={appState.setGraphRemoteBranchSelection}
          onIsolateGraphBranch={appState.isolateGraphBranch}
          remoteBranchesInGraph={Array.from(new Set(appState.state.commits.flatMap((c) => c.remoteBranchRefs)))}
          branchColors={graphLayout.branchColors}
          remotes={appState.state.remotes}
          upstream={appState.state.upstream}
          onAddRemote={appState.addRemote}
          onSetUpstream={appState.setCurrentUpstream}
          onClearUpstream={appState.clearCurrentUpstream}
          onListRemoteBranches={appState.listRemoteBranches}
          onFetchRemote={appState.fetchRemote}
          onPushCurrentBranch={appState.pushCurrentBranch}
          addRemoteDraftOpen={appState.state.addRemoteDraftOpen}
          onOpenAddRemoteDraft={appState.openAddRemoteDraft}
          onCloseAddRemoteDraft={appState.closeAddRemoteDraft}
          onOpenPushDialog={(localBranch) => openDialog({ kind: "push", localBranch })}
          onOpenDeleteRemoteBranch={(remoteName, branchName) => openDialog({ kind: "deleteRemoteBranch", remoteName, branchName })}
          onCompareBranch={compare.compareBranch}
          onGoToBranch={(kind, name) => void goTo.goToBranch(kind, name)}
          refreshGeneration={appState.state.refreshGeneration}
          onOpenRemoteDialog={openDialog}
          revealRequest={revealRequest?.section === "branches" ? revealRequest : undefined}
        />
      )}
      {panelVisibility.visibility.stash && (
        <StashPanel
          stashes={appState.state.stashes}
          onSelectRow={selectRowClosingCompare}
          onApplyStash={(index, restoreIndex) => void appState.applyStash(index, restoreIndex)}
          onPopStash={(index, restoreIndex) => void appState.popStash(index, restoreIndex)}
          onDropStash={dropStash}
          blockers={storedStashBlockers}
          revealRequest={sectionRevealRequest(revealRequest, "stash")}
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
          revealRequest={sectionRevealRequest(revealRequest, "worktree")}
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
          revealRequest={sectionRevealRequest(revealRequest, "submodule")}
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
          revealRequest={sectionRevealRequest(revealRequest, "reflog")}
        />
      )}
      {panelVisibility.visibility.tags && (
        <TagPanel
          tags={appState.state.tags}
          remotes={appState.state.remotes}
          onDelete={appState.deleteTag}
          onDeleteFromRemote={(tagName) => openDialog({ kind: "deleteRemoteTag", tagName })}
          onPush={appState.pushTags}
          onPushTag={(remoteName, tag) => void appState.pushTags(remoteName, [tag])}
          onBranchFromCommit={appState.openCreateBranchDraft}
          onCheckoutCommit={announce((commitId) => `Checked out ${commitId.slice(0, 7)}`, appState.checkoutCommit, { undoable: true })}
          onGoToTag={(tag) => void goTo.goToTag(tag.name, tag.commitId)}
          pushDisabled={repositoryOperationDisabled}
          operationDisabledReason={operationDisabledReason}
          revealRequest={sectionRevealRequest(revealRequest, "tags")}
          onCreateTagAtHead={
            appState.state.headInfo.headId === null
              ? undefined
              : () => {
                  const headId = appState.state.headInfo.headId;
                  if (headId !== null) openDialog({ kind: "createTag", commitId: headId });
                }
          }
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
          revealRequest={sectionRevealRequest(revealRequest, "pullRequests")}
        />
      )}
    </Sidebar>
  );
}
