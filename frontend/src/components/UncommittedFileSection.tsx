import type { SelectionActionKind } from "./diffPaneShared";
import { describeError, type DescribedError } from "../lib/errorMessages";
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
import { useEffect, useRef, useState, type MouseEvent } from "react";
import type {
  BlameLine,
  DiffHunk,
  DiffSource,
  DiffOptions,
  HunkOptions,
  RepoClient,
  StatusEntry,
  StatusKind,
} from "../ipc/RepoClient";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useWholeFile } from "../state/useWholeFile";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { BlameView } from "./BlameView";
import { CollapseToggle } from "./CollapseToggle";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import { DiffView } from "./DiffView";
import styles from "./DiffPane.module.css";
import { ImageDiff, ImageTextToggle } from "./ImageDiff";
import { imageMimeType, isSvgPath } from "../lib/imageTypes";
import {
  fileMenuItems,
  fileToolItems,
  isUntracked,
  selectionMenuItems,
  type FileAction,
  type SelectionSummary,
} from "./FileActionsMenu";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu } from "./primitives/ContextMenu";
import { menuAnchor, rectAnchor } from "../lib/menuKeys";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { FolderHistoryButton } from "./FolderHistoryButton";

const STATUS_ICONS: Record<StatusKind, LucideIcon> = {
  New: FilePlus,
  Modified: Pencil,
  Deleted: FileMinus,
  Renamed: ArrowRightLeft,
  TypeChange: FileDiff,
  Conflicted: AlertTriangle,
};

export function UncommittedFileSection({
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
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onStageLines,
  onUnstageLines,
  onDiscardLines,
  onRequestDiscardFile,
  onStashFile,
  stashDisabledReason,
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
  onStageHunk: (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void;
  onUnstageHunk: (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void;
  onDiscardHunk: (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void;
  onStageLines: (path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) => void;
  onUnstageLines: (path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) => void;
  onDiscardLines: (path: string, oldStart: number, newStart: number, hunk: HunkOptions, lineIndices: number[], expectedLines: string[]) => void;
  onRequestDiscardFile: (path: string) => void;
  onStashFile: (path: string) => void;
  /** `stashBlockReason`: disables "Stash file…" like the Stash button. */
  stashDisabledReason: string | null;
  sectionRef: (el: HTMLDivElement | null) => void;
}) {
  const {
    onStageFile,
    onUnstageFile,
    onSelectRow,
    onResolveConflict,
    onResolveAddDeleteConflict,
    onIgnorePath,
    onOpenFileHistory,
    onOpenFolderHistory,
    externalTools,
    onMarkResolved,
    onResolveWholeFile,
  } = useWorkspaceActions();
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
  const diffOptions: DiffOptions = {
    ignoreWhitespace: settings.ignoreWhitespace,
    contextLines: wholeFile.contextLines,
    algorithm: settings.algorithm,
  };
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
  }, [repoPath, client, entry.path, entry.staged, renamedFrom, isConflicted, showImage, mode, diffVersion, collapsed, nearViewport, settings.ignoreWhitespace, wholeFile.contextLines, settings.algorithm]);

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
    fetchedOptions.contextLines === wholeFile.contextLines &&
    fetchedOptions.algorithm === settings.algorithm;
  // The displayed hunks' context and algorithm: hunk and line actions re-diff with exactly these.
  const fetchedHunk: HunkOptions = {
    contextLines: fetchedOptions?.contextLines ?? wholeFile.contextLines,
    algorithm: fetchedOptions?.algorithm ?? settings.algorithm,
  };
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
              onStageHunk={unstagedActions ? (oldStart, newStart) => onStageHunk(entry.path, oldStart, newStart, fetchedHunk) : undefined}
              onUnstageHunk={stagedActions ? (oldStart, newStart) => onUnstageHunk(entry.path, oldStart, newStart, fetchedHunk) : undefined}
              onDiscardHunk={unstagedActions ? (oldStart, newStart) => onDiscardHunk(entry.path, oldStart, newStart, fetchedHunk) : undefined}
              onStageLines={unstagedActions ? (o, n, idx, shown) => onStageLines(entry.path, o, n, fetchedHunk, idx, shown) : undefined}
              onUnstageLines={stagedActions ? (o, n, idx, shown) => onUnstageLines(entry.path, o, n, fetchedHunk, idx, shown) : undefined}
              onDiscardLines={unstagedActions ? (o, n, idx, shown) => onDiscardLines(entry.path, o, n, fetchedHunk, idx, shown) : undefined}
              partialActionsDisabled={fetchedOptions?.ignoreWhitespace ?? settings.ignoreWhitespace}
              wholeFile={wholeFile}
              current={isCurrent}
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
