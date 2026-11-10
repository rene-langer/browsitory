// Types and helpers shared by `DiffPane` and its views (`UncommittedDiffPane`, `CommitDiffPane`,
// `StashDiffPane`, `UncommittedFileSection`).
import { type ReactNode } from "react";
import type { CommitChangeRequest, PatchSource, HeadInfo } from "../ipc/RepoClient";
import formStyles from "./primitives/FormDialog.module.css";
import { type StashBlockers } from "../lib/actions/stashActions";

/** The hook controls `DiffPane` hands to `CommitBox` (whether hooks are installed is derived). */
export interface CommitHooksControls {
  skip: boolean;
  onToggleSkip: (on: boolean) => void;
  running: { operationId: string; hook: string } | null;
  onCancel: () => void;
}

export const NO_HEAD_INFO: HeadInfo = { headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null };
export const noop = () => {};
export const noAmend = async () => false;

/** What a commit's view lists: the files it changed, or its whole tree (#446). */
export type CommitView = "changes" | "tree";
/** What `DiffPane` needs for #448's revert/apply and "Save as patch…". */
export interface DiffPanePatchActions {
  /** Why reverting or applying can't run now (`operationBlockers`), else `null`. */
  blockedReason: string | null;
  onApplyCommitChange: (change: CommitChangeRequest) => void;
  onSavePatch: (source: PatchSource, suggestedName: string) => void;
}

/** The stash view's actions; Apply/Pop use `restoreIndex`. */
export interface StashActions {
  onApply: (index: number) => void;
  onPop: (index: number) => void;
  onDrop: (index: number) => void;
  restoreIndex: boolean;
  onSetRestoreIndex: (on: boolean) => void;
  // Why Apply/Pop and Drop can't run now (`stashBlockers`, shared with the stash list and the
  // palette, #509). A paused merge, rebase or sequence step allows Drop (#521).
  blockers: StashBlockers;
}

/**
 * A confirmation's message plus, while the action is blocked, why (#521): the repository can
 * become busy while the confirmation is open.
 */
export function withBlockedReason(message: ReactNode, blockedReason: string | null): ReactNode {
  if (blockedReason === null) return message;
  return (
    <>
      {typeof message === "string" ? <p>{message}</p> : message}
      <p className={formStyles.blocked}>{blockedReason}</p>
    </>
  );
}

/** What the discard confirm dialogs are currently asking about; null when none is open. */
export type DiscardRequest = { kind: "file"; path: string } | { kind: "all" } | { kind: "files"; paths: string[] };

/** The multi-file selection DiffPane reports to the command palette (two or more files). */
export interface ReportedFileSelection {
  staged: boolean;
  paths: string[];
}

export type SelectionActionKind = "stageSelection" | "unstageSelection" | "discardSelection" | "ignoreSelection" | "stashSelection";

/** The file whose history the graph is showing, at the selected commit. */
export type FileHistoryFocus = { path: string; oldPath: string | null };
