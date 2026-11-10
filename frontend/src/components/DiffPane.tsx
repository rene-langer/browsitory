import { UncommittedDiffPane } from "./UncommittedDiffPane";
import { CommitDiffPane } from "./CommitDiffPane";
import { StashDiffPane } from "./StashDiffPane";
import {
  NO_HEAD_INFO,
  noAmend,
  noop,
  type CommitHooksControls,
  type CommitView,
  type DiffPanePatchActions,
  type DiscardRequest,
  type FileHistoryFocus,
  type ReportedFileSelection,
  type StashActions,
} from "./diffPaneShared";
import { useState } from "react";
import type {
  AmendOptions,
  FileConflictChoice,
  GraphCommit,
  HeadInfo,
  HunkOptions,
  MarkResolvedOutcome,
  ConflictSide,
  RepoClient,
  StashEntry,
  StatusEntry,
  Trailer,
} from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import type { ExternalToolsApi } from "../state/useExternalTools";
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

export function DiffPane({
  repoPath,
  client,
  selectedRow,
  status,
  onStageFile,
  onUnstageFile,
  onStageAllFiles,
  onUnstageAllFiles,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onStageLines,
  onUnstageLines,
  onDiscardLines,
  onDiscardFile,
  onDiscardAllUnstaged,
  onIgnorePath,
  onCommit,
  onSaveStash,
  onOpenStashDialog = noop,
  onSelectRow,
  onResolveConflict,
  onResolveAddDeleteConflict,
  mergeMessage,
  onAbortMerge,
  amendMode = false,
  headInfo = NO_HEAD_INFO,
  onSetAmendMode = noop,
  onAmendCommit = noAmend,
  focusCommitMessage = false,
  onCommitMessageFocused,
  commitHooks,
  clearCommitMessage = null,
  onCommitMessageCleared,
  rebaseProgress,
  onRebaseContinue,
  onRebaseAbort,
  onSequenceContinue = noop,
  onSequenceAbort = noop,
  sequencePending = false,
  commits,
  branchCommits,
  refreshGeneration = 0,
  initialCommitDraft,
  onCommitDraftChange,
  discardRequest,
  onDiscardRequestChange,
  onCurrentFileChange,
  fileHistoryFocus,
  onOpenFileHistory,
  onOpenLineHistory,
  onOpenFolderHistory,
  stashes = [],
  stashActions,
  externalTools,
  onMarkResolved,
  onResolveWholeFile,
  onDiscardFiles,
  onIgnorePaths,
  onSelectionChange,
  selectionBusyReason = null,
  discardBlockedReason = null,
  onRestoreFileVersion,
  restoreBlockedReason = null,
  patchActions,
}: {
  // "All files" view of a commit (#446): Restore… writes a file's version to the work tree.
  onRestoreFileVersion?: (commitId: string, path: string) => void | Promise<void>;
  // Why Restore… can't run now (`operationBlockers`), else `null`.
  restoreBlockedReason?: string | null;
  // Revert / apply a commit's file or hunk, "Save as patch…" (#448); omitted, none of them show.
  patchActions?: DiffPanePatchActions;
  // A multi-file selection's discard (#449): exactly the confirmed paths, one undo step.
  // Defaults to `onDiscardAllUnstaged`.
  onDiscardFiles?: (paths: string[]) => void;
  // A multi-file selection's "Ignore": one `/path` pattern per untracked file. Defaults to one
  // `onIgnorePath` per pattern.
  onIgnorePaths?: (patterns: string[]) => void;
  // Reports the working-tree selection while it holds two or more files, else `null`.
  onSelectionChange?: (selection: ReportedFileSelection | null) => void;
  // Why the selection's actions can't run now (another action is in progress), else `null`.
  selectionBusyReason?: string | null;
  // Why the discard confirmations can't discard now (`busyReason`: an action in flight or a
  // transfer; a paused merge, rebase or sequence step allows it, #521), else `null`.
  discardBlockedReason?: string | null;
  // The stash list: a selected commit id that is a stash's renders the stash view instead of
  // the commit view.
  stashes?: StashEntry[];
  stashActions?: StashActions;
  // Open-in-editor/file manager/diff tool/merge tool for file sections; omitted, file menus only
  // hold repo actions.
  externalTools?: ExternalToolsApi;
  // "Mark resolved" on a conflicted file's merge tool notice.
  onMarkResolved?: (path: string, force: boolean) => Promise<MarkResolvedOutcome | null>;
  // The conflict pane's "Use ours/theirs (whole file)".
  onResolveWholeFile?: (path: string, side: ConflictSide) => void;
  // Set while the graph shows a file's history: the commit view shows only that file.
  fileHistoryFocus?: FileHistoryFocus | null;
  onOpenFileHistory?: (path: string) => void;
  onOpenLineHistory?: (path: string, revision: string, startLine: number, endLine: number) => void;
  onOpenFolderHistory?: (folder: string) => void;
  repoPath: string;
  client: RepoClient;
  selectedRow: SelectedRow;
  // `useAppState`'s `state.refreshGeneration`: advances on every `refresh()`, so any refresh
  // (not just a hunk action taken in this pane) invalidates every open working-tree diff. Optional
  // only so tests that don't exercise refreshes needn't pass it.
  refreshGeneration?: number;
  // Discard confirmation state, lifted so the command palette can open the same dialogs as the
  // file menu and "Discard all…". Optional: when `onDiscardRequestChange` is absent the pane keeps
  // the state itself.
  discardRequest?: DiscardRequest | null;
  onDiscardRequestChange?: (request: DiscardRequest | null) => void;
  // Reports the current unstaged, discardable file (null when none).
  onCurrentFileChange?: (path: string | null) => void;
  // Passed straight through to `CommitBox` — see its `initialDraft`/`onDraftChange`.
  initialCommitDraft?: CommitDraft;
  onCommitDraftChange?: (draft: CommitDraft) => void;
  // Loaded history, used for the selected commit's header (author, date, parents).
  commits?: GraphCommit[];
  // The graph's own commits (`commits` may be a file history or search results): the commit
  // box's "Recent messages" offers the current branch's recent ones.
  branchCommits?: GraphCommit[];
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageAllFiles: (paths: string[]) => void;
  onUnstageAllFiles: (paths: string[]) => void;
  // May return the mutation's promise (as `useAppState`'s do): the pane waits for it to settle
  // before marking that path's diff stale, so the refetch can't race the mutation itself.
  onStageHunk: (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void | Promise<void>;
  onUnstageHunk: (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void | Promise<void>;
  onDiscardHunk: (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void | Promise<void>;
  onStageLines: (path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
  onUnstageLines: (path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
  onDiscardLines: (path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
  onDiscardFile: (path: string) => void | Promise<void>;
  // The paths the confirm dialog listed; only those are discarded.
  onDiscardAllUnstaged: (paths: string[]) => void;
  onIgnorePath: (pattern: string) => void;
  // Resolves `true` once committed; CommitBox keeps the message otherwise.
  onCommit: (message: string, trailers: Trailer[]) => Promise<boolean>;
  onSaveStash: () => void;
  // "Stash…" (paths empty) and a file's "Stash file…" (that path): open the stash dialog.
  onOpenStashDialog?: (paths: string[]) => void;
  onSelectRow: (row: SelectedRow) => void;
  onResolveConflict: (path: string, resolvedContent: string) => void;
  onResolveAddDeleteConflict: (path: string, choice: FileConflictChoice) => void;
  mergeMessage: string | null;
  onAbortMerge: () => void;
  amendMode?: boolean;
  headInfo?: HeadInfo;
  onSetAmendMode?: (on: boolean) => void;
  onAmendCommit?: (message: string, options: AmendOptions) => Promise<boolean>;
  focusCommitMessage?: boolean;
  onCommitMessageFocused?: () => void;
  // CommitBox's "Skip hooks" toggle and running-hook status (installed hooks come from
  // `headInfo.commitHooks`); omitted, neither shows.
  commitHooks?: CommitHooksControls;
  // One-shot CommitBox clear request (see its `clearMessage`).
  clearCommitMessage?: string | null;
  onCommitMessageCleared?: () => void;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  onRebaseContinue: () => void;
  onRebaseAbort: () => void;
  // Continue/Abort for a stopped cherry-pick/revert sequence (`headInfo.sequence`).
  onSequenceContinue?: () => void;
  onSequenceAbort?: () => void;
  // An action is running: the sequence's Continue/Abort are disabled (no double submit).
  sequencePending?: boolean;
}) {
  // A commit's Changes / All files view (#446), kept while moving between commits.
  const [commitView, setCommitView] = useState<CommitView>("changes");
  if (selectedRow === "uncommitted") {
    return (
      <UncommittedDiffPane
        repoPath={repoPath}
        client={client}
        status={status}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onStageAllFiles={onStageAllFiles}
        onUnstageAllFiles={onUnstageAllFiles}
        onStageHunk={onStageHunk}
        onUnstageHunk={onUnstageHunk}
        onDiscardHunk={onDiscardHunk}
        onStageLines={onStageLines}
        onUnstageLines={onUnstageLines}
        onDiscardLines={onDiscardLines}
        onDiscardFile={onDiscardFile}
        onDiscardAllUnstaged={onDiscardAllUnstaged}
        onIgnorePath={onIgnorePath}
        onCommit={onCommit}
        onSaveStash={onSaveStash}
        onOpenStashDialog={onOpenStashDialog}
        onSelectRow={onSelectRow}
        onResolveConflict={onResolveConflict}
        onResolveAddDeleteConflict={onResolveAddDeleteConflict}
        mergeMessage={mergeMessage}
        onAbortMerge={onAbortMerge}
        amendMode={amendMode}
        headInfo={headInfo}
        onSetAmendMode={onSetAmendMode}
        onAmendCommit={onAmendCommit}
        focusCommitMessage={focusCommitMessage}
        onCommitMessageFocused={onCommitMessageFocused}
        commitHooks={commitHooks}
        clearCommitMessage={clearCommitMessage}
        onCommitMessageCleared={onCommitMessageCleared}
        rebaseProgress={rebaseProgress}
        onRebaseContinue={onRebaseContinue}
        onRebaseAbort={onRebaseAbort}
        onSequenceContinue={onSequenceContinue}
        onSequenceAbort={onSequenceAbort}
        sequencePending={sequencePending}
        refreshGeneration={refreshGeneration}
        initialCommitDraft={initialCommitDraft}
        onCommitDraftChange={onCommitDraftChange}
        discardRequest={discardRequest}
        onDiscardRequestChange={onDiscardRequestChange}
        onCurrentFileChange={onCurrentFileChange}
        onOpenFileHistory={onOpenFileHistory}
        onOpenFolderHistory={onOpenFolderHistory}
        externalTools={externalTools}
        onMarkResolved={onMarkResolved}
        onResolveWholeFile={onResolveWholeFile}
        onDiscardFiles={onDiscardFiles ?? onDiscardAllUnstaged}
        onIgnorePaths={onIgnorePaths ?? ((patterns) => patterns.forEach(onIgnorePath))}
        onSelectionChange={onSelectionChange}
        selectionBusyReason={selectionBusyReason}
        discardBlockedReason={discardBlockedReason}
        commits={commits}
        branchCommits={branchCommits}
        patchActions={patchActions}
      />
    );
  }
  const stash = stashes.find((entry) => entry.commitId === selectedRow.commitId);
  if (stash !== undefined && stashActions !== undefined) {
    return (
      <StashDiffPane
        key={stash.commitId}
        repoPath={repoPath}
        client={client}
        stash={stash}
        actions={stashActions}
        onSelectRow={onSelectRow}
      />
    );
  }
  return (
    <CommitDiffPane
      key={selectedRow.commitId}
      repoPath={repoPath}
      client={client}
      commitId={selectedRow.commitId}
      commits={commits}
      onSelectRow={onSelectRow}
      onOpenFileHistory={onOpenFileHistory}
      onOpenLineHistory={onOpenLineHistory}
      onOpenFolderHistory={onOpenFolderHistory}
      focus={fileHistoryFocus ?? null}
      externalTools={externalTools}
      view={commitView}
      onViewChange={setCommitView}
      onRestoreFileVersion={onRestoreFileVersion}
      restoreBlockedReason={restoreBlockedReason}
      patchActions={patchActions}
    />
  );
}
