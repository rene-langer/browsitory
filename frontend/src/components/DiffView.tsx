import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from "react";
import { type DiffHunk, type DiffLine, type DiffLineOrigin } from "../ipc/RepoClient";
import { EMPTY_SELECTION, reduceSelection, type LineSelection, type SelectionAction } from "../lib/lineSelection";
import { useDiffSettingsContext } from "../state/useDiffSettings";
import { useRegisterDiffView } from "../state/useCurrentDiff";
import { Toolbar } from "./primitives/Toolbar";
import { useDiffViewOptions } from "../state/useDiffViewOptions";
import { wordDiff, type Segment } from "../lib/wordDiff";
import { useHunkTokens } from "../lib/highlight";
import { languageForPath } from "../lib/languageForPath";
import { wrapLines } from "../lib/wordWrap";
import { logFailure } from "../lib/logger";
import { DiffLineContent } from "./DiffLineContent";
import styles from "./DiffView.module.css";

/**
 * Pairs up a hunk's Remove/Add lines so word-level diffing and the split view can line them up.
 *
 * A "replace block" is one or more consecutive `Remove` lines immediately followed by one or more
 * consecutive `Add` lines; pair them 1:1 in encounter order (first Remove with first Add, etc.).
 * Leftover lines on either side (an uneven count, or a Remove/Add with no counterpart at all)
 * render unpaired — `pairedWith` is `null`.
 */
function pairReplaceBlocks(lines: DiffLine[]): Array<{ line: DiffLine; pairedWith: DiffLine | null }> {
  const result: Array<{ line: DiffLine; pairedWith: DiffLine | null }> = [];
  let i = 0;
  while (i < lines.length) {
    if (lines[i].origin !== "Remove") {
      result.push({ line: lines[i], pairedWith: null });
      i++;
      continue;
    }
    const removeStart = i;
    while (i < lines.length && lines[i].origin === "Remove") i++;
    const removes = lines.slice(removeStart, i);
    const addStart = i;
    while (i < lines.length && lines[i].origin === "Add") i++;
    const adds = lines.slice(addStart, i);
    const pairCount = Math.min(removes.length, adds.length);
    for (let k = 0; k < pairCount; k++) {
      result.push({ line: removes[k], pairedWith: adds[k] });
    }
    for (let k = pairCount; k < removes.length; k++) result.push({ line: removes[k], pairedWith: null });
    for (let k = 0; k < pairCount; k++) result.push({ line: adds[k], pairedWith: removes[k] });
    for (let k = pairCount; k < adds.length; k++) result.push({ line: adds[k], pairedWith: null });
  }
  return result;
}

/**
 * Word-level segments for every paired Remove/Add line in `hunks`, keyed by the `DiffLine` object
 * itself. Computed once per pair (the old code ran `wordDiff` twice per pair, once from each side)
 * and memoized on the `hunks` array by `DiffView`, so re-renders that don't refetch — toggling
 * split view, moving the active hunk, arming a discard — never redo the LCS work.
 */
function computeWordSegments(hunks: DiffHunk[] | null): Map<DiffLine, Segment[]> {
  const segments = new Map<DiffLine, Segment[]>();
  for (const hunk of hunks ?? []) {
    for (const { line, pairedWith } of pairReplaceBlocks(hunk.lines)) {
      if (pairedWith === null || line.origin !== "Remove") continue;
      const { oldSegments, newSegments } = wordDiff(line.content, pairedWith.content);
      segments.set(line, oldSegments);
      segments.set(pairedWith, newSegments);
    }
  }
  return segments;
}

// Highlight failures are never user-facing: log and fall back to plain text.
function logHighlightError(error: unknown): void {
  logFailure("Syntax highlighting failed", error);
}

type SplitRow =
  | { kind: "context"; line: DiffLine }
  | { kind: "pair"; oldLine: DiffLine; newLine: DiffLine }
  | { kind: "old-only"; line: DiffLine }
  | { kind: "new-only"; line: DiffLine };

/**
 * Groups a hunk's lines into rows for the split (side-by-side) layout: a `Context` line becomes
 * its own full-width row, a paired Remove/Add (see `pairReplaceBlocks`) becomes one two-column
 * row, and a leftover unpaired Remove or Add becomes a row with only its own column filled.
 */
function buildSplitRows(lines: DiffLine[]): SplitRow[] {
  const rows: SplitRow[] = [];
  let i = 0;
  while (i < lines.length) {
    if (lines[i].origin === "Context") {
      rows.push({ kind: "context", line: lines[i] });
      i++;
      continue;
    }
    if (lines[i].origin !== "Remove") {
      rows.push({ kind: "new-only", line: lines[i] });
      i++;
      continue;
    }
    const removeStart = i;
    while (i < lines.length && lines[i].origin === "Remove") i++;
    const removes = lines.slice(removeStart, i);
    const addStart = i;
    while (i < lines.length && lines[i].origin === "Add") i++;
    const adds = lines.slice(addStart, i);
    const pairCount = Math.min(removes.length, adds.length);
    for (let k = 0; k < pairCount; k++) {
      rows.push({ kind: "pair", oldLine: removes[k], newLine: adds[k] });
    }
    for (let k = pairCount; k < removes.length; k++) rows.push({ kind: "old-only", line: removes[k] });
    for (let k = pairCount; k < adds.length; k++) rows.push({ kind: "new-only", line: adds[k] });
  }
  return rows;
}

// `expectedLines` is every line's content of the hunk as displayed; the backend refuses the
// action (and the diff refetches) if the hunk no longer reads exactly like that.
type LineCallback = (oldStart: number, newStart: number, lineIndices: number[], expectedLines: string[]) => void;

const shownLines = (hunk: DiffHunk) => hunk.lines.map((l) => l.content);

/**
 * "Show whole file" (#444), for a parent that fetches the diff itself: `on` is the toggle,
 * `tooLongLines` the line count of a whole-file diff that was too long to show (the parent then
 * shows the normal context instead).
 */
export interface WholeFileToggle {
  on: boolean;
  onToggle: () => void;
  tooLongLines: number | null;
}

const lineCount = new Intl.NumberFormat("en-US");

/**
 * `hunks === null` means "not loaded yet" (renders a loading placeholder); `[]` is a real,
 * resolved empty diff (binary file, mode-only change, or no changes).
 *
 * Hunk actions are not tab stops: the diff container is one tab stop and handles `[`/`]`
 * (previous/next hunk), `s` (stage or unstage the active hunk) and `d` (discard it: one press, as
 * Undo brings it back and the toast offers Undo, #662).
 * The per-hunk buttons stay clickable but are removed from the tab order.
 *
 * The view options (Split, Wrap lines, whitespace, context, whole file, highlighting) are the
 * pane's: one `DiffOptionsToolbar` per pane sets them for every diff in it (#640). Lines wrap as
 * the `wordWrap` setting says (`lib/wordWrap.ts`). `wholeFile` is the file's "Show whole file"
 * state, for the hint shown when the file is too long for it.
 *
 * With line callbacks, Add/Remove lines are selectable (click, shift-click, ctrl/cmd-click, drag,
 * ArrowUp/ArrowDown with optional Shift) within one hunk; `s`/`d` and the hunk header buttons
 * then act on the selected lines instead of the whole hunk.
 */
/** A button in a hunk's header (`DiffView`'s `hunkActions`). */
export interface HunkHeaderAction {
  label: string;
  run: () => void;
  disabled?: boolean;
  /** Why it is disabled, or what it does. */
  title?: string;
}

export function DiffView({
  hunks,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onStageLines,
  onUnstageLines,
  onDiscardLines,
  partialActionsDisabled = false,
  path,
  wholeFile,
  hunkActions,
  current = false,
  focusKey,
}: {
  hunks: DiffHunk[] | null;
  onStageHunk?: (oldStart: number, newStart: number) => void;
  onUnstageHunk?: (oldStart: number, newStart: number) => void;
  onDiscardHunk?: (oldStart: number, newStart: number) => void;
  onStageLines?: LineCallback;
  onUnstageLines?: LineCallback;
  onDiscardLines?: LineCallback;
  partialActionsDisabled?: boolean;
  path?: string;
  wholeFile?: WholeFileToggle;
  /** Extra buttons in each hunk's header (a commit's Revert hunk / Apply hunk, #448). */
  hunkActions?: (hunk: DiffHunk) => HunkHeaderAction[];
  /**
   * The working tree's current file: the palette's "Wrap lines" and "Show whole file" then act on
   * this diff. Clicking or focusing a diff makes it current too.
   */
  current?: boolean;
  /**
   * A change moves focus into the diff (once its hunks are loaded): the working tree's keys
   * that go to another file from inside the diff, or Enter in its file list.
   */
  focusKey?: number;
}) {
  const [previousHunks, setPreviousHunks] = useState(hunks);
  const [activeIndex, setActiveIndex] = useState(0);
  // Split view is the pane's (#640): its one `DiffOptionsToolbar` sets it for every diff.
  const { split } = useDiffViewOptions();
  const [selection, setSelection] = useState<LineSelection>(EMPTY_SELECTION);
  const dragging = useRef<{ hunkIndex: number; line: number } | null>(null);
  const hunkRefs = useRef<Record<number, HTMLDivElement | null>>({});
  const diffRef = useRef<HTMLDivElement | null>(null);
  const focusedKey = useRef<number | undefined>(undefined);
  const loaded = hunks !== null;
  useEffect(() => {
    if (focusKey === undefined || focusKey === focusedKey.current || !loaded) return;
    focusedKey.current = focusKey;
    diffRef.current?.focus({ preventScroll: true });
  }, [focusKey, loaded]);
  const wordSegments = useMemo(() => computeWordSegments(hunks), [hunks]);
  const { settings, update } = useDiffSettingsContext();
  const tokens = useHunkTokens(hunks, path ? languageForPath(path) : null, settings.syntaxHighlight, logHighlightError);
  const wrapBySetting = useMemo(
    () => wrapLines(settings.wordWrap, { split, path, hunks }),
    [settings.wordWrap, split, path, hunks],
  );
  const wrap = wrapBySetting;
  const wholeFileOn = wholeFile?.on === true;
  // The palette's "Wrap lines" flips what this diff shows, for every diff: it sets the setting.
  const claimCurrent = useRegisterDiffView(
    path ?? null,
    { toggleWrap: () => update({ wordWrap: wrap ? "Off" : "On" }), toggleWholeFile: wholeFile?.onToggle ?? null },
    current,
  );
  const claimHandlers = { onPointerDownCapture: claimCurrent, onFocusCapture: claimCurrent };

  // The active hunk and the line selection are indexes into `hunks`, so a new hunk list resets
  // them. Reset-during-render (as `TagPanel` does) rather than an effect, which would cost a
  // second render per refetch.
  if (previousHunks !== hunks) {
    setPreviousHunks(hunks);
    setActiveIndex(0);
    setSelection(EMPTY_SELECTION);
  }

  // A drag ends wherever the button is released, even outside the diff.
  useEffect(() => {
    const endDrag = () => {
      dragging.current = null;
    };
    window.addEventListener("mouseup", endDrag);
    return () => window.removeEventListener("mouseup", endDrag);
  }, []);

  if (hunks === null) {
    return (
      <p role="status" className={styles.placeholder}>
        Loading diff…
      </p>
    );
  }

  const hint = (
    <>
      {partialActionsDisabled && <p className={styles.hint}>Turn off Ignore whitespace to stage partially.</p>}
      {wholeFileOn && wholeFile?.tooLongLines != null && (
        <p className={styles.hint}>
          This file is too long to show whole ({lineCount.format(wholeFile.tooLongLines)} lines); showing the
          changes with context instead.
        </p>
      )}
    </>
  );

  // An empty diff still shows its hints: ignoring whitespace can hide every hunk.
  if (hunks.length === 0) {
    return (
      <>
        {hint}
        <p className={styles.placeholder}>No text differences (binary file or mode-only change)</p>
      </>
    );
  }

  const lineActions =
    !partialActionsDisabled &&
    (onStageLines !== undefined || onUnstageLines !== undefined || onDiscardLines !== undefined);
  const hasActions =
    !partialActionsDisabled &&
    (onStageHunk !== undefined || onUnstageHunk !== undefined || onDiscardHunk !== undefined || lineActions);
  const active = Math.min(activeIndex, hunks.length - 1);
  const selectedCount = selection.selected.size;

  const apply = (action: SelectionAction) => {
    setSelection((prev) => reduceSelection(prev, action, hunks));
  };

  const selectedIn = (hunkIndex: number) =>
    selection.hunkIndex === hunkIndex ? [...selection.selected].sort((a, b) => a - b) : [];

  const moveTo = (index: number) => {
    setActiveIndex(index);
    hunkRefs.current[index]?.scrollIntoView?.({ block: "nearest" });
  };

  // Handlers only go on Add/Remove rows, so `shiftClick`/`dragTo` always target a selectable line
  // and the reducer's cursor never lands on a Context line.
  const lineProps = (hunkIndex: number, lineIndex: number) => {
    const base = { "data-hunk-index": hunkIndex, "data-line-index": lineIndex };
    if (!lineActions) return base;
    return {
      ...base,
      "aria-selected": selection.hunkIndex === hunkIndex && selection.selected.has(lineIndex),
      onMouseDown: (event: MouseEvent<HTMLDivElement>) => {
        if (event.button !== 0) return;
        dragging.current = { hunkIndex, line: lineIndex };
        // A drag ending on another row fires `click` on a common ancestor, not this row, so the
        // hunk must become active here for `s`/`d` to target the dragged lines.
        setActiveIndex(hunkIndex);
        // Anchor a plain-click drag right away; the following `click` re-applies it harmlessly.
        if (!event.shiftKey && !event.ctrlKey && !event.metaKey) apply({ type: "click", hunkIndex, line: lineIndex });
      },
      onMouseEnter: () => {
        if (dragging.current?.hunkIndex === hunkIndex) apply({ type: "dragTo", hunkIndex, line: lineIndex });
      },
      onClick: (event: MouseEvent<HTMLDivElement>) => {
        setActiveIndex(hunkIndex);
        apply(
          event.shiftKey
            ? { type: "shiftClick", hunkIndex, line: lineIndex }
            : event.ctrlKey || event.metaKey
              ? { type: "toggle", hunkIndex, line: lineIndex }
              : { type: "click", hunkIndex, line: lineIndex },
        );
      },
    };
  };

  const lineClass = (hunkIndex: number, lineIndex: number) => {
    if (!lineActions || selection.hunkIndex !== hunkIndex) return "";
    return `${selection.selected.has(lineIndex) ? ` ${styles.lineSelected}` : ""}${selection.cursor === lineIndex ? ` ${styles.lineCursor}` : ""}`;
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget || event.ctrlKey || event.metaKey || event.altKey) return;
    const hunk = hunks[active];
    const lines = selectedIn(active);
    // With lines selected, `s`/`d` act on those lines only. A missing line callback means "no
    // such action here", never "fall back to the whole hunk" — that hunk was not what the user
    // picked (and for discard, it may not even be this section's diff side).
    const byLines = lines.length > 0;
    const stageLines = onStageLines ?? onUnstageLines;
    const discard = byLines
      ? onDiscardLines && (() => onDiscardLines(hunk.oldStart, hunk.newStart, lines, shownLines(hunk)))
      : onDiscardHunk && (() => onDiscardHunk(hunk.oldStart, hunk.newStart));
    let handled = true;
    if (event.key === "]") {
      moveTo(Math.min(active + 1, hunks.length - 1));
      apply({ type: "clear" });
    } else if (event.key === "[") {
      moveTo(Math.max(active - 1, 0));
      apply({ type: "clear" });
    } else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && lineActions) {
      const delta = event.key === "ArrowDown" ? 1 : -1;
      if (selection.hunkIndex !== active) apply({ type: "focusHunk", hunkIndex: active });
      apply(event.shiftKey ? { type: "extend", delta } : { type: "moveCursor", delta });
    } else if (event.key === "s" && byLines) {
      if (stageLines !== undefined) stageLines(hunk.oldStart, hunk.newStart, lines, shownLines(hunk));
      else handled = false;
    } else if (event.key === "s") {
      if (onStageHunk !== undefined) onStageHunk(hunk.oldStart, hunk.newStart);
      else if (onUnstageHunk !== undefined) onUnstageHunk(hunk.oldStart, hunk.newStart);
    } else if (event.key === "d" && discard !== undefined) {
      discard();
    } else {
      handled = false;
    }
    if (handled) {
      // The enclosing file list owns j/k/s/arrows for file navigation; hunk keys must not leak to it.
      event.preventDefault();
      event.stopPropagation();
    }
  };

  return (
    <>
      {hint}
      <div
        ref={diffRef}
        className={wrap ? styles.diff : `${styles.diff} ${styles.nowrap}`}
        data-wrap={wrap}
        {...claimHandlers}
        {...(hasActions
          ? {
              tabIndex: 0,
              role: "group",
              "aria-label": "Diff hunks",
              "aria-keyshortcuts": "[ ] s d ArrowUp ArrowDown Shift+ArrowUp Shift+ArrowDown",
              onKeyDown: handleKeyDown,
            }
          : {})}
      >
        {lineActions && (
          <span role="status" aria-live="polite" className={styles.srOnly}>
            {selectedCount > 0 ? `${selectedCount} ${selectedCount === 1 ? "line" : "lines"} selected` : ""}
          </span>
        )}
        {hunks.map((hunk, hunkIndex) => {
          let oldLine = hunk.oldStart;
          let newLine = hunk.newStart;
          const splitRows = split ? buildSplitRows(hunk.lines) : null;
          const indexOf = split ? new Map(hunk.lines.map((l, i) => [l, i])) : null;
          const lines = selectedIn(hunkIndex);
          const byLines = lines.length > 0;
          const n = lines.length;
          const { oldStart, newStart } = hunk;
          const stage = byLines
            ? onStageLines && { label: `Stage lines (${n})`, run: () => onStageLines(oldStart, newStart, lines, shownLines(hunk)) }
            : onStageHunk && { label: "Stage hunk", run: () => onStageHunk(oldStart, newStart) };
          const unstage = byLines
            ? onUnstageLines && { label: `Unstage lines (${n})`, run: () => onUnstageLines(oldStart, newStart, lines, shownLines(hunk)) }
            : onUnstageHunk && { label: "Unstage hunk", run: () => onUnstageHunk(oldStart, newStart) };
          const discard = byLines
            ? onDiscardLines && { label: `Discard lines (${n})`, run: () => onDiscardLines(oldStart, newStart, lines, shownLines(hunk)) }
            : onDiscardHunk && { label: "Discard hunk", run: () => onDiscardHunk(oldStart, newStart) };
          // Split-view cells: the line's index in `hunk.lines` (the selection's coordinate space).
          const cell = (line: DiffLine) => indexOf!.get(line)!;
          return (
            <div
              key={hunkIndex}
              ref={(el) => {
                hunkRefs.current[hunkIndex] = el;
              }}
              className={hasActions && hunkIndex === active ? styles.activeHunk : undefined}
            >
              <div className={styles.hunkHeader}>
                <span className={styles.hunkRange}>
                  @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},{hunk.newLines} @@
                </span>
                {hunkActions !== undefined && (
                  <Toolbar aria-label="Hunk actions">
                    {hunkActions(hunk).map((action) => (
                      <button
                        key={action.label}
                        type="button"
                        disabled={action.disabled}
                        title={action.title}
                        onClick={action.run}
                      >
                        {action.label}
                      </button>
                    ))}
                  </Toolbar>
                )}
                {hasActions && (
                  <Toolbar>
                    {stage && (
                      <button tabIndex={-1} title={`${stage.label} (s)`} onClick={stage.run}>
                        {stage.label}
                      </button>
                    )}
                    {unstage && (
                      <button tabIndex={-1} title={`${unstage.label} (s)`} onClick={unstage.run}>
                        {unstage.label}
                      </button>
                    )}
                    {discard && (
                      <button
                        tabIndex={-1}
                        data-tone="danger"
                        title={`${discard.label} (d). Undo brings it back.`}
                        onClick={discard.run}
                      >
                        {discard.label}
                      </button>
                    )}
                  </Toolbar>
                )}
              </div>
              {split ? (
                <div className={`${styles.lines} ${styles.splitLines}`}>
                  {splitRows!.map((row, rowIndex) => {
                    if (row.kind === "context") {
                      const oldNo = oldLine++;
                      const newNo = newLine++;
                      return (
                        <div
                          key={rowIndex}
                          data-line-number={newNo}
                          className={`${styles.line} ${styles.lineContext} diff-line diff-line-context`}
                        >
                          <span className={styles.gutter} aria-hidden="true">
                            {oldNo}
                          </span>
                          <span className={styles.gutter} aria-hidden="true">
                            {newNo}
                          </span>
                          <span className={styles.marker} aria-hidden="true">
                            {originPrefix("Context")}
                          </span>
                          <span className={styles.content}><DiffLineContent line={row.line} tokens={tokens?.get(row.line)} segments={wordSegments.get(row.line)} showWhitespace={settings.showWhitespace} /></span>
                        </div>
                      );
                    }
                    if (row.kind === "pair") {
                      const oldNo = oldLine++;
                      const newNo = newLine++;
                      const oldIndex = cell(row.oldLine);
                      const newIndex = cell(row.newLine);
                      return (
                        <div key={rowIndex} className={styles.splitRow}>
                          <div
                            data-diff-column="old"
                            {...lineProps(hunkIndex, oldIndex)}
                            className={`${styles.splitColumn} ${styles.line} ${styles.lineRemove} diff-line diff-line-remove${lineClass(hunkIndex, oldIndex)}`}
                          >
                            <span className={styles.gutter} aria-hidden="true">
                              {oldNo}
                            </span>
                            <span className={styles.marker} aria-hidden="true">
                              {originPrefix("Remove")}
                            </span>
                            <span className={styles.content}><DiffLineContent line={row.oldLine} tokens={tokens?.get(row.oldLine)} segments={wordSegments.get(row.oldLine)} showWhitespace={settings.showWhitespace} /></span>
                          </div>
                          <div
                            data-diff-column="new"
                            data-line-number={newNo}
                            {...lineProps(hunkIndex, newIndex)}
                            className={`${styles.splitColumn} ${styles.line} ${styles.lineAdd} diff-line diff-line-add${lineClass(hunkIndex, newIndex)}`}
                          >
                            <span className={styles.gutter} aria-hidden="true">
                              {newNo}
                            </span>
                            <span className={styles.marker} aria-hidden="true">
                              {originPrefix("Add")}
                            </span>
                            <span className={styles.content}><DiffLineContent line={row.newLine} tokens={tokens?.get(row.newLine)} segments={wordSegments.get(row.newLine)} showWhitespace={settings.showWhitespace} /></span>
                          </div>
                        </div>
                      );
                    }
                    if (row.kind === "old-only") {
                      const oldNo = oldLine++;
                      const index = cell(row.line);
                      return (
                        <div key={rowIndex} className={styles.splitRow}>
                          <div
                            data-diff-column="old"
                            {...lineProps(hunkIndex, index)}
                            className={`${styles.splitColumn} ${styles.line} ${styles.lineRemove} diff-line diff-line-remove${lineClass(hunkIndex, index)}`}
                          >
                            <span className={styles.gutter} aria-hidden="true">
                              {oldNo}
                            </span>
                            <span className={styles.marker} aria-hidden="true">
                              {originPrefix("Remove")}
                            </span>
                            <span className={styles.content}><DiffLineContent line={row.line} tokens={tokens?.get(row.line)} segments={wordSegments.get(row.line)} showWhitespace={settings.showWhitespace} /></span>
                          </div>
                          <div className={styles.splitColumn} aria-hidden="true" />
                        </div>
                      );
                    }
                    const newNo = newLine++;
                    const index = cell(row.line);
                    return (
                      <div key={rowIndex} className={styles.splitRow}>
                        <div className={styles.splitColumn} aria-hidden="true" />
                        <div
                          data-diff-column="new"
                          data-line-number={newNo}
                          {...lineProps(hunkIndex, index)}
                          className={`${styles.splitColumn} ${styles.line} ${styles.lineAdd} diff-line diff-line-add${lineClass(hunkIndex, index)}`}
                        >
                          <span className={styles.gutter} aria-hidden="true">
                            {newNo}
                          </span>
                          <span className={styles.marker} aria-hidden="true">
                            {originPrefix("Add")}
                          </span>
                          <span className={styles.content}><DiffLineContent line={row.line} tokens={tokens?.get(row.line)} segments={wordSegments.get(row.line)} showWhitespace={settings.showWhitespace} /></span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : (
                <pre className={`${styles.lines} ${styles.unifiedLines}`}>
                  {hunk.lines.map((line, lineIndex) => {
                    const oldNo = line.origin !== "Add" ? oldLine++ : null;
                    const newNo = line.origin !== "Remove" ? newLine++ : null;
                    const selectable = line.origin !== "Context";
                    return (
                      <div
                        key={lineIndex}
                        data-line-number={newNo ?? undefined}
                        {...(selectable ? lineProps(hunkIndex, lineIndex) : {})}
                        className={`${styles.line} ${styles[`line${line.origin}`]} diff-line diff-line-${line.origin.toLowerCase()}${selectable ? lineClass(hunkIndex, lineIndex) : ""}`}
                      >
                        <span className={styles.gutter} aria-hidden="true">
                          {oldNo ?? ""}
                        </span>
                        <span className={styles.gutter} aria-hidden="true">
                          {newNo ?? ""}
                        </span>
                        <span className={styles.marker} aria-hidden="true">
                          {originPrefix(line.origin)}
                        </span>
                        <span className={styles.content}><DiffLineContent line={line} tokens={tokens?.get(line)} segments={wordSegments.get(line)} showWhitespace={settings.showWhitespace} /></span>
                      </div>
                    );
                  })}
                </pre>
              )}
            </div>
          );
        })}
      </div>
    </>
  );
}

function originPrefix(origin: DiffLineOrigin): string {
  switch (origin) {
    case "Add":
      return "+";
    case "Remove":
      return "-";
    case "Context":
      return " ";
  }
}
