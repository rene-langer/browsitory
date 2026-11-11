import { UncommittedDiffPane } from "./UncommittedDiffPane";
import { CommitDiffPane } from "./CommitDiffPane";
import { StashDiffPane } from "./StashDiffPane";
import { NO_HEAD_INFO, type CommitView, type DiscardRequest, type FileHistoryFocus } from "./diffPaneShared";
import { useState } from "react";
import { DiffViewOptionsProvider } from "../state/DiffViewOptionsProvider";
import type { ForgeRepository, GraphCommit, HeadInfo, RepoClient, StashEntry, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import type { CommitDraft } from "./CommitBox";

export type {
  CommitHooksControls,
  CommitView,
  DiffPanePatchActions,
  DiscardRequest,
  FileHistoryFocus,
  ReportedFileSelection,
  StashActions,
} from "./diffPaneShared";

/**
 * The diff pane: the working tree's changes and the commit dock, a commit's files, or a stash.
 * Its actions come from `WorkspaceActionsContext` (#491); the props are the data it shows.
 */
export function DiffPane(props: Parameters<typeof DiffPaneContent>[0]) {
  // The pane's diffs and its one options toolbar share Split and whole file (#640); under the
  // app's provider that is the app's.
  return (
    <DiffViewOptionsProvider>
      <DiffPaneContent {...props} />
    </DiffViewOptionsProvider>
  );
}

function DiffPaneContent({
  repoPath,
  client,
  selectedRow,
  status,
  mergeMessage,
  amendMode = false,
  headInfo = NO_HEAD_INFO,
  focusCommitMessage = false,
  clearCommitMessage = null,
  rebaseProgress,
  sequencePending = false,
  commits,
  branchCommits,
  refreshGeneration = 0,
  initialCommitDraft,
  discardRequest,
  onDiscardRequestChange,
  conflictRequest = false,
  onConflictRequestHandled,
  statusLoaded = true,
  fileHistoryFocus,
  stashes = [],
  selectionBusyReason = null,
  discardBlockedReason = null,
  restoreBlockedReason = null,
  issueForge = null,
}: {
  // The forge repository a selected commit's `#123` links to (#612), else `null`.
  issueForge?: ForgeRepository | null;
  // Why Restore… can't run now (`operationBlockers`), else `null`.
  restoreBlockedReason?: string | null;
  // Why the selection's actions can't run now (another action is in progress), else `null`.
  selectionBusyReason?: string | null;
  // Why the discard confirmations can't discard now (`busyReason`: an action in flight or a
  // transfer; a paused merge, rebase or sequence step allows it, #521), else `null`.
  discardBlockedReason?: string | null;
  // The stash list: a selected commit id that is a stash's renders the stash view instead of
  // the commit view.
  stashes?: StashEntry[];
  // Set while the graph shows a file's history: the commit view shows only that file.
  fileHistoryFocus?: FileHistoryFocus | null;
  repoPath: string;
  client: RepoClient;
  selectedRow: SelectedRow;
  // `useAppState`'s `state.refreshGeneration`: advances on every `refresh()`, so any refresh
  // (not just a hunk action taken in this pane) invalidates every open working-tree diff. Optional
  // only so tests that don't exercise refreshes needn't pass it.
  refreshGeneration?: number;
  // Discard confirmation state, owned by `RepoWorkspace` so the command palette opens the same
  // dialogs as the file menu and "Discard all…" (#493). Always controlled: tests hold it in a
  // small wrapper.
  discardRequest: DiscardRequest | null;
  onDiscardRequestChange: (request: DiscardRequest | null) => void;
  /** "Go to conflict" (#665), passed through to `UncommittedDiffPane`. */
  conflictRequest?: boolean;
  onConflictRequestHandled?: () => void;
  /** The first status read is in: an empty status is a clean tree, not one still loading. */
  statusLoaded?: boolean;
  // Passed straight through to `CommitBox` — see its `initialDraft`.
  initialCommitDraft?: CommitDraft;
  // Loaded history, used for the selected commit's header (author, date, parents).
  commits?: GraphCommit[];
  // The graph's own commits (`commits` may be a file history or search results): the commit
  // box's "Recent messages" offers the current branch's recent ones.
  branchCommits?: GraphCommit[];
  status: StatusEntry[];
  mergeMessage: string | null;
  amendMode?: boolean;
  headInfo?: HeadInfo;
  focusCommitMessage?: boolean;
  // One-shot CommitBox clear request (see its `clearMessage`).
  clearCommitMessage?: string | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  // An action is running: the sequence's Continue/Abort are disabled (no double submit).
  sequencePending?: boolean;
}) {
  const { stashActions } = useWorkspaceActions();
  // A commit's Changes / All files view (#446), kept while moving between commits.
  const [commitView, setCommitView] = useState<CommitView>("changes");
  if (selectedRow === "uncommitted") {
    return (
      <UncommittedDiffPane
        repoPath={repoPath}
        client={client}
        status={status}
        mergeMessage={mergeMessage}
        amendMode={amendMode}
        headInfo={headInfo}
        focusCommitMessage={focusCommitMessage}
        clearCommitMessage={clearCommitMessage}
        rebaseProgress={rebaseProgress}
        sequencePending={sequencePending}
        refreshGeneration={refreshGeneration}
        initialCommitDraft={initialCommitDraft}
        discardRequest={discardRequest}
        onDiscardRequestChange={onDiscardRequestChange}
        conflictRequest={conflictRequest}
        onConflictRequestHandled={onConflictRequestHandled}
        statusLoaded={statusLoaded}
        selectionBusyReason={selectionBusyReason}
        discardBlockedReason={discardBlockedReason}
        commits={commits}
        branchCommits={branchCommits}
      />
    );
  }
  const stash = stashes.find((entry) => entry.commitId === selectedRow.commitId);
  if (stash !== undefined && stashActions !== undefined) {
    return <StashDiffPane key={stash.commitId} repoPath={repoPath} client={client} stash={stash} actions={stashActions} />;
  }
  return (
    <CommitDiffPane
      key={selectedRow.commitId}
      repoPath={repoPath}
      client={client}
      commitId={selectedRow.commitId}
      commits={commits}
      focus={fileHistoryFocus ?? null}
      view={commitView}
      onViewChange={setCommitView}
      restoreBlockedReason={restoreBlockedReason}
      issueForge={issueForge}
    />
  );
}
