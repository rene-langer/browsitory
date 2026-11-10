import { MoreHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { BlameLine, CommitHunk, DiffHunk, DiffOptions, DiffSource, FileStats, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import type { SelectedRow } from "../state/useAppState";
import type { ExternalToolsApi } from "../state/useExternalTools";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useWholeFile } from "../state/useWholeFile";
import { FolderHistoryButton } from "./FolderHistoryButton";
import { LineHistoryButton } from "./LineHistoryButton";
import { BlameView } from "./BlameView";
import { CollapseToggle } from "./CollapseToggle";
import styles from "./DiffPane.module.css";
import { DiffView, type HunkHeaderAction } from "./DiffView";
import { ImageDiff, ImageTextToggle } from "./ImageDiff";
import { imageMimeType, isSvgPath } from "../lib/imageTypes";
import { commitFileMenuItems, type FileAction } from "./FileActionsMenu";
import { ContextMenu } from "./primitives/ContextMenu";
import { isMenuKey, menuAnchor, rectAnchor } from "../lib/menuKeys";
import { InlineError } from "./primitives/InlineError";
import { LineStatsBadge } from "./LineStatsBadge";

/**
 * A commit's file's revert/apply/save actions (#448). The hunk actions get the hunk as the view
 * showed it (with its context lines), which the backend finds again.
 */
export interface CommitFilePatchActions {
  /** Why reverting or applying can't run now, else `null`. */
  blockedReason: string | null;
  /** `stage`: also stage the result (#536). */
  onRevertFile: (stage: boolean) => void;
  onApplyFile: (stage: boolean) => void;
  onSavePatch: () => void;
  onRevertHunk: (hunk: CommitHunk, stage: boolean) => void;
  onApplyHunk: (hunk: CommitHunk, stage: boolean) => void;
}

/** A commit's hunk buttons' title when they can run: Shift+click stages the result (#536). */
export const HUNK_STAGE_HINT = "Shift+click to stage the result too";

/** Hunk identity needs whitespace to count, as for staging. */
const IGNORE_WHITESPACE_REASON = "Turn off Ignore whitespace to revert or apply a hunk.";

/**
 * One file's diff in a read-only view (a commit or a compare), with Blame and History.
 * `fetchDiff` runs again whenever `diffKey`, the path, the diff settings or "Show whole file"
 * change. With
 * `externalTools` and `toolSource`, a "…" button and right-click menu open the file in the
 * user's editor, file manager or diff tool. With `previewSource`, an image file (by extension)
 * shows a before/after preview instead of hunks; an SVG can switch to its text diff.
 */
export function ReadOnlyFileSection({
  repoPath,
  client,
  path,
  oldPath,
  diffKey,
  fetchDiff,
  blameAt,
  collapsed,
  onToggleCollapse,
  onSelectRow,
  onOpenFileHistory,
  onOpenLineHistory,
  onOpenFolderHistory,
  externalTools,
  toolSource,
  previewSource,
  patch,
  lineStats,
}: {
  repoPath: string;
  client: RepoClient;
  path: string;
  oldPath?: string | null;
  /** Identifies the diff source; a change refetches. */
  diffKey: string;
  fetchDiff: (opts: DiffOptions) => Promise<DiffHunk[]>;
  /** Revision Blame runs at (a commit id or "HEAD"). */
  blameAt: string;
  collapsed: boolean;
  onToggleCollapse: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onOpenFileHistory?: (path: string) => void;
  /** "History of lines" (#447) for the lines selected in the diff or blame view: their numbers
   *  are the new side's, which is `blameAt`'s version of the file. Leave it out when the new side
   *  is not a commit. */
  onOpenLineHistory?: (path: string, revision: string, startLine: number, endLine: number) => void;
  /** "Folder history" (#447) of the file's folder. */
  onOpenFolderHistory?: (folder: string) => void;
  externalTools?: ExternalToolsApi;
  /** The two versions "Open in diff tool" compares. */
  toolSource?: DiffSource;
  /** The two versions an image preview shows (the same ones the diff compares). */
  previewSource?: DiffSource;
  /** A commit's file: Revert / Apply / Save as patch in its menu, Revert / Apply hunk (#448). */
  patch?: CommitFilePatchActions;
  /** A commit's file: its lines added and removed (#620), or "Binary". */
  lineStats?: FileStats;
}) {
  const [mode, setMode] = useState<"diff" | "blame">("diff");
  const isImage = previewSource !== undefined && imageMimeType(path) !== null;
  const [imageAsText, setImageAsText] = useState(false);
  const showImage = isImage && !imageAsText;
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo?: HTMLElement } | null>(null);
  const actionsRef = useRef<HTMLButtonElement>(null);
  const tools = externalTools !== undefined && toolSource !== undefined ? { externalTools, toolSource } : null;
  const hasMenu = tools !== null || patch !== undefined;
  const onPickFileAction = (action: FileAction) => {
    if (patch !== undefined) {
      if (action.kind === "revertCommitFile") return patch.onRevertFile(action.stage);
      if (action.kind === "applyCommitFile") return patch.onApplyFile(action.stage);
      if (action.kind === "saveFilePatch") return patch.onSavePatch();
    }
    if (tools === null) return;
    if (action.kind === "openInEditor") tools.externalTools.openInEditor(path, null);
    else if (action.kind === "reveal") tools.externalTools.reveal(path);
    else if (action.kind === "diffTool") tools.externalTools.openDiffTool(path, tools.toolSource);
  };
  const bodyRef = useRef<HTMLDivElement>(null);
  const [hunks, setHunks] = useState<DiffHunk[] | null>(null);
  // The options `hunks` were fetched with: a hunk action finds the hunk again with them (#469).
  const [fetchedOptions, setFetchedOptions] = useState<DiffOptions | null>(null);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);

  const { settings } = useDiffSettingsContext();
  const wholeFile = useWholeFile(settings.contextLines);
  const diffOptions: DiffOptions = {
    ignoreWhitespace: settings.ignoreWhitespace,
    contextLines: wholeFile.contextLines,
    algorithm: settings.algorithm,
  };
  const hunkActions =
    patch === undefined
      ? undefined
      : (hunk: DiffHunk): HunkHeaderAction[] => {
          const ref: CommitHunk = {
            oldStart: hunk.oldStart,
            newStart: hunk.newStart,
            contextLines: fetchedOptions?.contextLines ?? wholeFile.contextLines,
            algorithm: fetchedOptions?.algorithm ?? settings.algorithm,
          };
          const reason = settings.ignoreWhitespace ? IGNORE_WHITESPACE_REASON : patch.blockedReason;
          const blocked = { disabled: reason !== null, title: reason ?? HUNK_STAGE_HINT };
          return [
            { label: "Revert hunk", run: (event) => patch.onRevertHunk(ref, event.shiftKey), ...blocked },
            { label: "Apply hunk", run: (event) => patch.onApplyHunk(ref, event.shiftKey), ...blocked },
          ];
        };
  const loadDiff = () => {
    let ignore = false;
    const requested = diffOptions;
    const request = fetchDiff(requested);
    request
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
    if (mode !== "diff" || collapsed || showImage) return;
    return loadDiff();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [diffKey, path, oldPath, mode, collapsed, showImage, settings.ignoreWhitespace, wholeFile.contextLines, settings.algorithm]);

  const loadBlame = () => {
    let ignore = false;
    client
      .getBlame(repoPath, blameAt, path)
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
  }, [repoPath, client, blameAt, path, mode]);

  return (
    <li className={styles.fileSection}>
      <div
        className={styles.fileSectionHeader}
        onContextMenu={
          !hasMenu
            ? undefined
            : (event) => {
                event.preventDefault();
                setMenu({ ...menuAnchor(event, event.currentTarget), returnFocusTo: actionsRef.current ?? undefined });
              }
        }
        // The Menu key or Shift+F10 on any header control (collapse, Blame, History, "…") opens the
        // file menu at the header. Every header menu (this, or a right-click on the non-focusable
        // header) returns focus to the header's "…" when it closes from the keyboard or an item.
        onKeyDown={
          !hasMenu
            ? undefined
            : (event) => {
                if (!isMenuKey(event)) return;
                event.preventDefault();
                setMenu({ ...rectAnchor(event.currentTarget), returnFocusTo: actionsRef.current ?? undefined });
              }
        }
      >
        <CollapseToggle collapsed={collapsed} path={path} onToggle={onToggleCollapse} />
        <span className={styles.path}>{path}</span>
        {oldPath != null && <span className={styles.renamedFrom}>Renamed from {oldPath}</span>}
        {lineStats !== undefined &&
          (lineStats.binary ? (
            <span className={styles.renamedFrom}>Binary</span>
          ) : (
            <LineStatsBadge insertions={lineStats.insertions} deletions={lineStats.deletions} />
          ))}
        <div className={styles.rowActions}>
          <button type="button" onClick={() => setMode("blame")}>
            Blame
          </button>
          {onOpenFileHistory !== undefined && (
            <button type="button" aria-label={`History of ${path}`} onClick={() => onOpenFileHistory(path)}>
              History
            </button>
          )}
          {onOpenLineHistory !== undefined && (
            <LineHistoryButton
              container={bodyRef}
              enabled={!collapsed}
              path={path}
              revision={blameAt}
              onOpen={onOpenLineHistory}
            />
          )}
          {onOpenFolderHistory !== undefined && <FolderHistoryButton path={path} onOpen={onOpenFolderHistory} />}
          {hasMenu && (
            <button
              type="button"
              className={styles.stageToggle}
              ref={actionsRef}
              aria-label={`Actions for ${path}`}
              aria-haspopup="menu"
              aria-expanded={menu !== null}
              onClick={(event) => {
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
        </div>
      </div>
      {!collapsed && (
        <div className={styles.fileSectionBody} ref={bodyRef}>
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
              <button onClick={() => setMode("diff")}>Back to diff</button>
            </>
          ) : showImage && previewSource !== undefined ? (
            <>
              {isSvgPath(path) && <ImageTextToggle asText={false} onToggle={() => setImageAsText(true)} />}
              <ImageDiff client={client} repoPath={repoPath} path={path} source={previewSource} refreshKey={diffKey} />
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
              {isImage && <ImageTextToggle asText onToggle={() => setImageAsText(false)} />}
              <DiffView hunks={hunks} path={path} wholeFile={wholeFile} hunkActions={hunkActions} />
            </>
          )}
        </div>
      )}
      {menu !== null && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          items={commitFileMenuItems(onPickFileAction, {
            tools: tools !== null,
            patch: patch === undefined ? null : { blockedReason: patch.blockedReason },
          })}
          onClose={() => setMenu(null)}
        />
      )}
    </li>
  );
}
