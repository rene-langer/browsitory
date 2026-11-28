import { useCallback, useContext, useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { Blame, DiffHunk, DiffOptions, DiffSource, HunkOptions, RepoClient, StatusEntry } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { imageMimeType, isSvgPath } from "../lib/imageTypes";
import { OpenPreferencesContext } from "../state/openPreferences";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useWholeFile } from "../state/useWholeFile";
import { useDiffSizeLimit } from "../state/useDiffSizeLimit";
import { useWorkspaceActions } from "../state/useWorkspaceActions";
import { BlameUnavailableNote, BlameView } from "./BlameView";
import { blameFailure, EMPTY_BLAME, isBlameRefusal, type BlameFailure } from "../lib/blame";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import styles from "./DiffPane.module.css";
import { DiffTooLarge, DiffView } from "./DiffView";
import { FolderHistoryButton } from "./FolderHistoryButton";
import { ImageDiff, ImageTextToggle } from "./ImageDiff";
import { InlineError } from "./primitives/InlineError";
import { isUntracked, statusLetter } from "../lib/statusLetter";
import { useMarkResolved } from "./useMarkResolved";
import { useNearView } from "./useNearView";

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
 * The working tree's diff viewer (#639): a file's header (path, kind, Blame, History) and its
 * diff with hunk and line actions, its blame, its conflict pane or its image preview. The parent
 * keys it by file, so switching files starts it afresh. The list view shows one, the current
 * file's; the stacked view one per file (`stacked`, #793).
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
  current = true,
  stacked = false,
  stackRoot = null,
  onStackFocus,
  onStackSection,
}: {
  /** The palette's "Wrap lines" and "Show whole file" act on this diff (see `DiffView`). */
  current?: boolean;
  /**
   * In the stacked view (#793): the body renders, and the diff loads, once the viewer is near
   * what `stackRoot` (the scrolling stack) shows.
   */
  stacked?: boolean;
  stackRoot?: HTMLElement | null;
  /** Stacked: focus moved into the viewer of `entry`. */
  onStackFocus?: (entry: StatusEntry) => void;
  /** Stacked: the section of `entry`'s viewer, `null` once it goes. Kept the same across renders. */
  onStackSection?: (entry: StatusEntry, element: HTMLElement | null) => void;
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
  // In the stack, only what is near the view is rendered and fetched; once rendered, a diff stays
  // (no jumps while scrolling back), and a refresh refetches it when it next comes near.
  const [sectionElement, setSectionElement] = useState<HTMLElement | null>(null);
  const nearView = useNearView(sectionElement, stackRoot, stacked);
  const [seen, setSeen] = useState(!stacked);
  if (nearView && !seen) setSeen(true);
  const active = !stacked || nearView;
  const [mode, setMode] = useState<"diff" | "blame">("diff");
  const [hunks, setHunks] = useState<DiffHunk[] | null>(null);
  // The options `hunks` were fetched with. Hunk/line actions must use these, not the current
  // settings: after a settings change the shown hunks are still the old ones until the refetch
  // lands, and their (oldStart, newStart) only mean something under the options they came from.
  const [fetchedOptions, setFetchedOptions] = useState<DiffOptions | null>(null);
  const [blame, setBlame] = useState<Blame>(EMPTY_BLAME);
  const [error, setError] = useState<DescribedError | BlameFailure | null>(null);
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
  const sizeLimit = useDiffSizeLimit(`${entry.staged ? "staged" : "unstaged"}\0${renamedFrom ?? ""}\0${entry.path}`);
  const diffOptions: DiffOptions = {
    ignoreWhitespace: settings.ignoreWhitespace,
    contextLines: wholeFile.contextLines,
    algorithm: settings.algorithm,
    ...sizeLimit.options,
  };

  // What the shown hunks were fetched for: a stacked diff coming back into view refetches only
  // when this changed meanwhile.
  const loadSignature = JSON.stringify([diffVersion, diffOptions]);
  const loadedSignature = useRef<string | null>(null);

  // Pulled out of the effect (returning its own cleanup) so the failure banner's Retry button can
  // re-run the same fetch. `diffVersion`, not `status`, is the staleness signal: a status array
  // that is new by reference but not the product of a refresh must not refetch anything.
  const loadDiff = () => {
    let ignore = false;
    const requested = diffOptions;
    const signature = loadSignature;
    (renamedFrom === null
      ? client.getWorkingDiff(repoPath, entry.path, entry.staged, requested)
      : client.getWorkingDiff(repoPath, entry.path, entry.staged, requested, renamedFrom)
    )
      .then((next) => {
        if (!ignore && wholeFile.accept(next, requested.contextLines)) {
          setHunks(next);
          setFetchedOptions(requested);
          setError(null);
          sizeLimit.loaded();
          loadedSignature.current = signature;
        }
      })
      .catch((err: unknown) => {
        if (ignore) return;
        if (sizeLimit.refused(err)) setError(null);
        else setError(describeError(err));
      });
    return () => {
      ignore = true;
    };
  };

  useEffect(() => {
    if (mode !== "diff" || isConflicted || showImage || contentsHidden || !active) return;
    if (stacked && loadedSignature.current === loadSignature) return;
    return loadDiff();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoPath, client, entry.path, entry.staged, renamedFrom, isConflicted, showImage, mode, diffVersion, contentsHidden, settings.ignoreWhitespace, wholeFile.contextLines, settings.algorithm, sizeLimit.options, active]);

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
          setBlame(next);
          setError(null);
        }
      })
      .catch((err: unknown) => {
        if (!ignore) setError(blameFailure(err));
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
  const setSection = useCallback(
    (element: HTMLElement | null) => {
      sectionRef.current = element;
      setSectionElement(element);
      onStackSection?.(entry, element);
    },
    // The parent keys the viewer by file, so `entry` names the same file for its whole life.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [onStackSection],
  );
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
      ref={setSection}
      className={styles.viewer}
      aria-label={`Changes in ${entry.path} (${side})`}
      aria-keyshortcuts="j k"
      tabIndex={-1}
      onKeyDown={handleKeyDown}
      onFocus={onStackFocus === undefined ? undefined : () => onStackFocus(entry)}
    >
      <div className={styles.viewerHeader}>
        {/* In the stack, a heading per file lets a screen reader move between them. */}
        <span className={styles.path} role={stacked ? "heading" : undefined} aria-level={stacked ? 3 : undefined}>
          {entry.path}
        </span>
        {/* "Untracked" says it all; the rest name the side plainly (#1027). */}
        <span className={styles.viewerKind}>
          {kind === "Untracked" ? kind : `${kind}, ${entry.staged ? "staged" : "not staged"}`}
        </span>
        {renamedFrom !== null && <span className={styles.renamedFrom}>Renamed from {renamedFrom}</span>}
        <div className={styles.rowActions}>
          {/* While blame shows, the header's way back replaces Blame (#986). */}
          {mode === "blame" ? (
            <button type="button" onClick={() => setMode("diff")}>
              Back to diff
            </button>
          ) : (
            <button type="button" onClick={() => setMode("blame")}>
              Blame
            </button>
          )}
          {onOpenFileHistory !== undefined && !isUntracked(entry) && (
            <button type="button" aria-label={`History of ${entry.path}`} onClick={() => onOpenFileHistory(entry.path)}>
              History
            </button>
          )}
          {onOpenFolderHistory !== undefined && <FolderHistoryButton path={entry.path} onOpen={onOpenFolderHistory} />}
        </div>
      </div>
      <div className={styles.viewerBody} data-file-body="">
        {!seen ? (
          <p className={styles.stackPlaceholder}>The diff loads when it scrolls into view.</p>
        ) : mode === "blame" ? (
          isBlameRefusal(error) ? (
            <BlameUnavailableNote message={error.message} />
          ) : error !== null ? (
            <InlineError message={error.message} hint={error.hint} onDismiss={() => setError(null)} onRetry={loadBlame} />
          ) : (
            <BlameView blame={blame} path={entry.path} onSelectRow={onSelectRow} />
          )
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
        ) : sizeLimit.tooLarge !== null ? (
          <DiffTooLarge message={sizeLimit.tooLarge} onLoadAnyway={sizeLimit.loadAnyway} />
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
              current={current}
              focusKey={focusKey === 0 ? undefined : focusKey}
            />
          </>
        )}
      </div>
      {confirmDialog}
    </section>
  );
}
