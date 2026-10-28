import { MoreHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { BlameLine, DiffHunk, DiffOptions, DiffSource, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import type { SelectedRow } from "../state/useAppState";
import type { ExternalToolsApi } from "../state/useExternalTools";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { BlameView } from "./BlameView";
import { CollapseToggle } from "./CollapseToggle";
import styles from "./DiffPane.module.css";
import { DiffView } from "./DiffView";
import { ImageDiff, ImageTextToggle } from "./ImageDiff";
import { imageMimeType, isSvgPath } from "../lib/imageTypes";
import { commitFileMenuItems, type FileAction } from "./FileActionsMenu";
import { ContextMenu } from "./primitives/ContextMenu";
import { isMenuKey, menuAnchor, rectAnchor } from "../lib/menuKeys";
import { InlineError } from "./primitives/InlineError";

/**
 * One file's diff in a read-only view (a commit or a compare), with Blame and History.
 * `fetchDiff` runs again whenever `diffKey`, the path or the diff settings change. With
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
  externalTools,
  toolSource,
  previewSource,
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
  externalTools?: ExternalToolsApi;
  /** The two versions "Open in diff tool" compares. */
  toolSource?: DiffSource;
  /** The two versions an image preview shows (the same ones the diff compares). */
  previewSource?: DiffSource;
}) {
  const [mode, setMode] = useState<"diff" | "blame">("diff");
  const isImage = previewSource !== undefined && imageMimeType(path) !== null;
  const [imageAsText, setImageAsText] = useState(false);
  const showImage = isImage && !imageAsText;
  const [menu, setMenu] = useState<{ x: number; y: number; returnFocusTo?: HTMLElement } | null>(null);
  const actionsRef = useRef<HTMLButtonElement>(null);
  const tools = externalTools !== undefined && toolSource !== undefined ? { externalTools, toolSource } : null;
  const onPickFileAction = (action: FileAction) => {
    if (tools === null) return;
    if (action.kind === "openInEditor") tools.externalTools.openInEditor(path, null);
    else if (action.kind === "reveal") tools.externalTools.reveal(path);
    else if (action.kind === "diffTool") tools.externalTools.openDiffTool(path, tools.toolSource);
  };
  const [hunks, setHunks] = useState<DiffHunk[] | null>(null);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);

  const { settings } = useDiffSettingsContext();
  const diffOptions = { ignoreWhitespace: settings.ignoreWhitespace, contextLines: settings.contextLines };
  const loadDiff = () => {
    let ignore = false;
    const request = fetchDiff(diffOptions);
    request
      .then((next) => {
        if (!ignore) {
          setHunks(next);
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
  }, [diffKey, path, oldPath, mode, collapsed, showImage, settings.ignoreWhitespace, settings.contextLines]);

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
          tools === null
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
          tools === null
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
        <div className={styles.rowActions}>
          <button type="button" onClick={() => setMode("blame")}>
            Blame
          </button>
          {onOpenFileHistory !== undefined && (
            <button type="button" aria-label={`History of ${path}`} onClick={() => onOpenFileHistory(path)}>
              History
            </button>
          )}
          {tools !== null && (
            <button
              type="button"
              className={styles.stageToggle}
              ref={actionsRef}
              aria-label={`Actions for ${path}`}
              aria-haspopup="menu"
              onClick={(event) => {
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
        </div>
      </div>
      {!collapsed && (
        <div className={styles.fileSectionBody}>
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
              <DiffView hunks={hunks} path={path} />
            </>
          )}
        </div>
      )}
      {menu !== null && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          returnFocusTo={menu.returnFocusTo}
          items={commitFileMenuItems(onPickFileAction)}
          onClose={() => setMenu(null)}
        />
      )}
    </li>
  );
}
