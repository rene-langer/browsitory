import { createContext, useContext } from "react";
import type {
  AmendOptions,
  ConflictSide,
  FileConflictChoice,
  HunkOptions,
  MarkResolvedOutcome,
  Trailer,
} from "../ipc/RepoClient";
import type { CommitDraft } from "../components/CommitBox";
import type {
  CommitHooksControls,
  DiffPanePatchActions,
  ReportedFileSelection,
  StashActions,
} from "../components/diffPaneShared";
import type { SelectedRow } from "./useAppState";
import type { ExternalToolsApi } from "./useExternalTools";

type HunkAction = (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void | Promise<void>;
type LineAction = (
  path: string,
  oldStart: number,
  newStart: number,
  hunk: HunkOptions,
  lineIndices: number[],
  expectedLines: string[],
) => void | Promise<void>;

/**
 * What the workspace's views can do (#491): `RepoWorkspace` builds this once, from `useAppState`
 * and its own wrappers (toasts, busy re-checks), and provides it through
 * `WorkspaceActionsContext`. The diff pane's views and the commit dock read the actions they
 * need instead of having each one passed down through every level. Data (`status`,
 * `selectedRow`, `headInfo`, …) stays in explicit props.
 *
 * The required members are the ones every view needs to work; an optional one that is absent
 * hides the control it drives (as the old optional props did).
 */
export interface WorkspaceActions {
  onSelectRow: (row: SelectedRow) => void;

  // Staging. May return the mutation's promise (as `useAppState`'s do): the pane waits for it to
  // settle before marking that path's diff stale, so the refetch can't race the mutation itself.
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageAllFiles: (paths: string[]) => void;
  onUnstageAllFiles: (paths: string[]) => void;
  onStageHunk: HunkAction;
  onUnstageHunk: HunkAction;
  onDiscardHunk: HunkAction;
  onStageLines: LineAction;
  onUnstageLines: LineAction;
  onDiscardLines: LineAction;
  onDiscardFile: (path: string) => void | Promise<void>;
  // The paths the confirm dialog listed; only those are discarded.
  onDiscardAllUnstaged: (paths: string[]) => void;
  // A multi-file selection's discard (#449): exactly the confirmed paths, one undo step.
  // Defaults to `onDiscardAllUnstaged`.
  onDiscardFiles?: (paths: string[]) => void;
  // "Delete untracked…" (#622): exactly the confirmed untracked paths. Defaults to
  // `onDiscardFiles`.
  onDeleteUntracked?: (paths: string[]) => void;
  onIgnorePath: (pattern: string) => void;
  // A multi-file selection's "Ignore": one `/path` pattern per untracked file. Defaults to one
  // `onIgnorePath` per pattern.
  onIgnorePaths?: (patterns: string[]) => void;

  // Conflicts.
  onResolveConflict: (path: string, resolvedContent: string) => void;
  onResolveAddDeleteConflict: (path: string, choice: FileConflictChoice) => void;
  // "Mark resolved" on a conflicted file's merge tool notice.
  onMarkResolved?: (path: string, force: boolean) => Promise<MarkResolvedOutcome | null>;
  // The conflict pane's "Use ours/theirs (whole file)".
  onResolveWholeFile?: (path: string, side: ConflictSide) => void;

  // The commit dock. `onCommit`/`onAmendCommit` resolve `true` once committed/amended;
  // CommitBox keeps the message otherwise.
  onCommit: (message: string, trailers: Trailer[]) => Promise<boolean>;
  onAmendCommit: (message: string, options: AmendOptions) => Promise<boolean>;
  onSetAmendMode: (on: boolean) => void;
  // Where a commit lands and "Commit and push" (#651): `branch` names the current branch in the
  // commit button (null on a detached HEAD); `onCommitAndPush` resolves like `onCommit`, and
  // `commitAndPushReason` says why it can't run (null when it can).
  commitFlow?: {
    branch: string | null;
    onCommitAndPush: (message: string, trailers: Trailer[]) => Promise<boolean>;
    commitAndPushReason: string | null;
    // A commit or amend refused for lack of a git identity (#689): the dialog that sets it, then
    // commits again (`retrying`; an amend is retried by hand).
    identity: {
      required: boolean;
      retrying: boolean;
      onSave: (name: string, email: string, global: boolean) => Promise<string | null>;
      onCancel: () => void;
    };
  };
  onAbortMerge: () => void;
  onSaveStash: () => void;
  // "Stash…" (paths empty) and a file's "Stash file…" (that path): open the stash dialog.
  onOpenStashDialog: (paths: string[]) => void;
  onRebaseContinue: () => void;
  /** The commit dock's edited message at an edit stop, `null` when unchanged (#882). */
  onRebaseEditMessage?: (message: string | null) => void;
  onRebaseAbort: () => void;
  // Continue/Abort for a stopped cherry-pick/revert sequence (`headInfo.sequence`).
  onSequenceContinue: () => void;
  onSequenceAbort: () => void;
  // Passed straight through to `CommitBox` — see its `onDraftChange`.
  onCommitDraftChange?: (draft: CommitDraft) => void;
  onCommitMessageFocused?: () => void;
  onCommitMessageCleared?: () => void;
  // CommitBox's "Skip hooks" toggle and running-hook status (installed hooks come from
  // `headInfo.commitHooks`); omitted, neither shows.
  commitHooks?: CommitHooksControls;

  // History and external tools. Omitted, their buttons and menu items don't show.
  onOpenFileHistory?: (path: string) => void;
  onOpenLineHistory?: (path: string, revision: string, startLine: number, endLine: number) => void;
  onOpenFolderHistory?: (folder: string) => void;
  externalTools?: ExternalToolsApi;
  // "All files" view of a commit (#446): Restore… writes a file's version to the work tree.
  onRestoreFileVersion?: (commitId: string, path: string) => void | Promise<void>;
  // Revert / apply a commit's file or hunk, "Save as patch…" (#448).
  patchActions?: DiffPanePatchActions;
  // The stash view's actions; omitted, a selected stash shows as a plain commit.
  stashActions?: StashActions;

  // Reports to the command palette: the current unstaged, discardable file (null when none), and
  // the working-tree selection while it holds two or more files (else null).
  onCurrentFileChange?: (path: string | null) => void;
  onSelectionChange?: (selection: ReportedFileSelection | null) => void;
}

export const WorkspaceActionsContext = createContext<WorkspaceActions | null>(null);

/** The workspace's actions; only valid below `RepoWorkspace` (or a test's provider). */
export function useWorkspaceActions(): WorkspaceActions {
  const actions = useContext(WorkspaceActionsContext);
  if (actions === null) throw new Error("useWorkspaceActions needs a WorkspaceActionsContext provider");
  return actions;
}
