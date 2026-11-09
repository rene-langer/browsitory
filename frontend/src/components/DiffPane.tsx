import { describeError, type DescribedError } from "../lib/errorMessages";
import { conflictReason, rebaseInterruption, sequenceNoun, stashBlockReason } from "../lib/operationStatus";
import {
  AlertTriangle,
  ArrowRightLeft,
  FileDiff,
  FileMinus,
  FilePlus,
  Minus,
  Pencil,
  Plus,
  MoreHorizontal,
  type LucideIcon,
} from "lucide-react";
import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import type {
  AmendOptions,
  BlameLine,
  CommitChangeRequest,
  CommitHunk,
  PatchSource,
  DiffHunk,
  DiffSource,
  DiffOptions,
  FileConflictChoice,
  GraphCommit,
  HeadInfo,
  MarkResolvedOutcome,
  ConflictSide,
  RepoClient,
  StashEntry,
  StashFiles,
  StatusEntry,
  StatusKind,
  Trailer,
} from "../ipc/RepoClient";
import { messageHistoryEntries, readCommitMessages, recentBranchCommits } from "../lib/commitMessageHistory";
import { useCommitMessageSettingsContext } from "../state/useCommitMessageSettings";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useWholeFile } from "../state/useWholeFile";
import type { SelectedRow } from "../state/useAppState";
import type { ExternalToolsApi } from "../state/useExternalTools";
import { BlameView } from "./BlameView";
import { CommitBox, type CommitDraft } from "./CommitBox";
import { SplitButton } from "./primitives/SplitButton";
import { Toolbar } from "./primitives/Toolbar";
import { CollapseAllToggle, CollapseToggle } from "./CollapseToggle";
import { CommitHeader } from "./CommitHeader";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import { DiffView } from "./DiffView";
import styles from "./DiffPane.module.css";
import { ImageDiff, ImageTextToggle } from "./ImageDiff";
import { imageMimeType, isSvgPath } from "../lib/imageTypes";
import {
  discardAllConfirmMessage,
  discardConfirmMessage,
  fileMenuItems,
  fileToolItems,
  isUntracked,
  selectionMenuItems,
  type FileAction,
  type SelectionSummary,
} from "./FileActionsMenu";
import {
  EMPTY_FILE_SELECTION,
  extendFileSelection,
  selectAllFiles,
  selectOnlyFile,
  selectedPaths,
  toggleFile,
  type FileSelection,
} from "../lib/fileSelection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu } from "./primitives/ContextMenu";
import { isMenuKey, menuAnchor, rectAnchor } from "../lib/menuKeys";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { ReadOnlyFileSection, type CommitFilePatchActions } from "./ReadOnlyFileSection";
import { CommitTreeBrowser } from "./CommitTreeBrowser";
import treeStyles from "./CommitTreeBrowser.module.css";
import { FolderHistoryButton } from "./FolderHistoryButton";
import { RebaseProgressPanel } from "./RebaseProgressPanel";

const STATUS_ICONS: Record<StatusKind, LucideIcon> = {
  New: FilePlus,
  Modified: Pencil,
  Deleted: FileMinus,
  Renamed: ArrowRightLeft,
  TypeChange: FileDiff,
  Conflicted: AlertTriangle,
};

function UncommittedFileSection({
  repoPath,
  client,
  entry,
  status,
  isCurrent,
  selected,
  rowId,
  collapsed,
  diffVersion,
  onToggleCollapse,
  onSelect,
  selectionSummary,
  onSelectionAction,
  selectionBusyReason,
  onStageFile,
  onUnstageFile,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onStageLines,
  onUnstageLines,
  onDiscardLines,
  onSelectRow,
  onResolveConflict,
  onResolveAddDeleteConflict,
  onRequestDiscardFile,
  onIgnorePath,
  onStashFile,
  stashDisabledReason,
  onOpenFileHistory,
  onOpenFolderHistory,
  externalTools,
  onMarkResolved,
  onResolveWholeFile,
  sectionRef,
}: {
  repoPath: string;
  client: RepoClient;
  entry: StatusEntry;
  status: StatusEntry[];
  /** The focused file: its diff is scrolled into view, and single-file keys act on it. */
  isCurrent: boolean;
  /** Part of the selection (`aria-selected`); the current file always is. */
  selected: boolean;
  rowId: string;
  collapsed: boolean;
  // This section's staleness signal: its own per-path version plus the pane-wide refresh
  // generation, combined by the parent (`UncommittedDiffPane`) into one monotonically increasing
  // number — see `staleness` there for how each part is bumped. Any change means "this diff may
  // be stale, refetch it".
  diffVersion: number;
  onToggleCollapse: () => void;
  onSelect: (event?: MouseEvent) => void;
  /** Set while this row is part of a selection of two or more files: its menu acts on all of them. */
  selectionSummary: SelectionSummary | null;
  onSelectionAction: (kind: SelectionActionKind) => void;
  /** Why the selection's repository actions are disabled (another action is running), else `null`. */
  selectionBusyReason: string | null;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void;
  onUnstageHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void;
  onDiscardHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void;
  onStageLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void;
  onUnstageLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void;
  onDiscardLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void;
  onSelectRow: (row: SelectedRow) => void;
  onResolveConflict: (path: string, resolvedContent: string) => void;
  onResolveAddDeleteConflict: (path: string, choice: FileConflictChoice) => void;
  onRequestDiscardFile: (path: string) => void;
  onIgnorePath: (pattern: string) => void;
  onStashFile: (path: string) => void;
  /** `stashBlockReason`: disables "Stash file…" like the Stash button. */
  stashDisabledReason: string | null;
  onOpenFileHistory?: (path: string) => void;
  onOpenFolderHistory?: (folder: string) => void;
  externalTools?: ExternalToolsApi;
  onMarkResolved?: (path: string, force: boolean) => Promise<MarkResolvedOutcome | null>;
  // The conflict pane's "Use ours/theirs (whole file)".
  onResolveWholeFile?: (path: string, side: ConflictSide) => void;
  sectionRef: (el: HTMLDivElement | null) => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo?: HTMLElement } | null>(null);
  const actionsRef = useRef<HTMLButtonElement>(null);
  const [confirmMarkers, setConfirmMarkers] = useState(false);
  const [mode, setMode] = useState<"diff" | "blame">("diff");
  const [hunks, setHunks] = useState<DiffHunk[] | null>(null);
  // A staged rename compares its old path in HEAD with its new path in the index (#137).
  const renamedFrom = entry.staged ? (entry.oldPath ?? null) : null;
  const diffSource: DiffSource = entry.staged ? { kind: "Staged", oldPath: renamedFrom } : { kind: "Unstaged" };
  const onPickFileAction = (action: FileAction) => {
    switch (action.kind) {
      case "discard":
        return onRequestDiscardFile(entry.path);
      case "stash":
        return onStashFile(entry.path);
      case "ignore":
        return onIgnorePath(action.pattern);
      case "openInEditor": {
        // A hunk that empties the file starts at line 0, which no editor accepts.
        const line = hunks?.[0]?.newStart ?? 0;
        return externalTools?.openInEditor(entry.path, line > 0 ? line : null);
      }
      case "reveal":
        return externalTools?.reveal(entry.path);
      case "diffTool":
        return externalTools?.openDiffTool(entry.path, diffSource);
      case "mergeTool":
        return externalTools?.openMergeTool(entry.path);
      case "stageSelection":
      case "unstageSelection":
      case "discardSelection":
      case "ignoreSelection":
      case "stashSelection":
        return onSelectionAction(action.kind);
    }
  };
  // Without external tools, conflicted entries have no actions and staged ones only "Stash file…"
  // (see `fileMenuItems`).
  const tools =
    externalTools === undefined ? undefined : { mergeToolRunning: externalTools.mergeToolRunning.has(entry.path) };
  // A row in a multi-file selection offers the selection's actions (like the commit graph's
  // multi-select menu), plus this file's own external-tool items.
  const menuItems =
    selectionSummary !== null
      ? [
          ...selectionMenuItems(selectionSummary, onPickFileAction, selectionBusyReason, stashDisabledReason),
          ...(tools === undefined ? [] : fileToolItems(entry, onPickFileAction, tools)),
        ]
      : fileMenuItems(entry, onPickFileAction, tools, stashDisabledReason);
  const hasFileActions = menuItems.length > 0;
  const markResolved = async (force: boolean) => {
    if (onMarkResolved === undefined) return;
    const outcome = await onMarkResolved(entry.path, force);
    if (outcome === "MarkersRemain") setConfirmMarkers(true);
    else if (outcome === "Resolved") externalTools?.dismissMergeToolResult(entry.path);
  };
  const mergeToolResult = externalTools?.mergeToolResults.get(entry.path);
  // The options `hunks` were fetched with. Hunk/line actions must use these, not the current
  // settings: after a settings change the shown hunks are still the old ones until the refetch
  // lands, and their (oldStart, newStart) only mean something under the options they came from.
  const [fetchedOptions, setFetchedOptions] = useState<DiffOptions | null>(null);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);
  const isConflicted = entry.kind === "Conflicted";
  // Image files show a before/after preview instead of hunks; an SVG (text) can switch to its
  // text diff. Conflicted images still go to the conflict pane.
  const isImage = imageMimeType(entry.path) !== null;
  const [imageAsText, setImageAsText] = useState(false);
  const showImage = isImage && !isConflicted && !imageAsText;
  // Tracks proximity to the viewport so a long file list doesn't eagerly fetch every section's
  // diff at once (PERF-001) — only sections within `rootMargin` of the viewport ever load.
  const sectionElRef = useRef<HTMLDivElement | null>(null);
  const [nearViewport, setNearViewport] = useState(false);
  useEffect(() => {
    const el = sectionElRef.current;
    if (el === null) return;
    const observer = new IntersectionObserver(([observerEntry]) => setNearViewport(observerEntry.isIntersecting), {
      rootMargin: "200px",
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Every file's diff is fetched keyed on the file's own identity rather than a shared
  // "selected" pointer. Whole-file staging/unstaging moves this file to a differently-keyed
  // section and remounts it fresh, but *partial* (hunk) staging leaves it at the same
  // path/staged key while only its hunk content changes underneath — and so does anything else
  // that changes the working tree or index without moving the file between groups (a stash
  // apply, a pull, the palette's Refresh after an external edit). `diffVersion` is the signal for
  // all of those: the parent bumps it for both sides of a hunk-mutated path once the mutation has
  // settled, and folds in a refresh generation that advances on every `appState.refresh()`. It
  // replaced a direct `status` dependency only so that a status array that is new by reference
  // but not the product of a refresh (a re-render, an optimistic update) doesn't refetch anything.
  //
  // `loadDiff` is pulled out of the effect (returning its own cleanup) so the failure banner's
  // Retry button can re-run the exact same fetch on demand, not just on a dependency change.
  const { settings } = useDiffSettingsContext();
  // "Show whole file" fetches with whole-file context; hunk and line actions then pass it too.
  const wholeFile = useWholeFile(settings.contextLines);
  const diffOptions = { ignoreWhitespace: settings.ignoreWhitespace, contextLines: wholeFile.contextLines };
  const loadDiff = () => {
    let ignore = false;
    const requested = diffOptions;
    (renamedFrom === null
      ? client.getWorkingDiff(repoPath, entry.path, entry.staged, requested)
      : client.getWorkingDiff(repoPath, entry.path, entry.staged, requested, renamedFrom)
    )
      .then((next) => {
        if (!ignore && wholeFile.accept(next, requested.contextLines)) {
          setHunks(next);
          setFetchedOptions(requested);
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          setError(describeError(err));
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    // Lazy: a collapsed section fetches nothing until it is expanded, and a section far from the
    // viewport fetches nothing until it is scrolled near (both PERF-001). `diffVersion` (not
    // `status`) is the staleness signal — see the comment on `loadDiff` above.
    if (mode !== "diff" || isConflicted || showImage || collapsed || !nearViewport) return;
    return loadDiff();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, entry.path, entry.staged, renamedFrom, isConflicted, showImage, mode, diffVersion, collapsed, nearViewport, settings.ignoreWhitespace, wholeFile.contextLines]);

  // `status` is a dependency for the same reason as the diff effect above: staging or committing
  // the file on screen while its blame view is open must not leave stale pre-commit attribution
  // on screen.
  const loadBlame = () => {
    let ignore = false;
    client
      .getBlame(repoPath, "HEAD", entry.path)
      .then((next) => {
        if (!ignore) {
          setBlameLines(next);
          setError(null);
        }
      })
      .catch(() => {
        if (!ignore) {
          setError({ message: "No blame available for this file at this revision." });
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    if (mode !== "blame") return;
    return loadBlame();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, entry.path, mode, status]);

  const Icon = STATUS_ICONS[entry.kind];
  // While a settings change is refetching, the shown hunks belong to the old options: offer no
  // hunk or line action until the matching hunks arrive.
  const optionsCurrent =
    fetchedOptions !== null &&
    fetchedOptions.ignoreWhitespace === settings.ignoreWhitespace &&
    fetchedOptions.contextLines === wholeFile.contextLines;
  const fetchedContext = fetchedOptions?.contextLines ?? wholeFile.contextLines;
  const unstagedActions = optionsCurrent && !entry.staged;
  const stagedActions = optionsCurrent && entry.staged;

  return (
    <ListRow
      id={rowId}
      selected={selected}
      onClick={onSelect}
      className={[styles.fileSection, isConflicted ? styles.conflicted : null, isCurrent ? styles.current : null]
        .filter((name) => name !== null)
        .join(" ")}
    >
      <div
        className={styles.fileSectionHeader}
        onContextMenu={
          hasFileActions
            ? (event) => {
                event.preventDefault();
                // The header isn't focusable: closing from the keyboard or an item lands on "…".
                setMenu({ ...menuAnchor(event, event.currentTarget), returnFocusTo: actionsRef.current ?? undefined });
              }
            : undefined
        }
        ref={(el) => {
          sectionElRef.current = el;
        }}
      >
        <CollapseToggle collapsed={collapsed} path={entry.path} onToggle={onToggleCollapse} />
        <Icon size={14} className={styles.statusIcon} aria-hidden="true" />
        <span className={styles.path}>
          {entry.path} ({entry.kind})
        </span>
        {renamedFrom !== null && <span className={styles.renamedFrom}>Renamed from {renamedFrom}</span>}
        <div className={styles.rowActions}>
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              setMode("blame");
            }}
          >
            Blame
          </button>
          {onOpenFileHistory !== undefined && !isUntracked(entry) && (
            <button
              type="button"
              aria-label={`History of ${entry.path}`}
              onClick={(event) => {
                event.stopPropagation();
                onOpenFileHistory(entry.path);
              }}
            >
              History
            </button>
          )}
          {onOpenFolderHistory !== undefined && (
            <FolderHistoryButton path={entry.path} onOpen={onOpenFolderHistory} stopPropagation />
          )}
          {hasFileActions && (
            <button
              type="button"
              className={styles.stageToggle}
              ref={actionsRef}
              aria-label={`Actions for ${entry.path}`}
              aria-haspopup="menu"
              aria-expanded={menu !== null}
              onClick={(event) => {
                event.stopPropagation();
                // A pointer click (`detail > 0`) need not focus the button (WebKit), so name it as
                // where focus returns. A key, or DiffPane's Menu-key `click()`, leaves focus where it is.
                const button = event.currentTarget;
                if (menu !== null) {
                  // A second click closes the open menu. Focus stays on the button, as when the
                  // menu closes from a key (WebKit may not have focused it on the click).
                  button.focus({ preventScroll: true });
                  setMenu(null);
                  return;
                }
                setMenu({ ...rectAnchor(button), returnFocusTo: event.detail > 0 ? button : undefined });
              }}
            >
              <MoreHorizontal size={14} aria-hidden="true" />
            </button>
          )}
          {entry.staged ? (
            <button
              type="button"
              className={styles.stageToggle}
              aria-label={`Unstage ${entry.path}`}
              onClick={(event) => {
                event.stopPropagation();
                onUnstageFile(entry.path);
              }}
            >
              <Minus size={14} aria-hidden="true" />
            </button>
          ) : (
            <button
              type="button"
              className={styles.stageToggle}
              aria-label={`Stage ${entry.path}`}
              onClick={(event) => {
                event.stopPropagation();
                onStageFile(entry.path);
              }}
            >
              <Plus size={14} aria-hidden="true" />
            </button>
          )}
        </div>
      </div>
      {isConflicted && externalTools !== undefined && mergeToolResult !== undefined && (
        // A persistent notice, not a toast: the user is usually still in the tool's window when
        // a toast would have closed.
        <div role="status" className={styles.mergeToolNotice}>
          <span>
            {mergeToolResult === "Resolved"
              ? "The merge tool finished."
              : "The merge tool closed without resolving the conflict."}
          </span>
          {/* Offered either way: the tool may have saved a resolution it didn't report, and the
              conflict-markers check still asks before staging a file that has them. */}
          {onMarkResolved !== undefined && (
            <button
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                void markResolved(false);
              }}
            >
              Mark resolved
            </button>
          )}
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              externalTools.dismissMergeToolResult(entry.path);
            }}
          >
            Dismiss
          </button>
        </div>
      )}
      {!collapsed && (
        <div className={styles.fileSectionBody} data-file-body="" ref={sectionRef}>
          {mode === "blame" ? (
            <>
              {error !== null ? (
                <InlineError
                  message={error.message}
                  hint={error.hint}
                  onDismiss={() => setError(null)}
                  onRetry={loadBlame}
                />
              ) : (
                <BlameView lines={blameLines} onSelectRow={onSelectRow} />
              )}
              <button type="button" onClick={() => setMode("diff")}>
                Back to diff
              </button>
            </>
          ) : isConflicted ? (
            <ConflictResolutionPane
              repoPath={repoPath}
              client={client}
              path={entry.path}
              onResolve={onResolveConflict}
              onResolveAddDelete={onResolveAddDeleteConflict}
              onResolveWholeFile={onResolveWholeFile}
              onMarkResolved={onMarkResolved === undefined ? undefined : () => void markResolved(false)}
            />
          ) : showImage ? (
            <>
              {isSvgPath(entry.path) && (
                <ImageTextToggle asText={false} onToggle={() => setImageAsText(true)} />
              )}
              {nearViewport && (
                <ImageDiff
                  client={client}
                  repoPath={repoPath}
                  path={entry.path}
                  source={diffSource}
                  refreshKey={diffVersion}
                />
              )}
            </>
          ) : error !== null ? (
            <InlineError
              message={error.message}
              hint={error.hint}
              onDismiss={() => setError(null)}
              onRetry={loadDiff}
            />
          ) : (
            <>
            {isImage && !isConflicted && (
              <ImageTextToggle asText onToggle={() => setImageAsText(false)} />
            )}
            <DiffView
              hunks={hunks}
              path={entry.path}
              onStageHunk={unstagedActions ? (oldStart, newStart) => onStageHunk(entry.path, oldStart, newStart, fetchedContext) : undefined}
              onUnstageHunk={stagedActions ? (oldStart, newStart) => onUnstageHunk(entry.path, oldStart, newStart, fetchedContext) : undefined}
              onDiscardHunk={unstagedActions ? (oldStart, newStart) => onDiscardHunk(entry.path, oldStart, newStart, fetchedContext) : undefined}
              onStageLines={unstagedActions ? (o, n, idx, shown) => onStageLines(entry.path, o, n, fetchedContext, idx, shown) : undefined}
              onUnstageLines={stagedActions ? (o, n, idx, shown) => onUnstageLines(entry.path, o, n, fetchedContext, idx, shown) : undefined}
              onDiscardLines={unstagedActions ? (o, n, idx, shown) => onDiscardLines(entry.path, o, n, fetchedContext, idx, shown) : undefined}
              partialActionsDisabled={fetchedOptions?.ignoreWhitespace ?? settings.ignoreWhitespace}
              wholeFile={wholeFile}
            />
            </>
          )}
        </div>
      )}
      {menu !== null && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          // Opened by the list's Menu key (`click()` on "…" with focus left on the row), "…" is
          // neither `returnFocusTo` nor the focused element, but still this menu's own button.
          isTrigger={(button) => button === actionsRef.current}
          items={menuItems}
          onClose={() => setMenu(null)}
        />
      )}
      {confirmMarkers && (
        <ConfirmDialog
          ariaLabel="Mark resolved"
          message={`Conflict markers remain in ${entry.path}. Mark it resolved anyway?`}
          confirmLabel="Mark resolved"
          onConfirm={() => {
            setConfirmMarkers(false);
            void markResolved(true);
          }}
          onCancel={() => setConfirmMarkers(false)}
        />
      )}
    </ListRow>
  );
}

/** The hook controls `DiffPane` hands to `CommitBox` (whether hooks are installed is derived). */
export interface CommitHooksControls {
  skip: boolean;
  onToggleSkip: (on: boolean) => void;
  running: { operationId: string; hook: string } | null;
  onCancel: () => void;
}

const NO_HEAD_INFO: HeadInfo = { headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null };
const noop = () => {};
const noAmend = async () => false;

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
  onStageHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void | Promise<void>;
  onUnstageHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void | Promise<void>;
  onDiscardHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void | Promise<void>;
  onStageLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
  onUnstageLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
  onDiscardLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
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

/** What a commit's view lists: the files it changed, or its whole tree (#446). */
export type CommitView = "changes" | "tree";
/** What `DiffPane` needs for #448's revert/apply and "Save as patch…". */
export interface DiffPanePatchActions {
  /** Why reverting or applying can't run now (`operationBlockers`), else `null`. */
  blockedReason: string | null;
  onApplyCommitChange: (change: CommitChangeRequest) => void;
  onSavePatch: (source: PatchSource, suggestedName: string) => void;
}

/** A commit's file's `CommitFilePatchActions`, from `DiffPane`'s. */
function commitFilePatch(
  actions: DiffPanePatchActions,
  commitId: string,
  path: string,
  oldPath: string | null,
): CommitFilePatchActions {
  const change = (reverse: boolean, hunk: CommitHunk | null) =>
    actions.onApplyCommitChange({ commitId, path, oldPath, hunk, reverse, stage: false });
  const name = path.split("/").pop() ?? path;
  return {
    blockedReason: actions.blockedReason,
    onRevertFile: () => change(true, null),
    onApplyFile: () => change(false, null),
    onSavePatch: () =>
      actions.onSavePatch({ kind: "CommitFile", commitId, path, oldPath }, `${commitId.slice(0, 7)}-${name}.patch`),
    onRevertHunk: (hunk) => change(true, hunk),
    onApplyHunk: (hunk) => change(false, hunk),
  };
}


/** The stash view's actions; Apply/Pop use `restoreIndex`. */
export interface StashActions {
  onApply: (index: number) => void;
  onPop: (index: number) => void;
  onDrop: (index: number) => void;
  restoreIndex: boolean;
  onSetRestoreIndex: (on: boolean) => void;
  // Why Apply/Pop can't run now (staged changes, conflicts, a running operation), else `null`.
  disabledReason: string | null;
}

/** What the discard confirm dialogs are currently asking about; null when none is open. */
export type DiscardRequest = { kind: "file"; path: string } | { kind: "all" } | { kind: "files"; paths: string[] };

/** The multi-file selection DiffPane reports to the command palette (two or more files). */
export interface ReportedFileSelection {
  staged: boolean;
  paths: string[];
}

type SelectionActionKind = "stageSelection" | "unstageSelection" | "discardSelection" | "ignoreSelection" | "stashSelection";

/** The file whose history the graph is showing, at the selected commit. */
export type FileHistoryFocus = { path: string; oldPath: string | null };

// The change lists' keys (#449): arrows/j/k move, Shift extends, Ctrl/Cmd+A selects the group,
// Escape keeps only the current file, `s` stages or unstages, the Menu key opens the menu.
const FILE_LIST_SHORTCUTS =
  "ArrowUp ArrowDown Shift+ArrowUp Shift+ArrowDown Control+A Meta+A Escape s ContextMenu Shift+F10";

function entryKey(entry: { path: string; staged: boolean }): string {
  return `${entry.staged}:${entry.path}`;
}

function UncommittedDiffPane({
  repoPath,
  client,
  commits,
  branchCommits,
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
  onOpenStashDialog,
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
  refreshGeneration,
  discardRequest: discardRequestProp,
  onDiscardRequestChange,
  onCurrentFileChange,
  initialCommitDraft,
  onCommitDraftChange,
  onOpenFileHistory,
  onOpenFolderHistory,
  externalTools,
  onMarkResolved,
  onResolveWholeFile,
  onDiscardFiles,
  onIgnorePaths,
  onSelectionChange,
  selectionBusyReason,
  patchActions,
}: {
  patchActions?: DiffPanePatchActions;
  onDiscardFiles: (paths: string[]) => void;
  onIgnorePaths: (patterns: string[]) => void;
  onSelectionChange?: (selection: ReportedFileSelection | null) => void;
  selectionBusyReason: string | null;
  onOpenFileHistory?: (path: string) => void;
  onOpenFolderHistory?: (folder: string) => void;
  externalTools?: ExternalToolsApi;
  onMarkResolved?: (path: string, force: boolean) => Promise<MarkResolvedOutcome | null>;
  // The conflict pane's "Use ours/theirs (whole file)".
  onResolveWholeFile?: (path: string, side: ConflictSide) => void;
  repoPath: string;
  client: RepoClient;
  status: StatusEntry[];
  refreshGeneration: number;
  discardRequest?: DiscardRequest | null;
  onDiscardRequestChange?: (request: DiscardRequest | null) => void;
  onCurrentFileChange?: (path: string | null) => void;
  initialCommitDraft?: CommitDraft;
  onCommitDraftChange?: (draft: CommitDraft) => void;
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageAllFiles: (paths: string[]) => void;
  onUnstageAllFiles: (paths: string[]) => void;
  // May return the mutation's promise (as `useAppState`'s do): the pane waits for it to settle
  // before marking that path's diff stale, so the refetch can't race the mutation itself.
  onStageHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void | Promise<void>;
  onUnstageHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void | Promise<void>;
  onDiscardHunk: (path: string, oldStart: number, newStart: number, contextLines: number) => void | Promise<void>;
  onStageLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
  onUnstageLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
  onDiscardLines: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void | Promise<void>;
  onDiscardFile: (path: string) => void | Promise<void>;
  // The paths the confirm dialog listed; only those are discarded.
  onDiscardAllUnstaged: (paths: string[]) => void;
  onIgnorePath: (pattern: string) => void;
  // Resolves `true` once committed; CommitBox keeps the message otherwise.
  /** The loaded history (may be a file history or search results); stands in for
   * `branchCommits` when that is absent. */
  commits?: GraphCommit[];
  /** The graph's commits (not a file history or search), for the branch's recent messages and
   * HEAD's author in the amend Author field. */
  branchCommits?: GraphCommit[];
  onCommit: (message: string, trailers: Trailer[]) => Promise<boolean>;
  onSaveStash: () => void;
  onOpenStashDialog: (paths: string[]) => void;
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
  const { settings: commitMessageSettings } = useCommitMessageSettingsContext();
  const loadRecentAuthors = useCallback(() => client.listRecentAuthors(repoPath), [client, repoPath]);
  const loadMessageHistory = useCallback(
    () =>
      messageHistoryEntries(
        readCommitMessages(repoPath),
        recentBranchCommits(branchCommits ?? commits ?? [], headInfo.headId),
        (commitId) => client.getCommitMessage(repoPath, commitId),
      ),
    [client, repoPath, branchCommits, commits, headInfo.headId],
  );
  // HEAD's author comes from the graph's commits: in file-history or search mode `commits` holds
  // those results, which may not include HEAD (#464).
  const headCommit =
    headInfo.headId === null ? undefined : (branchCommits ?? commits)?.find((c) => c.id === headInfo.headId);
  const headAuthor = headCommit === undefined ? null : { name: headCommit.authorName, email: headCommit.authorEmail };
  const [headMessage, setHeadMessage] = useState<string | null>(null);
  useEffect(() => {
    if (!amendMode || headInfo.headId === null) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setHeadMessage(null);
      return;
    }
    let cancelled = false;
    void client.getCommitMessage(repoPath, headInfo.headId).then(
      (text) => !cancelled && setHeadMessage(text),
      () => !cancelled && setHeadMessage(""),
    );
    return () => {
      cancelled = true;
    };
  }, [amendMode, headInfo.headId, client, repoPath]);
  const [current, setCurrent] = useState<{ path: string; staged: boolean } | null>(null);
  // The files selected for a multi-file action (#449), within one group. The current file is
  // always part of it; a plain click or arrow key makes it the only one.
  const [selection, setSelection] = useState<FileSelection>(EMPTY_FILE_SELECTION);
  const rowIdPrefix = useId();
  const [collapsedKeys, setCollapsedKeys] = useState<Set<string>>(new Set());
  const sectionRefs = useRef<Record<string, HTMLDivElement | null>>({});
  // Per-`entryKey` version counters: the per-path half of each section's staleness signal (the
  // other half is `refreshGeneration`, see `staleness` below). A hunk stage/unstage/discard bumps
  // *both* sides of the affected path — staging a hunk of a partially-staged file changes its
  // staged diff as much as its unstaged one, and either section may be open — and only once the
  // mutation's promise has settled, so the refetch can't observe the index before the backend
  // has applied the change.
  const [diffVersion, setDiffVersion] = useState<Record<string, number>>({});
  const bumpPathVersions = (path: string) => {
    const keys = [entryKey({ path, staged: false }), entryKey({ path, staged: true })];
    setDiffVersion((prev) => {
      const next = { ...prev };
      for (const key of keys) next[key] = (prev[key] ?? 0) + 1;
      return next;
    });
  };
  const wrapHunkAction =
    (action: (path: string, oldStart: number, newStart: number, contextLines: number) => void | Promise<void>) =>
    (path: string, oldStart: number, newStart: number, contextLines: number) => {
      // `useAppState`'s hunk actions never reject (`runMutation` records failures in
      // `state.error`), but `finally` keeps a rejecting caller from skipping the bump too — a
      // failed mutation may still have partially applied.
      void Promise.resolve(action(path, oldStart, newStart, contextLines)).finally(() => bumpPathVersions(path));
    };
  const wrapLineAction =
    (action: (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => void | Promise<void>) =>
    (path: string, oldStart: number, newStart: number, contextLines: number, lineIndices: number[], expectedLines: string[]) => {
      // Same settle-then-bump contract as `wrapHunkAction`.
      void Promise.resolve(action(path, oldStart, newStart, contextLines, lineIndices, expectedLines)).finally(() =>
        bumpPathVersions(path),
      );
    };
  const wrapFileAction = (action: (path: string) => void | Promise<void>) => (path: string) => {
    // Same settle-then-bump contract as `wrapHunkAction`.
    void Promise.resolve(action(path)).finally(() => bumpPathVersions(path));
  };
  // One number per section: the path's own version plus the pane-wide refresh generation. Both
  // only ever increase, so the sum changes whenever either does — and a hunk action, which bumps
  // both (its `runMutation` ends in a `refresh()`), still yields one refetch rather than two when
  // React batches the two updates into one render. Every open section refetches on any refresh:
  // that is deliberate, it's the only signal an external edit, stash apply or pull leaves behind
  // (there's no file watcher). PERF-001's savings come from lazy loading instead — collapsed and
  // off-screen sections never fetch at all.
  const staleness = (key: string) => (diffVersion[key] ?? 0) + refreshGeneration;

  useEffect(() => {
    if (current === null) return;
    sectionRefs.current[entryKey(current)]?.scrollIntoView?.({ block: "nearest" });
  }, [current]);

  const stagedEntries = status.filter((entry) => entry.staged);
  const unstagedEntries = status.filter((entry) => !entry.staged);
  const stagedCount = stagedEntries.length;
  const conflictCount = status.filter((entry) => entry.kind === "Conflicted").length;
  // `git-core::status` reports conflicted entries with `staged: false`, so they sit in the
  // "Changes" group — but staging a conflicted path is what *marks the conflict resolved*, with
  // whatever happens to be in the working tree. One "Stage all" click would silently resolve
  // every outstanding conflict, so the bulk action skips them. The per-row Stage control on a
  // conflicted file is left alone: that's a deliberate, one-file-at-a-time action.
  const stageAllPaths = unstagedEntries
    .filter((entry) => entry.kind !== "Conflicted")
    .map((entry) => entry.path);

  const allEntries = [...unstagedEntries, ...stagedEntries];
  const allKeys = allEntries.map(entryKey);
  const allCollapsed = allKeys.length > 0 && allKeys.every((key) => collapsedKeys.has(key));

  const isEntryCurrent = (entry: StatusEntry) =>
    current !== null && current.path === entry.path && current.staged === entry.staged;
  const isEntrySelected = (entry: StatusEntry) =>
    selection.staged === entry.staged && selection.paths.has(entry.path);

  const groupEntries = (staged: boolean) => (staged ? stagedEntries : unstagedEntries);
  const groupPaths = (staged: boolean) => groupEntries(staged).map((entry) => entry.path);

  /** Makes `entry` the current file with `next` as the selection. */
  const focusEntry = (entry: StatusEntry, next: FileSelection) => {
    setCurrent({ path: entry.path, staged: entry.staged });
    setSelection(next);
  };
  const selectEntry = (entry: StatusEntry) => focusEntry(entry, selectOnlyFile(entry.staged, entry.path));

  // Ctrl/Cmd+click toggles a file, Shift+click selects the range from the anchor. Clicks inside a
  // file's diff keep their own meaning (Shift/Ctrl+click select diff lines there): they only make
  // the file current, keeping the selection when the file is already part of it.
  const handleRowClick = (entry: StatusEntry) => (event?: MouseEvent) => {
    const inBody = event?.target instanceof Element && event.target.closest("[data-file-body]") !== null;
    if (inBody) {
      if (isEntrySelected(entry)) setCurrent({ path: entry.path, staged: entry.staged });
      else selectEntry(entry);
      return;
    }
    if (event?.shiftKey) {
      focusEntry(entry, extendFileSelection(selection, entry.staged, groupPaths(entry.staged), entry.path));
    } else if (event?.ctrlKey || event?.metaKey) {
      const next = toggleFile(selection, entry.staged, entry.path);
      if (next.paths.has(entry.path)) {
        focusEntry(entry, next);
      } else {
        // Toggled off: the current file moves to the anchor (or none is left).
        setSelection(next);
        const anchor = groupEntries(entry.staged).find((candidate) => candidate.path === next.anchor);
        setCurrent(anchor === undefined ? null : { path: anchor.path, staged: anchor.staged });
      }
    } else {
      selectEntry(entry);
    }
  };

  const selectedEntries = (staged: boolean) =>
    selectedPaths(selection, staged, groupPaths(staged)).map(
      (path) => groupEntries(staged).find((entry) => entry.path === path) as StatusEntry,
    );
  // The selection's actions skip conflicted files, as "Stage all" and "Discard all…" do: staging a
  // conflicted file marks it resolved with whatever is in the working tree.
  const summaryOf = (staged: boolean): SelectionSummary | null => {
    const entries = selectedEntries(staged);
    if (entries.length < 2) return null;
    return {
      staged,
      actionable: entries.filter((entry) => entry.kind !== "Conflicted").length,
      untracked: entries.filter(isUntracked).length,
    };
  };
  const unstagedSummary = summaryOf(false);
  const stagedSummary = summaryOf(true);

  const toggleCollapse = (key: string) => {
    setCollapsedKeys((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  const toggleCollapseAll = () => {
    setCollapsedKeys(allCollapsed ? new Set() : new Set(allKeys));
  };

  // Shift+arrow (`extend`) moves the current file and selects the range from the anchor to it.
  const navigateGroup = (entries: StatusEntry[], direction: 1 | -1, extend = false) => {
    if (entries.length === 0) return;
    const currentIndex = entries.findIndex(
      (entry) => current !== null && entry.path === current.path && entry.staged === current.staged,
    );
    const nextIndex =
      currentIndex === -1 ? 0 : Math.min(Math.max(currentIndex + direction, 0), entries.length - 1);
    const next = entries[nextIndex];
    if (!extend) {
      selectEntry(next);
      return;
    }
    // The range starts at the current file; without one in this group, at the file moved to.
    const anchorPath = entries[Math.max(currentIndex, 0)].path;
    const base =
      currentIndex !== -1 && selection.staged === next.staged && selection.paths.size > 0
        ? selection
        : selectOnlyFile(next.staged, anchorPath);
    focusEntry(next, extendFileSelection(base, next.staged, entries.map((entry) => entry.path), next.path));
  };

  const runSelectionAction = (staged: boolean, kind: SelectionActionKind) => {
    const entries = selectedEntries(staged);
    const paths = entries.filter((entry) => entry.kind !== "Conflicted").map((entry) => entry.path);
    if (kind === "stashSelection") {
      if (stashReason !== null || selectionBusyReason !== null || paths.length === 0) return;
      onOpenStashDialog(paths);
      return;
    }
    if (selectionBusyReason !== null) return;
    switch (kind) {
      case "stageSelection":
        if (paths.length === 0) return;
        onStageAllFiles(paths);
        break;
      case "unstageSelection":
        if (paths.length === 0) return;
        onUnstageAllFiles(paths);
        break;
      case "discardSelection":
        // Opens the confirmation; nothing is discarded until it is confirmed.
        if (paths.length > 0) setDiscardRequest({ kind: "files", paths });
        return;
      case "ignoreSelection": {
        const patterns = entries.filter(isUntracked).map((entry) => `/${entry.path}`);
        if (patterns.length === 0) return;
        onIgnorePaths(patterns);
        break;
      }
    }
    // The files leave this group (or the list): don't keep a selection that would come back if
    // they reappeared.
    setSelection(EMPTY_FILE_SELECTION);
  };

  // The per-row Stage/Unstage controls are `<button>`s nested inside a `role="option"` row. That
  // keeps them reachable by mouse and by plain Tab, but ARIA's listbox/option pattern treats an
  // option's children as content contributing to the option's name, not as independent widgets —
  // so assistive tech arrowing through this listbox does not reliably surface them. `s` is the
  // keyboard equivalent, on the container that already owns navigation (`j`/`k`/arrows), so the
  // whole stage/unstage flow is doable without ever reaching those buttons. See
  // `primitives/ListRow.tsx`'s doc comment for why the buttons weren't moved out of the row.
  const toggleStageSelected = (entries: StatusEntry[]) => {
    const staged = entries[0]?.staged;
    if (staged !== undefined && summaryOf(staged) !== null) {
      runSelectionAction(staged, staged ? "unstageSelection" : "stageSelection");
      return;
    }
    const entry = entries.find(
      (candidate) => current !== null && candidate.path === current.path && candidate.staged === current.staged,
    );
    if (entry === undefined) return;
    if (entry.staged) {
      onUnstageFile(entry.path);
    } else {
      onStageFile(entry.path);
    }
  };

  const handleGroupKeyDown = (entries: StatusEntry[]) => (event: KeyboardEvent<HTMLUListElement>) => {
    // The file menu and discard dialog render inside their row (React events bubble through the
    // tree), so keys typed in them must not drive the list underneath.
    if ((event.target as HTMLElement).closest('[role="menu"], dialog') !== null) return;
    // Inside a file's diff (or a text field) Ctrl/Cmd+A and Shift+arrows keep their own meaning.
    const inBody = (event.target as HTMLElement).closest("[data-file-body], input, textarea") !== null;
    const staged = entries[0]?.staged ?? false;
    if (event.key === "ArrowDown" || event.key === "j") {
      if (event.shiftKey && inBody) return;
      event.preventDefault();
      navigateGroup(entries, 1, event.shiftKey);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      if (event.shiftKey && inBody) return;
      event.preventDefault();
      navigateGroup(entries, -1, event.shiftKey);
    } else if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "a") {
      if (inBody || entries.length === 0) return;
      event.preventDefault();
      const next = selectAllFiles(selection, staged, entries.map((entry) => entry.path));
      const keepCurrent = current !== null && current.staged === staged && next.paths.has(current.path);
      if (keepCurrent) setSelection(next);
      else focusEntry(entries[0], next);
    } else if (event.key === "Escape" && summaryOf(staged) !== null && current !== null && current.staged === staged) {
      // Back to just the current file.
      event.preventDefault();
      setSelection(selectOnlyFile(staged, current.path));
    } else if (event.key === "s") {
      event.preventDefault();
      toggleStageSelected(entries);
    } else if (isMenuKey(event)) {
      // Opens a row's actions menu by activating its "⋯" button: the row holding focus when focus
      // is on one of its controls (Tab can put it on a row that isn't selected), else the
      // selected row. A row without a "⋯" (no actions for it) makes this a no-op.
      const row =
        (event.target as HTMLElement).closest('[role="option"]') ??
        event.currentTarget.querySelector(`.${styles.current}`);
      const trigger = row?.querySelector<HTMLButtonElement>('button[aria-haspopup="menu"]') ?? null;
      if (trigger !== null) {
        event.preventDefault();
        trigger.click();
      }
    }
  };

  const [localDiscardRequest, setLocalDiscardRequest] = useState<DiscardRequest | null>(null);
  const discardRequest = onDiscardRequestChange !== undefined ? (discardRequestProp ?? null) : localDiscardRequest;
  const setDiscardRequest = onDiscardRequestChange ?? setLocalDiscardRequest;
  const discardableEntries = unstagedEntries.filter((entry) => entry.kind !== "Conflicted");

  const discardFileEntry =
    discardRequest?.kind === "file"
      ? discardableEntries.find((entry) => entry.path === discardRequest.path)
      : undefined;
  // A selection's discard (#449): the requested files that can still be discarded.
  const discardFilesEntries =
    discardRequest?.kind === "files"
      ? discardableEntries.filter((entry) => discardRequest.paths.includes(entry.path))
      : [];

  // A request whose target is gone (refresh, staging, unmount) must not linger: otherwise the
  // destructive dialog would pop up unprompted when the path later reappears.
  const requestTargetGone =
    discardRequest !== null &&
    (discardRequest.kind === "all"
      ? discardableEntries.length === 0
      : discardRequest.kind === "files"
        ? discardFilesEntries.length === 0
        : discardFileEntry === undefined);
  useEffect(() => {
    if (requestTargetGone) setDiscardRequest(null);
  }, [requestTargetGone, setDiscardRequest]);
  const setDiscardRequestRef = useRef(setDiscardRequest);
  useEffect(() => {
    setDiscardRequestRef.current = setDiscardRequest;
  });
  useEffect(() => () => setDiscardRequestRef.current(null), []);

  // Report the current file to the palette; only unstaged, non-conflicted entries can be discarded.
  const currentDiscardablePath =
    current !== null && !current.staged
      ? (discardableEntries.find((entry) => entry.path === current.path)?.path ?? null)
      : null;
  useEffect(() => {
    onCurrentFileChange?.(currentDiscardablePath);
  }, [currentDiscardablePath, onCurrentFileChange]);
  useEffect(() => () => onCurrentFileChange?.(null), [onCurrentFileChange]);

  const handleStageAll = () => {
    onStageAllFiles(stageAllPaths);
  };

  const handleUnstageAll = () => {
    onUnstageAllFiles(stagedEntries.map((entry) => entry.path));
  };

  const stashReason = stashBlockReason({ status, rebaseProgress, headInfo });

  // Reports a selection of two or more files to the command palette; keyed on its content so an
  // unchanged selection reports nothing new.
  const reportedSelection = unstagedSummary !== null ? false : stagedSummary !== null ? true : null;
  // Conflicted files are left out, as the selection's actions leave them out.
  const reportedPaths =
    reportedSelection === null
      ? []
      : selectedEntries(reportedSelection)
          .filter((entry) => entry.kind !== "Conflicted")
          .map((entry) => entry.path);
  const reportedKey =
    reportedSelection === null || reportedPaths.length === 0 ? "" : `${reportedSelection}\0${reportedPaths.join("\0")}`;
  useEffect(() => {
    if (reportedKey === "") {
      onSelectionChange?.(null);
      return;
    }
    const [staged, ...paths] = reportedKey.split("\0");
    onSelectionChange?.({ staged: staged === "true", paths });
  }, [reportedKey, onSelectionChange]);
  useEffect(() => () => onSelectionChange?.(null), [onSelectionChange]);

  // What the live region announces: the size of a multi-file selection, or the one current file.
  const selectionAnnouncement = (() => {
    for (const staged of [false, true]) {
      const paths = selectedPaths(selection, staged, groupPaths(staged));
      if (paths.length >= 2) return `${paths.length} files selected in ${staged ? "Staged" : "Changes"}`;
      if (paths.length === 1) return `${paths[0]} selected`;
    }
    return "";
  })();

  const renderSelectionBar = (summary: SelectionSummary) => (
    <div
      className={styles.selectionBar}
      role="group"
      aria-label={summary.staged ? "Selected staged files" : "Selected unstaged files"}
    >
      <span>{selectedPaths(selection, summary.staged, groupPaths(summary.staged)).length} selected</span>
      {selectionMenuItems(
        summary,
        (action) => runSelectionAction(summary.staged, action.kind as SelectionActionKind),
        selectionBusyReason,
        stashReason,
      ).map((item) => (
        <button key={item.label} type="button" onClick={item.onSelect} disabled={item.disabled} title={item.title}>
          {item.label}
        </button>
      ))}
      <button
        type="button"
        onClick={() =>
          current !== null && current.staged === summary.staged
            ? setSelection(selectOnlyFile(summary.staged, current.path))
            : setSelection(EMPTY_FILE_SELECTION)
        }
      >
        Clear selection
      </button>
    </div>
  );

  const renderSection = (entry: StatusEntry, index: number) => {
    const key = entryKey(entry);
    const summary = isEntrySelected(entry) ? (entry.staged ? stagedSummary : unstagedSummary) : null;
    return (
      <UncommittedFileSection
        key={key}
        repoPath={repoPath}
        client={client}
        entry={entry}
        status={status}
        isCurrent={isEntryCurrent(entry)}
        selected={isEntrySelected(entry)}
        rowId={`${rowIdPrefix}-${entry.staged ? "staged" : "unstaged"}-${index}`}
        collapsed={collapsedKeys.has(key)}
        diffVersion={staleness(key)}
        onToggleCollapse={() => toggleCollapse(key)}
        onSelect={handleRowClick(entry)}
        selectionSummary={summary}
        onSelectionAction={(kind) => runSelectionAction(entry.staged, kind)}
        selectionBusyReason={selectionBusyReason}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onStageHunk={wrapHunkAction(onStageHunk)}
        onUnstageHunk={wrapHunkAction(onUnstageHunk)}
        onDiscardHunk={wrapHunkAction(onDiscardHunk)}
        onStageLines={wrapLineAction(onStageLines)}
        onUnstageLines={wrapLineAction(onUnstageLines)}
        onDiscardLines={wrapLineAction(onDiscardLines)}
        onSelectRow={onSelectRow}
        onResolveConflict={onResolveConflict}
        onResolveAddDeleteConflict={onResolveAddDeleteConflict}
        onRequestDiscardFile={(path) => setDiscardRequest({ kind: "file", path })}
        onIgnorePath={onIgnorePath}
        onStashFile={(path) => onOpenStashDialog([path])}
        stashDisabledReason={stashReason}
        onOpenFileHistory={onOpenFileHistory}
        onOpenFolderHistory={onOpenFolderHistory}
        externalTools={externalTools}
        onMarkResolved={onMarkResolved}
        onResolveWholeFile={onResolveWholeFile}
        sectionRef={(el) => {
          sectionRefs.current[key] = el;
        }}
      />
    );
  };

  return (
    <div>
      <span aria-live="polite" aria-atomic="true" className={styles.srOnly} data-testid="file-selection-announcement">
        {selectionAnnouncement}
      </span>
      {allKeys.length > 0 && (
        <div className={styles.groupHeading}>
          <CollapseAllToggle allCollapsed={allCollapsed} onToggle={toggleCollapseAll} />
        </div>
      )}
      {unstagedEntries.length > 0 && (
        <div>
          <div className={styles.groupHeading}>
            <span>Changes ({unstagedEntries.length})</span>
            {conflictCount > 0 && (
              <span className={styles.conflictBadge}>
                {conflictCount} conflict{conflictCount === 1 ? "" : "s"}
              </span>
            )}
            {/* Disabled when every unstaged entry is a conflict — the action would be a no-op
                that still costs a full refresh. */}
            <button type="button" onClick={handleStageAll} disabled={stageAllPaths.length === 0}>
              Stage all
            </button>
            <button
              type="button"
              onClick={() => setDiscardRequest({ kind: "all" })}
              disabled={discardableEntries.length === 0}
            >
              Discard all…
            </button>
          </div>
          {unstagedSummary !== null && renderSelectionBar(unstagedSummary)}
          <ul
            className={styles.fileList}
            role="listbox"
            aria-multiselectable="true"
            aria-label="Unstaged changes"
            aria-keyshortcuts={FILE_LIST_SHORTCUTS}
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(unstagedEntries)}
          >
            {unstagedEntries.map(renderSection)}
          </ul>
        </div>
      )}
      {stagedEntries.length > 0 && (
        <div>
          <div className={styles.groupHeading}>
            <span>Staged ({stagedEntries.length})</span>
            <button type="button" onClick={handleUnstageAll}>
              Unstage all
            </button>
            {patchActions !== undefined && (
              <button type="button" onClick={() => patchActions.onSavePatch({ kind: "Staged" }, "staged.patch")}>
                Save as patch…
              </button>
            )}
          </div>
          {stagedSummary !== null && renderSelectionBar(stagedSummary)}
          <ul
            className={styles.fileList}
            role="listbox"
            aria-multiselectable="true"
            aria-label="Staged changes"
            aria-keyshortcuts={FILE_LIST_SHORTCUTS}
            tabIndex={0}
            onKeyDown={handleGroupKeyDown(stagedEntries)}
          >
            {stagedEntries.map(renderSection)}
          </ul>
        </div>
      )}
      {discardRequest?.kind === "all" && discardableEntries.length > 0 && (
        <ConfirmDialog
          ariaLabel="Discard all changes"
          message={discardAllConfirmMessage(discardableEntries)}
          confirmLabel="Discard all"
          onConfirm={() => {
            setDiscardRequest(null);
            onDiscardAllUnstaged(discardableEntries.map((e) => e.path));
          }}
          onCancel={() => setDiscardRequest(null)}
        />
      )}
      {discardRequest?.kind === "files" && discardFilesEntries.length > 0 && (
        <ConfirmDialog
          ariaLabel="Discard selected changes"
          message={discardAllConfirmMessage(discardFilesEntries)}
          confirmLabel={`Discard ${discardFilesEntries.length} ${discardFilesEntries.length === 1 ? "file" : "files"}`}
          confirmDisabled={selectionBusyReason !== null}
          confirmTitle={selectionBusyReason ?? undefined}
          onConfirm={() => {
            if (selectionBusyReason !== null) return;
            setDiscardRequest(null);
            setSelection(EMPTY_FILE_SELECTION);
            onDiscardFiles(discardFilesEntries.map((entry) => entry.path));
          }}
          onCancel={() => setDiscardRequest(null)}
        />
      )}
      {discardFileEntry !== undefined && (
        <ConfirmDialog
          ariaLabel="Discard changes"
          message={discardConfirmMessage(discardFileEntry)}
          confirmLabel={isUntracked(discardFileEntry) ? "Delete file" : "Discard changes"}
          onConfirm={() => {
            setDiscardRequest(null);
            wrapFileAction(onDiscardFile)(discardFileEntry.path);
          }}
          onCancel={() => setDiscardRequest(null)}
        />
      )}
      <div className={styles.commitDock} role="group" aria-label="Commit actions">
        {/* Stashing mid-rebase is destructive in a way nothing else undoes: a paused step's
            resolved/amended content lives in the working tree, so stashing it away and continuing
            lands an empty (or wrong) commit. Disabled for the whole pause by the stash rule the
            file menu and the command palette share (`stashBlockReason`, #495). */}
        <SplitButton
          label="Stash"
          onClick={onSaveStash}
          disabled={stashReason !== null}
          title={stashReason ?? undefined}
          menuLabel="More stash options"
          items={[
            { label: "Stash all", onSelect: () => onSaveStash() },
            { label: "Stash…", onSelect: () => onOpenStashDialog([]) },
          ]}
        />
        {headInfo.sequence !== null ? (
          // A stopped cherry-pick/revert sequence: no editable message per step; Continue
          // commits the resolved step with its MERGE_MSG, like rebase continue.
          <RebaseProgressPanel
            currentStep={headInfo.sequence.currentStep}
            totalSteps={headInfo.sequence.totalSteps}
            disabled={sequencePending || conflictCount > 0}
            disabledReason={conflictReason(conflictCount)}
            onContinue={onSequenceContinue}
            onAbort={onSequenceAbort}
            abortDisabled={sequencePending}
            title={headInfo.sequence.kind === "CherryPick" ? "Cherry-pick in progress" : "Revert in progress"}
            continueLabel={`Continue ${sequenceNoun(headInfo.sequence.kind)}`}
            abortLabel={`Abort ${sequenceNoun(headInfo.sequence.kind)}`}
            hooks={
              commitHooks === undefined ? undefined : { running: commitHooks.running, onCancel: commitHooks.onCancel }
            }
          />
        ) : rebaseProgress !== null ? (
          // An interrupted rebase (changed outside Browsitory) can only be aborted.
          <RebaseProgressPanel
            currentStep={rebaseProgress.currentStep}
            totalSteps={rebaseProgress.totalSteps}
            disabled={conflictCount > 0 || rebaseInterruption(headInfo.rebase) !== undefined}
            disabledReason={rebaseInterruption(headInfo.rebase) ?? conflictReason(conflictCount)}
            onContinue={onRebaseContinue}
            onAbort={onRebaseAbort}
          />
        ) : (
          <CommitBox
            onCommit={onCommit}
            disabled={stagedCount === 0 || conflictCount > 0}
            disabledReason={conflictCount > 0 ? conflictReason(conflictCount) : "Stage changes to commit"}
            onAbortMerge={onAbortMerge}
            abortLabel={
              headInfo.operation === "CherryPick"
                ? "Abort cherry-pick"
                : headInfo.operation === "Revert"
                  ? "Abort revert"
                  : "Abort merge"
            }
            // A pending squash merge pre-fills the message too, but there is no merge to abort.
            // A leftover's message belongs to a commit already made: the next commit is the user's own.
            initialMessage={(headInfo.operationLeftover ? null : mergeMessage) ?? headInfo.squashMessage ?? undefined}
            // A terminal rebase's step writes MERGE_MSG (and a merge step MERGE_HEAD) too; only the
            // terminal ends it (#263).
            showAbort={mergeMessage !== null && headInfo.terminalRebase === null}
            initialDraft={initialCommitDraft}
            onDraftChange={onCommitDraftChange}
            focusMessage={focusCommitMessage}
            onFocusMessageHandled={onCommitMessageFocused}
            hooks={
              commitHooks === undefined
                ? undefined
                : {
                    installed: headInfo.commitHooks.length > 0,
                    // The worker lists `pre-merge-commit` only when the commit will run it.
                    merging: headInfo.commitHooks.includes("pre-merge-commit"),
                    ...commitHooks,
                  }
            }
            clearMessage={clearCommitMessage}
            onClearMessageHandled={onCommitMessageCleared}
            messageSettings={commitMessageSettings}
            loadRecentAuthors={loadRecentAuthors}
            loadMessageHistory={loadMessageHistory}
            amend={{
              // A leftover merge/pick/revert doesn't block amend: the backend clears it first (#240).
              available:
                headInfo.headId !== null &&
                (headInfo.operationLeftover || (headInfo.operation === null && mergeMessage === null)) &&
                headInfo.sequence === null &&
                headInfo.terminalRebase === null,
              on: amendMode,
              headMessage,
              pushed: headInfo.headPushed,
              onToggle: onSetAmendMode,
              onAmend: onAmendCommit,
              headAuthor,
            }}
          />
        )}
      </div>
    </div>
  );
}

function CommitDiffPane({
  repoPath,
  client,
  commitId,
  commits,
  onSelectRow,
  focus,
  onOpenFileHistory,
  onOpenLineHistory,
  onOpenFolderHistory,
  externalTools,
  view,
  onViewChange,
  onRestoreFileVersion,
  restoreBlockedReason,
  patchActions,
}: {
  patchActions?: DiffPanePatchActions;
  repoPath: string;
  client: RepoClient;
  commitId: string;
  commits?: GraphCommit[];
  onSelectRow: (row: SelectedRow) => void;
  focus: FileHistoryFocus | null;
  onOpenFileHistory?: (path: string) => void;
  onOpenLineHistory?: (path: string, revision: string, startLine: number, endLine: number) => void;
  onOpenFolderHistory?: (folder: string) => void;
  externalTools?: ExternalToolsApi;
  view: CommitView;
  onViewChange: (view: CommitView) => void;
  onRestoreFileVersion?: (commitId: string, path: string) => void | Promise<void>;
  restoreBlockedReason: string | null;
}) {
  const knownCommitIds = useMemo(() => new Set((commits ?? []).map((c) => c.id)), [commits]);
  const fetchCommitDiff = (path: string, oldPath: string | null) => (opts: DiffOptions) =>
    oldPath != null
      ? client.getCommitRenameDiff(repoPath, commitId, oldPath, path, opts)
      : client.getCommitDiff(repoPath, commitId, path, opts);
  const [files, setFiles] = useState<string[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());
  const [showAll, setShowAll] = useState(false);
  const [focusCollapsed, setFocusCollapsed] = useState(false);

  const loadFiles = () => {
    let ignore = false;
    client
      .getCommitFiles(repoPath, commitId)
      .then((next) => {
        if (!ignore) {
          setFiles(next);
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          setError(describeError(err));
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    return loadFiles();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, commitId]);

  const allCollapsed = files.length > 0 && files.every((path) => collapsedPaths.has(path));

  const toggleCollapse = (path: string) => {
    setCollapsedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const toggleCollapseAll = () => {
    setCollapsedPaths(allCollapsed ? new Set() : new Set(files));
  };

  if (error !== null) {
    return (
      <InlineError
        message={error.message}
        hint={error.hint}
        onDismiss={() => setError(null)}
        onRetry={loadFiles}
      />
    );
  }

  const focused = focus !== null && !showAll;

  return (
    <div>
      <CommitHeader
        key={commitId}
        repoPath={repoPath}
        client={client}
        commitId={commitId}
        commit={commits?.find((c) => c.id === commitId)}
        knownCommitIds={knownCommitIds}
        onSelectRow={onSelectRow}
      />
      <div className={treeStyles.viewToggle} role="group" aria-label="Commit files">
        <button type="button" aria-pressed={view === "changes"} onClick={() => onViewChange("changes")}>
          Changes
        </button>
        <button type="button" aria-pressed={view === "tree"} onClick={() => onViewChange("tree")}>
          All files
        </button>
      </div>
      {view === "tree" ? (
        <CommitTreeBrowser
          repoPath={repoPath}
          client={client}
          commitId={commitId}
          onSelectRow={onSelectRow}
          onOpenFileHistory={onOpenFileHistory}
          onOpenLineHistory={onOpenLineHistory}
          onOpenFolderHistory={onOpenFolderHistory}
          externalTools={externalTools}
          onRestoreFileVersion={onRestoreFileVersion}
          restoreBlockedReason={restoreBlockedReason}
        />
      ) : focused ? (
        <>
          <ul className={styles.fileList}>
            <ReadOnlyFileSection
              key={focus.path}
              repoPath={repoPath}
              client={client}
              path={focus.path}
              oldPath={focus.oldPath}
              diffKey={`commit:${commitId}`}
              fetchDiff={fetchCommitDiff(focus.path, focus.oldPath)}
              blameAt={commitId}
              collapsed={focusCollapsed}
              onToggleCollapse={() => setFocusCollapsed((c) => !c)}
              onSelectRow={onSelectRow}
              onOpenLineHistory={onOpenLineHistory}
              onOpenFolderHistory={onOpenFolderHistory}
              externalTools={externalTools}
              toolSource={{ kind: "Commit", commitId, oldPath: focus.oldPath }}
              previewSource={{ kind: "Commit", commitId, oldPath: focus.oldPath }}
              patch={patchActions && commitFilePatch(patchActions, commitId, focus.path, focus.oldPath)}
            />
          </ul>
          {files.length > 1 && (
            <button type="button" onClick={() => setShowAll(true)}>
              Show all {files.length} files
            </button>
          )}
        </>
      ) : (
        <>
          {files.length > 0 && (
            <div className={styles.groupHeading}>
              <CollapseAllToggle allCollapsed={allCollapsed} onToggle={toggleCollapseAll} />
            </div>
          )}
          <ul className={styles.fileList}>
            {files.map((path) => (
              <ReadOnlyFileSection
                key={path}
                repoPath={repoPath}
                client={client}
                path={path}
                diffKey={`commit:${commitId}`}
                fetchDiff={fetchCommitDiff(path, null)}
                blameAt={commitId}
                collapsed={collapsedPaths.has(path)}
                onToggleCollapse={() => toggleCollapse(path)}
                onSelectRow={onSelectRow}
                onOpenFileHistory={onOpenFileHistory}
                onOpenLineHistory={onOpenLineHistory}
                onOpenFolderHistory={onOpenFolderHistory}
                externalTools={externalTools}
                toolSource={{ kind: "Commit", commitId, oldPath: null }}
                previewSource={{ kind: "Commit", commitId, oldPath: null }}
                patch={patchActions && commitFilePatch(patchActions, commitId, path, null)}
              />
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

/**
 * A selected stash: its message, `stash@{n}`, Apply / Pop / Drop… with "Restore staged changes",
 * then its tracked changes (diffed against the HEAD it was made on) and its untracked files
 * (diffed as whole-file adds through the parentless untracked-files commit).
 */
function StashDiffPane({
  repoPath,
  client,
  stash,
  actions,
  onSelectRow,
}: {
  repoPath: string;
  client: RepoClient;
  stash: StashEntry;
  actions: StashActions;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const [files, setFiles] = useState<StashFiles | null>(null);
  const [error, setError] = useState<DescribedError | null>(null);
  const [collapsedPaths, setCollapsedPaths] = useState<Set<string>>(new Set());
  const [confirmingDrop, setConfirmingDrop] = useState(false);

  const loadFiles = () => {
    let ignore = false;
    client
      .getStashFiles(repoPath, stash.commitId)
      .then((next) => {
        if (!ignore) {
          setFiles(next);
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) {
          setError(describeError(err));
        }
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    return loadFiles();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, stash.commitId]);

  const toggleCollapse = (key: string) => {
    setCollapsedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const applyTitle = actions.disabledReason ?? undefined;
  const untrackedCommitId = files?.untrackedCommitId ?? null;

  return (
    <section aria-label="Stash details" className={styles.stashDetails}>
      <div className={styles.stashHeader}>
        <h2 className={styles.stashMessage}>{stash.message}</h2>
        <span className={styles.stashRef}>
          stash@{"{"}
          {stash.index}
          {"}"} · {stash.commitId.slice(0, 7)}
        </span>
        <Toolbar aria-label="Stash actions">
          <button
            type="button"
            disabled={actions.disabledReason !== null}
            title={applyTitle}
            onClick={() => actions.onApply(stash.index)}
          >
            Apply
          </button>
          <button
            type="button"
            disabled={actions.disabledReason !== null}
            title={applyTitle}
            onClick={() => actions.onPop(stash.index)}
          >
            Pop
          </button>
          <button type="button" onClick={() => setConfirmingDrop(true)}>
            Drop…
          </button>
          <label className={styles.restoreIndex}>
            <input
              type="checkbox"
              checked={actions.restoreIndex}
              onChange={(event) => actions.onSetRestoreIndex(event.target.checked)}
            />
            Restore staged changes
          </label>
        </Toolbar>
      </div>
      {error !== null ? (
        <InlineError message={error.message} hint={error.hint} onDismiss={() => setError(null)} onRetry={loadFiles} />
      ) : files === null ? null : files.tracked.length === 0 && files.untracked.length === 0 ? (
        <p className={styles.stashEmpty}>This stash has no file changes.</p>
      ) : (
        <>
          {files.tracked.length > 0 && (
            <>
              <div className={styles.groupHeading}>
                <span>Changes ({files.tracked.length})</span>
              </div>
              <ul className={styles.fileList}>
                {files.tracked.map((path) => (
                  <ReadOnlyFileSection
                    key={path}
                    repoPath={repoPath}
                    client={client}
                    path={path}
                    diffKey={`commit:${stash.commitId}`}
                    fetchDiff={(opts) => client.getCommitDiff(repoPath, stash.commitId, path, opts)}
                    previewSource={{ kind: "Commit", commitId: stash.commitId, oldPath: null }}
                    blameAt={stash.commitId}
                    collapsed={collapsedPaths.has(`t:${path}`)}
                    onToggleCollapse={() => toggleCollapse(`t:${path}`)}
                    onSelectRow={onSelectRow}
                  />
                ))}
              </ul>
            </>
          )}
          {files.untracked.length > 0 && untrackedCommitId !== null && (
            <>
              <div className={styles.groupHeading}>
                <span>Untracked ({files.untracked.length})</span>
              </div>
              <ul className={styles.fileList}>
                {files.untracked.map((path) => (
                  <ReadOnlyFileSection
                    key={path}
                    repoPath={repoPath}
                    client={client}
                    path={path}
                    diffKey={`commit:${untrackedCommitId}`}
                    fetchDiff={(opts) => client.getCommitDiff(repoPath, untrackedCommitId, path, opts)}
                    previewSource={{ kind: "Commit", commitId: untrackedCommitId, oldPath: null }}
                    blameAt={untrackedCommitId}
                    collapsed={collapsedPaths.has(`u:${path}`)}
                    onToggleCollapse={() => toggleCollapse(`u:${path}`)}
                    onSelectRow={onSelectRow}
                  />
                ))}
              </ul>
            </>
          )}
        </>
      )}
      {confirmingDrop && (
        <ConfirmDialog
          ariaLabel={`Drop stash ${stash.message}`}
          message={<p>Drop "{stash.message}"? This cannot be undone.</p>}
          confirmLabel="Drop stash"
          onConfirm={() => {
            setConfirmingDrop(false);
            actions.onDrop(stash.index);
          }}
          onCancel={() => setConfirmingDrop(false)}
        />
      )}
    </section>
  );
}
