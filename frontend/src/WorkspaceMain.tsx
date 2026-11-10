import { useMemo, useRef } from "react";
import type { CommitDraft } from "./components/CommitBox";
import { CommitGraph } from "./components/CommitGraph";
import { CommitSearchBar } from "./components/CommitSearchBar";
import { CommitSearchResults } from "./components/CommitSearchResults";
import { CompareDiffPane } from "./components/CompareDiffPane";
import { DiffPane, type DiscardRequest } from "./components/DiffPane";
import { FileHistoryList } from "./components/FileHistoryList";
import { GoToNotice } from "./components/GoToNotice";
import { SplitView } from "./components/primitives/SplitView";
import type { RepoClient } from "./ipc/RepoClient";
import { issueForgeRepository } from "./lib/messageLinks";
import { sequenceToast, sequenceUndoable } from "./lib/operationStatus";
import type { useAnnounce } from "./state/useAnnounce";
import type { SelectedRow, UseAppStateResult } from "./state/useAppState";
import type { UseCommitSearchResult } from "./state/useCommitSearch";
import type { UseCompareResult } from "./state/useCompare";
import type { ExternalToolsApi } from "./state/useExternalTools";
import type { UseFileHistoryResult } from "./state/useFileHistory";
import type { UseGoToResult } from "./state/useGoTo";
import type { VisitKind } from "./lib/navigationHistory";
import { commitsPatchName } from "./state/usePatchActions";
import type { useSequenceActions } from "./state/useSequenceActions";
import { useCommitLineStats } from "./state/useCommitLineStats";
import { useShowLineStats, writeShowLineStats } from "./lib/lineStatsPreference";

/**
 * `RepoWorkspace`'s main area (#491): the commit list (the graph, search results or a file's
 * history) beside the diff pane (or a compare). The sync actions are in `RepoToolbar` (#637). The diff pane's actions come from
 * `WorkspaceActionsContext`.
 */
export function WorkspaceMain({
  repoPath,
  client,
  appState,
  announce,
  operationDisabled: repositoryOperationDisabled,
  operationDisabledReason,
  goTo,
  fileHistory,
  commitSearch,
  compare,
  searchFocusRequested,
  onSearchFocusHandled: clearSearchFocusRequest,
  onRequestSearch: requestSearchFocus,
  onSelectRow: selectRowClosingCompare,
  revealRequest,
  onCherryPick: cherryPickCommit,
  onRevert: revertCommit,
  runPickAction,
  onOpenCommitDialog: openCommitDialog,
  onEditHeadMessage,
  onOpenFolderHistory: openFolderHistory,
  externalTools,
  discardRequest,
  onDiscardRequestChange: setDiscardRequest,
  initialCommitDraft,
  discardBlockedReason: discardDropBlockedReason,
  focusCommitMessage,
}: {
  repoPath: string;
  client: RepoClient;
  appState: UseAppStateResult;
  announce: ReturnType<typeof useAnnounce>;
  // The one "repository busy" rule and its reason (`operationBlockers`, #495).
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  goTo: UseGoToResult;
  fileHistory: UseFileHistoryResult;
  commitSearch: UseCommitSearchResult;
  compare: UseCompareResult;
  // Ctrl/Cmd+F or the palette asked to focus the search field.
  searchFocusRequested: boolean;
  onSearchFocusHandled: () => void;
  onRequestSearch: () => void;
  // Selects a row, closing an open compare; `kind` as `selectRow`'s.
  onSelectRow: (row: SelectedRow, kind?: VisitKind) => void;
  // "Go to" (#441): bumped to scroll the graph to the selected row.
  revealRequest: number;
  onCherryPick: (commitId: string, mainline: number | null) => void;
  onRevert: (commitId: string, mainline: number | null) => void;
  runPickAction: ReturnType<typeof useSequenceActions>["runPickAction"];
  // Opens the parent picker or the reset dialog of one commit (#507).
  onOpenCommitDialog: (kind: "Cherry-pick" | "Revert" | "Reset", commitId: string, returnFocusTo?: HTMLElement | null) => void;
  // "Edit commit message…" on HEAD: amend mode with the message field focused.
  onEditHeadMessage: () => void;
  onOpenFolderHistory: (folder: string) => void;
  externalTools: ExternalToolsApi;
  discardRequest: DiscardRequest | null;
  onDiscardRequestChange: (request: DiscardRequest | null) => void;
  initialCommitDraft: CommitDraft | undefined;
  // Why discarding can't run now (`busyReason`, #521), else `null`.
  discardBlockedReason: string | null;
  focusCommitMessage: boolean;
}) {
  const searchResultsRef = useRef<HTMLUListElement>(null);
  // #620: the commit list's lines-changed badges, counted as rows come into view.
  const showLineStats = useShowLineStats();
  const lineStats = useCommitLineStats(client, repoPath);
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
  // #612: the repository a commit message's `#123` links to.
  const upstreamRemote = appState.state.upstream?.remoteName ?? null;
  const issueForge = useMemo(
    () => issueForgeRepository(appState.state.forgeRepositories, upstreamRemote),
    [appState.state.forgeRepositories, upstreamRemote],
  );
  // #448: "Save as patch…" for commits (oldest first).
  const savePatchOfCommits = (commitIds: string[]) =>
    void appState.savePatch({ kind: "Commits", commitIds }, commitsPatchName(commitIds, appState.state.commits));
  return (
    <SplitView
      storageKey="history-diff-width"
      defaultWidth={420}
      minWidth={280}
      maxWidth={800}
      label="History and diff width"
      left={
        <>
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
              showLineStats={showLineStats}
              onToggleLineStats={() => writeShowLineStats(!showLineStats)}
              collapseMerges={appState.state.collapseMerges}
              onToggleCollapseMerges={() => void appState.setCollapseMerges(!appState.state.collapseMerges)}
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
                onGoToCommit={(commitId) => void goTo.goToRevision(commitId)}
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
                    sequenceUndoable,
                  )
                }
                onRevertMany={(ids) =>
                  void runPickAction(
                    () => appState.revertMany(ids),
                    (outcome) => sequenceToast("Revert", outcome),
                    sequenceUndoable,
                  )
                }
                onCheckoutCommit={announce((commitId) => `Checked out ${commitId.slice(0, 7)}`, appState.checkoutCommit, { undoable: true })}
                onChooseParent={openCommitDialog}
                onRequestReset={(commitId, returnFocusTo) => openCommitDialog("Reset", commitId, returnFocusTo)}
                onEditHeadMessage={onEditHeadMessage}
                onEditHistory={(commitId, onto, request) => void appState.editHistory(commitId, onto, request)}
                onRewordCommit={(commitId, onto) => appState.openDialog({ kind: "reword", commitId, onto })}
                graphRemoteBranchSelection={appState.state.graphSelection.remote}
                remotes={appState.state.remotes}
                onPushTag={(remoteName, tag) => void announce("Push complete", appState.pushTags)(remoteName, [tag])}
                onDeleteTag={appState.deleteTag}
                onCreateTag={(commitId) => appState.openDialog({ kind: "createTag", commitId })}
                onDeleteTagFromRemote={(tagName) => appState.openDialog({ kind: "deleteRemoteTag", tagName })}
                revealRequest={revealRequest}
                lineStats={showLineStats ? lineStats.stats : undefined}
                onCommitsVisible={showLineStats ? lineStats.request : undefined}
                expandedMerges={appState.state.expandedMerges}
                onExpandMerge={(mergeId, atLeast) => void appState.expandMerge(mergeId, atLeast)}
                onCollapseMerge={appState.collapseMerge}
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
            restoreBlockedReason={operationDisabledReason}
            issueForge={issueForge}
            status={appState.state.status}
            refreshGeneration={appState.state.refreshGeneration}
            discardRequest={discardRequest}
            onDiscardRequestChange={setDiscardRequest}
            initialCommitDraft={initialCommitDraft}
            discardBlockedReason={discardDropBlockedReason}
            selectionBusyReason={appState.state.pending ? "Another action is in progress." : null}
            mergeMessage={appState.state.mergeMessage}
            sequencePending={appState.state.pending}
            amendMode={appState.state.amendMode}
            headInfo={appState.state.headInfo}
            focusCommitMessage={focusCommitMessage}
            clearCommitMessage={appState.state.clearCommitMessage}
            rebaseProgress={appState.state.rebaseProgress}
          />
        )
      }
    />
  );
}
