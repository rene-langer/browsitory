import { ChevronDown, ChevronRight, Minus, MoreHorizontal, Plus } from "lucide-react";
import { useRef, useState, type MouseEvent } from "react";
import type { StatusEntry } from "../ipc/RepoClient";
import { menuAnchor, rectAnchor } from "../lib/menuKeys";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { useUserCommandsContext } from "../state/userCommandsContext";
import { NO_SELECTION, userCommandMenuItems } from "../lib/userCommands";
import type { SelectionActionKind } from "./diffPaneShared";
import styles from "./DiffPane.module.css";
import {
  fileMenuItems,
  fileToolItems,
  selectionMenuItems,
  type FileAction,
  type SelectionSummary,
} from "./FileActionsMenu";
import { ContextMenu } from "./primitives/ContextMenu";
import { ListRow } from "./primitives/ListRow";
import { useMarkResolved } from "./useMarkResolved";
import { statusLetter } from "../lib/statusLetter";

/**
 * One file in the working tree's list (#639): its status letter and path, its "⋯" menu and its
 * Stage/Unstage toggle. Its diff is in `UncommittedFileViewer` while it is the current file.
 */
export function UncommittedFileRow({
  entry,
  isCurrent,
  selected,
  rowId,
  onSelect,
  selectionSummary,
  onSelectionAction,
  selectionBusyReason,
  onRequestDiscardFile,
  onStashFile,
  stashDisabledReason,
  editorLine,
  rowRef,
  name,
  folderFiles,
}: {
  /**
   * An untracked folder listed as one entry (#845): whether its files are listed under it, and
   * the chevron that lists or hides them.
   */
  folderFiles?: { shown: boolean; onToggle: () => void };
  entry: StatusEntry;
  /** In the tree layout (#729), the file's name in its folder: shown instead of the whole path. */
  name?: string;
  /** The file the viewer shows; single-file keys act on it. */
  isCurrent: boolean;
  /** Part of the selection (`aria-selected`); the current file always is. */
  selected: boolean;
  rowId: string;
  onSelect: (event?: MouseEvent) => void;
  /** Set while this row is part of a selection of two or more files: its menu acts on all of them. */
  selectionSummary: SelectionSummary | null;
  onSelectionAction: (kind: SelectionActionKind) => void;
  /** Why the selection's repository actions are disabled (another action is running), else `null`. */
  selectionBusyReason: string | null;
  onRequestDiscardFile: (path: string) => void;
  onStashFile: (path: string) => void;
  /** `stashBlockReason`: disables "Stash file…" like the Stash button. */
  stashDisabledReason: string | null;
  /** The line "Open in editor" goes to: the first change shown, when this file's diff is loaded. */
  editorLine: () => number | null;
  rowRef: (el: HTMLElement | null) => void;
}) {
  const { onStageFile, onUnstageFile, onIgnorePath, externalTools, onMarkResolved } = useWorkspaceActions();
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo?: HTMLElement } | null>(null);
  const actionsRef = useRef<HTMLButtonElement>(null);
  const { markResolved, confirmDialog } = useMarkResolved(entry.path);
  const diffSource = entry.staged
    ? { kind: "Staged" as const, oldPath: entry.oldPath ?? null }
    : { kind: "Unstaged" as const };
  const onPickFileAction = (action: FileAction) => {
    switch (action.kind) {
      case "stage":
        return onStageFile(entry.path);
      case "discard":
        return onRequestDiscardFile(entry.path);
      case "stash":
        return onStashFile(entry.path);
      case "ignore":
        return onIgnorePath(action.pattern);
      case "openInEditor":
        return externalTools?.openInEditor(entry.path, editorLine());
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
  const userCommands = useUserCommandsContext();
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
      : [
          ...fileMenuItems(entry, onPickFileAction, tools, stashDisabledReason),
          ...(userCommands === null
            ? []
            : userCommandMenuItems(userCommands.commands, "File", { ...NO_SELECTION, paths: [entry.path] }, userCommands.run)),
        ];
  const hasFileActions = menuItems.length > 0;
  const mergeToolResult = externalTools?.mergeToolResults.get(entry.path);
  const isConflicted = entry.kind === "Conflicted";
  const { letter, kind } = statusLetter(entry);
  const renamedFrom = entry.staged ? (entry.oldPath ?? null) : null;

  return (
    <ListRow
      id={rowId}
      selected={selected}
      onClick={onSelect}
      className={[styles.fileRow, isConflicted ? styles.conflicted : null, isCurrent ? styles.current : null]
        .filter((name) => name !== null)
        .join(" ")}
    >
      <div
        className={styles.fileRowMain}
        ref={rowRef}
        title={renamedFrom === null ? `${entry.path} (${kind})` : `${entry.path} (${kind} from ${renamedFrom})`}
        onContextMenu={
          hasFileActions
            ? (event) => {
                event.preventDefault();
                // The row isn't focusable: closing from the keyboard or an item lands on "…".
                setMenu({ ...menuAnchor(event, event.currentTarget), returnFocusTo: actionsRef.current ?? undefined });
              }
            : undefined
        }
      >
        <span className={styles.statusLetter} data-kind={entry.kind} aria-hidden="true">
          {letter}
        </span>
        {/* The kind is spelled out for assistive tech (and the E2E specs, which find a row by
            "path (Kind)"); on screen the letter shows it. */}
        {name === undefined ? (
          <span className={styles.path}>
            {entry.path}
            <span className={styles.srOnly}> ({entry.kind})</span>
          </span>
        ) : (
          // In a folder the name is enough on screen; assistive tech still hears the whole path.
          <span className={styles.path}>
            <span aria-hidden="true">{name}</span>
            <span className={styles.srOnly}>
              {entry.path} ({entry.kind})
            </span>
          </span>
        )}
        {entry.untrackedFolder === true && (
          <span className={styles.folderNote}>untracked folder, more than 500 files</span>
        )}
        <div className={styles.rowActions}>
          {folderFiles !== undefined && (
            <button
              type="button"
              className={styles.stageToggle}
              aria-label={`${folderFiles.shown ? "Hide" : "Show"} files in ${entry.path}`}
              aria-expanded={folderFiles.shown}
              onClick={(event) => {
                event.stopPropagation();
                folderFiles.onToggle();
              }}
            >
              {folderFiles.shown ? (
                <ChevronDown size={14} aria-hidden="true" />
              ) : (
                <ChevronRight size={14} aria-hidden="true" />
              )}
            </button>
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
                // where focus returns. A key, or the list's Menu-key `click()`, leaves focus where it is.
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
              title="Unstage"
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
              title="Stage"
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
      {confirmDialog}
    </ListRow>
  );
}
