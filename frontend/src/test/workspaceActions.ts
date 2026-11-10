import type { WorkspaceActions } from "../state/useWorkspaceActions";

const noop = () => {};
const notCommitted = async () => false;

/**
 * A complete `WorkspaceActions` for a test (#491): every required action does nothing (commit and
 * amend resolve `false`), every optional one is absent, and `overrides` replace either.
 */
export function testWorkspaceActions(overrides: Partial<WorkspaceActions> = {}): WorkspaceActions {
  return {
    onSelectRow: noop,
    onStageFile: noop,
    onUnstageFile: noop,
    onStageAllFiles: noop,
    onUnstageAllFiles: noop,
    onStageHunk: noop,
    onUnstageHunk: noop,
    onDiscardHunk: noop,
    onStageLines: noop,
    onUnstageLines: noop,
    onDiscardLines: noop,
    onDiscardFile: noop,
    onDiscardAllUnstaged: noop,
    onIgnorePath: noop,
    onResolveConflict: noop,
    onResolveAddDeleteConflict: noop,
    onCommit: notCommitted,
    onAmendCommit: notCommitted,
    onSetAmendMode: noop,
    onAbortMerge: noop,
    onSaveStash: noop,
    onOpenStashDialog: noop,
    onRebaseContinue: noop,
    onRebaseAbort: noop,
    onSequenceContinue: noop,
    onSequenceAbort: noop,
    ...overrides,
  };
}

// Every member's name, so `splitWorkspaceActions` can tell actions from data props. A `Record`
// over the interface's keys: adding a member to `WorkspaceActions` fails to compile until it is
// listed here.
const ACTION_KEYS: Record<keyof WorkspaceActions, true> = {
  onSelectRow: true,
  onStageFile: true,
  onUnstageFile: true,
  onStageAllFiles: true,
  onUnstageAllFiles: true,
  onStageHunk: true,
  onUnstageHunk: true,
  onDiscardHunk: true,
  onStageLines: true,
  onUnstageLines: true,
  onDiscardLines: true,
  onDiscardFile: true,
  onDiscardAllUnstaged: true,
  onDiscardFiles: true,
  onDeleteUntracked: true,
  onIgnorePath: true,
  onIgnorePaths: true,
  onResolveConflict: true,
  onResolveAddDeleteConflict: true,
  onMarkResolved: true,
  onResolveWholeFile: true,
  onCommit: true,
  onAmendCommit: true,
  onSetAmendMode: true,
  onAbortMerge: true,
  onSaveStash: true,
  onOpenStashDialog: true,
  onRebaseContinue: true,
  onRebaseAbort: true,
  onSequenceContinue: true,
  onSequenceAbort: true,
  onCommitDraftChange: true,
  onCommitMessageFocused: true,
  onCommitMessageCleared: true,
  commitHooks: true,
  onOpenFileHistory: true,
  onOpenLineHistory: true,
  onOpenFolderHistory: true,
  externalTools: true,
  onRestoreFileVersion: true,
  patchActions: true,
  stashActions: true,
  onCurrentFileChange: true,
  onSelectionChange: true,
};

/** Splits a test's mixed props into the workspace actions and the component's own props. */
export function splitWorkspaceActions<P extends object>(
  props: P & Partial<WorkspaceActions>,
): [Partial<WorkspaceActions>, Omit<P, keyof WorkspaceActions>] {
  const actions: Record<string, unknown> = {};
  const rest: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(props)) {
    if (key in ACTION_KEYS) actions[key] = value;
    else rest[key] = value;
  }
  return [actions as Partial<WorkspaceActions>, rest as Omit<P, keyof WorkspaceActions>];
}
