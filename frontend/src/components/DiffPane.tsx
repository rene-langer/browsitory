import { describeError, type DescribedError } from "../lib/errorMessages";
import { conflictReason, rebaseInterruption, sequenceNoun } from "../lib/operationStatus";
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
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type {
  AmendOptions,
  BlameLine,
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
import { useCommitMessageSettingsContext } from "../state/useCommitMessageSettings";
import { useDiffSettingsContext } from "../state/useDiffSettings";
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
  isUntracked,
  type FileAction,
} from "./FileActionsMenu";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu } from "./primitives/ContextMenu";
import { isMenuKey, menuAnchor, rectAnchor } from "../lib/menuKeys";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { ReadOnlyFileSection } from "./ReadOnlyFileSection";
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
  collapsed,
  diffVersion,
  onToggleCollapse,
  onSelect,
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
  onOpenFileHistory,
  externalTools,
  onMarkResolved,
  onResolveWholeFile,
  sectionRef,
}: {
  repoPath: string;
  client: RepoClient;
  entry: StatusEntry;
  status: StatusEntry[];
  isCurrent: boolean;
  collapsed: boolean;
  // This section's staleness signal: its own per-path version plus the pane-wide refresh
  // generation, combined by the parent (`UncommittedDiffPane`) into one monotonically increasing
  // number — see `staleness` there for how each part is bumped. Any change means "this diff may
  // be stale, refetch it".
  diffVersion: number;
  onToggleCollapse: () => void;
  onSelect: () => void;
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
  onOpenFileHistory?: (path: string) => void;
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
    }
  };
  // Without external tools, conflicted entries have no actions and staged ones only "Stash file…"
  // (see `fileMenuItems`).
  const menuItems = fileMenuItems(
    entry,
    onPickFileAction,
    externalTools === undefined ? undefined : { mergeToolRunning: externalTools.mergeToolRunning.has(entry.path) },
  );
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
  const diffOptions = { ignoreWhitespace: settings.ignoreWhitespace, contextLines: settings.contextLines };
  const loadDiff = () => {
    let ignore = false;
    const requested = diffOptions;
    (renamedFrom === null
      ? client.getWorkingDiff(repoPath, entry.path, entry.staged, requested)
      : client.getWorkingDiff(repoPath, entry.path, entry.staged, requested, renamedFrom)
    )
      .then((next) => {
        if (!ignore) {
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
  }, [repoPath, client, entry.path, entry.staged, renamedFrom, isConflicted, showImage, mode, diffVersion, collapsed, nearViewport, settings.ignoreWhitespace, settings.contextLines]);

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
    fetchedOptions.contextLines === settings.contextLines;
  const fetchedContext = fetchedOptions?.contextLines ?? settings.contextLines;
  const unstagedActions = optionsCurrent && !entry.staged;
  const stagedActions = optionsCurrent && entry.staged;

  return (
    <ListRow
      selected={isCurrent}
      onClick={onSelect}
      className={isConflicted ? `${styles.fileSection} ${styles.conflicted}` : styles.fileSection}
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
          {hasFileActions && (
            <button
              type="button"
              className={styles.stageToggle}
              ref={actionsRef}
              aria-label={`Actions for ${entry.path}`}
              aria-haspopup="menu"
              onClick={(event) => {
                event.stopPropagation();
                // A pointer click (`detail > 0`) need not focus the button (WebKit), so name it as
                // where focus returns. A key, or DiffPane's Menu-key `click()`, leaves focus where it is.
                setMenu({
                  ...rectAnchor(event.currentTarget),
                  returnFocusTo: event.detail > 0 ? event.currentTarget : undefined,
                });
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
        <div className={styles.fileSectionBody} ref={sectionRef}>
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

const NO_HEAD_INFO: HeadInfo = { headId: null, detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [], sequence: null, rebase: null };
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
  refreshGeneration = 0,
  initialCommitDraft,
  onCommitDraftChange,
  discardRequest,
  onDiscardRequestChange,
  onCurrentFileChange,
  fileHistoryFocus,
  onOpenFileHistory,
  stashes = [],
  stashActions,
  externalTools,
  onMarkResolved,
  onResolveWholeFile,
}: {
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
        externalTools={externalTools}
        onMarkResolved={onMarkResolved}
        onResolveWholeFile={onResolveWholeFile}
        commits={commits}
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
      focus={fileHistoryFocus ?? null}
      externalTools={externalTools}
    />
  );
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
export type DiscardRequest = { kind: "file"; path: string } | { kind: "all" };

/** The file whose history the graph is showing, at the selected commit. */
export type FileHistoryFocus = { path: string; oldPath: string | null };

function entryKey(entry: { path: string; staged: boolean }): string {
  return `${entry.staged}:${entry.path}`;
}

function UncommittedDiffPane({
  repoPath,
  client,
  commits,
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
  externalTools,
  onMarkResolved,
  onResolveWholeFile,
}: {
  onOpenFileHistory?: (path: string) => void;
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
  /** The graph's commits, for HEAD's author in the amend Author field. */
  commits?: GraphCommit[];
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
  const headCommit = headInfo.headId === null ? undefined : commits?.find((c) => c.id === headInfo.headId);
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

  const isEntrySelected = (entry: StatusEntry) =>
    current !== null && current.path === entry.path && current.staged === entry.staged;

  const selectEntry = (entry: StatusEntry) => {
    setCurrent({ path: entry.path, staged: entry.staged });
  };

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

  const navigateGroup = (entries: StatusEntry[], direction: 1 | -1) => {
    if (entries.length === 0) return;
    const currentIndex = entries.findIndex(
      (entry) => current !== null && entry.path === current.path && entry.staged === current.staged,
    );
    const nextIndex =
      currentIndex === -1 ? 0 : Math.min(Math.max(currentIndex + direction, 0), entries.length - 1);
    selectEntry(entries[nextIndex]);
  };

  // The per-row Stage/Unstage controls are `<button>`s nested inside a `role="option"` row. That
  // keeps them reachable by mouse and by plain Tab, but ARIA's listbox/option pattern treats an
  // option's children as content contributing to the option's name, not as independent widgets —
  // so assistive tech arrowing through this listbox does not reliably surface them. `s` is the
  // keyboard equivalent, on the container that already owns navigation (`j`/`k`/arrows), so the
  // whole stage/unstage flow is doable without ever reaching those buttons. See
  // `primitives/ListRow.tsx`'s doc comment for why the buttons weren't moved out of the row.
  const toggleStageSelected = (entries: StatusEntry[]) => {
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
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      navigateGroup(entries, 1);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      navigateGroup(entries, -1);
    } else if (event.key === "s") {
      event.preventDefault();
      toggleStageSelected(entries);
    } else if (isMenuKey(event)) {
      // Opens a row's actions menu by activating its "⋯" button: the row holding focus when focus
      // is on one of its controls (Tab can put it on a row that isn't selected), else the
      // selected row. A row without a "⋯" (no actions for it) makes this a no-op.
      const row =
        (event.target as HTMLElement).closest('[role="option"]') ??
        event.currentTarget.querySelector('[aria-selected="true"]');
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

  // A request whose target is gone (refresh, staging, unmount) must not linger: otherwise the
  // destructive dialog would pop up unprompted when the path later reappears.
  const requestTargetGone =
    discardRequest !== null &&
    (discardRequest.kind === "all"
      ? discardableEntries.length === 0
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

  const renderSection = (entry: StatusEntry) => {
    const key = entryKey(entry);
    return (
      <UncommittedFileSection
        key={key}
        repoPath={repoPath}
        client={client}
        entry={entry}
        status={status}
        isCurrent={isEntrySelected(entry)}
        collapsed={collapsedKeys.has(key)}
        diffVersion={staleness(key)}
        onToggleCollapse={() => toggleCollapse(key)}
        onSelect={() => selectEntry(entry)}
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
        onOpenFileHistory={onOpenFileHistory}
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
          <ul
            className={styles.fileList}
            role="listbox"
            aria-label="Unstaged changes"
            aria-keyshortcuts="s"
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
          </div>
          <ul
            className={styles.fileList}
            role="listbox"
            aria-label="Staged changes"
            aria-keyshortcuts="s"
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
            lands an empty (or wrong) commit. Disabled for the whole pause, same rule as
            `BranchSwitcher`'s ref-mutating actions. */}
        <SplitButton
          label="Stash"
          onClick={onSaveStash}
          disabled={status.length === 0 || rebaseProgress !== null || headInfo.sequence !== null}
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
            initialMessage={mergeMessage ?? headInfo.squashMessage ?? undefined}
            showAbort={mergeMessage !== null}
            initialDraft={initialCommitDraft}
            onDraftChange={onCommitDraftChange}
            focusMessage={focusCommitMessage}
            onFocusMessageHandled={onCommitMessageFocused}
            hooks={
              commitHooks === undefined
                ? undefined
                : {
                    installed: headInfo.commitHooks.length > 0,
                    merging: headInfo.operation === "Merge",
                    ...commitHooks,
                  }
            }
            clearMessage={clearCommitMessage}
            onClearMessageHandled={onCommitMessageCleared}
            messageSettings={commitMessageSettings}
            loadRecentAuthors={loadRecentAuthors}
            amend={{
              available:
                headInfo.headId !== null &&
                headInfo.operation === null &&
                headInfo.sequence === null &&
                mergeMessage === null,
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
  externalTools,
}: {
  repoPath: string;
  client: RepoClient;
  commitId: string;
  commits?: GraphCommit[];
  onSelectRow: (row: SelectedRow) => void;
  focus: FileHistoryFocus | null;
  onOpenFileHistory?: (path: string) => void;
  externalTools?: ExternalToolsApi;
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
      {focused ? (
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
              externalTools={externalTools}
              toolSource={{ kind: "Commit", commitId, oldPath: focus.oldPath }}
              previewSource={{ kind: "Commit", commitId, oldPath: focus.oldPath }}
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
                externalTools={externalTools}
                toolSource={{ kind: "Commit", commitId, oldPath: null }}
                previewSource={{ kind: "Commit", commitId, oldPath: null }}
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
