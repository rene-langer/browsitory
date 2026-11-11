import { useContext, useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { BlameLine, DiffHunk, DiffOptions, DiffSource, HunkOptions, RepoClient, StatusEntry } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { imageMimeType, isSvgPath } from "../lib/imageTypes";
import { OpenPreferencesContext } from "../state/openPreferences";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useWholeFile } from "../state/useWholeFile";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { BlameView } from "./BlameView";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import styles from "./DiffPane.module.css";
import { DiffView } from "./DiffView";
import { isUntracked } from "./FileActionsMenu";
import { FolderHistoryButton } from "./FolderHistoryButton";
import { ImageDiff, ImageTextToggle } from "./ImageDiff";
import { InlineError } from "./primitives/InlineError";
import { statusLetter } from "../lib/statusLetter";
import { useMarkResolved } from "./useMarkResolved";

type HunkCallback = (path: string, oldStart: number, newStart: number, hunk: HunkOptions) => void;
type LineCallback = (
  path: string,
  oldStart: number,
  newStart: number,
  hunk: HunkOptions,
  lineIndices: number[],
  expectedLines: string[],
) => void;

/**
 * The working tree's one diff viewer (#639): the current file's header (path, kind, Blame,
 * History) and its diff with hunk and line actions, its blame, its conflict pane or its image
 * preview. The parent keys it by file, so switching files starts it afresh.
 */
export function UncommittedFileViewer({
  repoPath,
  client,
  entry,
  status,
  diffVersion,
  contentsHidden,
  onShowContents,
  focusKey,
  onNavigate,
  onFirstChangedLine,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onStageLines,
  onUnstageLines,
  onDiscardLines,
}: {
  repoPath: string;
  client: RepoClient;
  entry: StatusEntry;
  status: StatusEntry[];
  // The staleness signal: the file's own version plus the pane-wide refresh generation, combined
  // by `UncommittedDiffPane` into one increasing number. Any change refetches the diff.
  diffVersion: number;
  /** "Expand untracked files" is off and this untracked file hasn't been opened (#623). */
  contentsHidden: boolean;
  onShowContents: () => void;
  /** A change moves focus into the diff once it has loaded. */
  focusKey: number;
  /** j/k inside the viewer: the next (1) or previous (-1) file in the list. */
  onNavigate: (direction: 1 | -1) => void;
  /** The first changed line of the loaded diff, for "Open in editor"; `null` before one loads. */
  onFirstChangedLine: (line: number | null) => void;
  onStageHunk: HunkCallback;
  onUnstageHunk: HunkCallback;
  onDiscardHunk: HunkCallback;
  onStageLines: LineCallback;
  onUnstageLines: LineCallback;
  onDiscardLines: LineCallback;
}) {
  const {
    onSelectRow,
    onResolveConflict,
    onResolveAddDeleteConflict,
    onOpenFileHistory,
    onOpenFolderHistory,
    onMarkResolved,
    onResolveWholeFile,
  } = useWorkspaceActions();
  const { markResolved, confirmDialog } = useMarkResolved(entry.path);
  const [mode, setMode] = useState<"diff" | "blame">("diff");
  const [hunks, setHunks] = useState<DiffHunk[] | null>(null);
  // The options `hunks` were fetched with. Hunk/line actions must use these, not the current
  // settings: after a settings change the shown hunks are still the old ones until the refetch
  // lands, and their (oldStart, newStart) only mean something under the options they came from.
  const [fetchedOptions, setFetchedOptions] = useState<DiffOptions | null>(null);
  const [blameLines, setBlameLines] = useState<BlameLine[]>([]);
  const [error, setError] = useState<DescribedError | null>(null);
  // A staged rename compares its old path in HEAD with its new path in the index (#137).
  const renamedFrom = entry.staged ? (entry.oldPath ?? null) : null;
  const diffSource: DiffSource = entry.staged ? { kind: "Staged", oldPath: renamedFrom } : { kind: "Unstaged" };
  const isConflicted = entry.kind === "Conflicted";
  // Image files show a before/after preview instead of hunks; an SVG (text) can switch to its
  // text diff. Conflicted images still go to the conflict pane.
  const isImage = imageMimeType(entry.path) !== null;
  const [imageAsText, setImageAsText] = useState(false);
  const showImage = isImage && !isConflicted && !imageAsText;
  const { settings } = useDiffSettingsContext();
  // Links the untracked-file note to the Diff preferences (#784); absent outside the app.
  const openPreferences = useContext(OpenPreferencesContext);
  // "Show whole file" fetches with whole-file context; hunk and line actions then pass it too.
  const wholeFile = useWholeFile(settings.contextLines);
  const diffOptions: DiffOptions = {
    ignoreWhitespace: settings.ignoreWhitespace,
    contextLines: wholeFile.contextLines,
    algorithm: settings.algorithm,
  };

  // Pulled out of the effect (returning its own cleanup) so the failure banner's Retry button can
  // re-run the same fetch. `diffVersion`, not `status`, is the staleness signal: a status array
  // that is new by reference but not the product of a refresh must not refetch anything.
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
        if (!ignore) setError(describeError(err));
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    if (mode !== "diff" || isConflicted || showImage || contentsHidden) return;
    return loadDiff();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, entry.path, entry.staged, renamedFrom, isConflicted, showImage, mode, diffVersion, contentsHidden, settings.ignoreWhitespace, wholeFile.contextLines, settings.algorithm]);

  // A hunk that empties the file starts at line 0, which no editor accepts.
  const firstLine = hunks?.[0]?.newStart ?? 0;
  // The latest callback, so a new one each render doesn't report again.
  const reportFirstLine = useRef(onFirstChangedLine);
  useEffect(() => {
    reportFirstLine.current = onFirstChangedLine;
  });
  useEffect(() => {
    reportFirstLine.current(firstLine > 0 ? firstLine : null);
  }, [firstLine]);

  // `status` is a dependency so that staging or committing the file while its blame is open
  // doesn't leave stale attribution on screen.
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
        if (!ignore) setError({ message: "No blame available for this file at this revision." });
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

  // Focus lands on the viewer at once; the diff takes it over once its hunks load (`DiffView`'s
  // `focusKey`), which also covers a diff with nothing to focus (an image, a conflict).
  const sectionRef = useRef<HTMLElement | null>(null);
  const focusedKey = useRef(0);
  useEffect(() => {
    if (focusKey === 0 || focusKey === focusedKey.current) return;
    focusedKey.current = focusKey;
    if (!sectionRef.current?.contains(document.activeElement)) sectionRef.current?.focus({ preventScroll: true });
  }, [focusKey]);

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
    if (event.key !== "j" && event.key !== "k") return;
    const target = event.target as HTMLElement;
    if (target.closest('input, textarea, select, [contenteditable="true"], [role="menu"], dialog') !== null) return;
    event.preventDefault();
    onNavigate(event.key === "j" ? 1 : -1);
  };

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
  const { kind } = statusLetter(entry);
  const side = entry.staged ? "staged" : "unstaged";

  return (
    <section
      ref={sectionRef}
      className={styles.viewer}
      aria-label={`Changes in ${entry.path} (${side})`}
      aria-keyshortcuts="j k"
      tabIndex={-1}
      onKeyDown={handleKeyDown}
    >
      <div className={styles.viewerHeader}>
        <span className={styles.path}>{entry.path}</span>
        <span className={styles.viewerKind}>
          {kind}, {side}
        </span>
        {renamedFrom !== null && <span className={styles.renamedFrom}>Renamed from {renamedFrom}</span>}
        <div className={styles.rowActions}>
          <button type="button" aria-pressed={mode === "blame"} onClick={() => setMode(mode === "blame" ? "diff" : "blame")}>
            Blame
          </button>
          {onOpenFileHistory !== undefined && !isUntracked(entry) && (
            <button type="button" aria-label={`History of ${entry.path}`} onClick={() => onOpenFileHistory(entry.path)}>
              History
            </button>
          )}
          {onOpenFolderHistory !== undefined && <FolderHistoryButton path={entry.path} onOpen={onOpenFolderHistory} />}
        </div>
      </div>
      <div className={styles.viewerBody} data-file-body="">
        {mode === "blame" ? (
          <>
            {error !== null ? (
              <InlineError message={error.message} hint={error.hint} onDismiss={() => setError(null)} onRetry={loadBlame} />
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
        ) : contentsHidden ? (
          <p className={styles.viewerNote}>
            An untracked file. Its contents load when you ask (Preferences: Expand untracked files is off).{" "}
            <button type="button" onClick={onShowContents}>
              Show contents
            </button>
            {openPreferences !== null && (
              <>
                {" "}
                <button type="button" onClick={() => openPreferences("diff")}>
                  Diff preferences
                </button>
              </>
            )}
          </p>
        ) : showImage ? (
          <>
            {isSvgPath(entry.path) && <ImageTextToggle asText={false} onToggle={() => setImageAsText(true)} />}
            <ImageDiff client={client} repoPath={repoPath} path={entry.path} source={diffSource} refreshKey={diffVersion} />
          </>
        ) : error !== null ? (
          <InlineError message={error.message} hint={error.hint} onDismiss={() => setError(null)} onRetry={loadDiff} />
        ) : (
          <>
            {isImage && <ImageTextToggle asText onToggle={() => setImageAsText(false)} />}
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
              current
              focusKey={focusKey === 0 ? undefined : focusKey}
            />
          </>
        )}
      </div>
      {confirmDialog}
    </section>
  );
}
