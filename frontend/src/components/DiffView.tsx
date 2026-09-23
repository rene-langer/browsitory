import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import type { DiffHunk, DiffLine, DiffLineOrigin } from "../ipc/RepoClient";
import { Toolbar } from "./primitives/Toolbar";
import { wordDiff, type Segment } from "../lib/wordDiff";
import styles from "./DiffView.module.css";

const DISCARD_DISARM_MS = 5000;

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

/** Renders a line's content as plain text, or word-level highlighted segments when paired. */
function renderContent(line: DiffLine, wordSegments: Map<DiffLine, Segment[]>): ReactNode {
  const segments = wordSegments.get(line);
  if (segments === undefined) return line.content;
  return segments.map((segment, segmentIndex) =>
    segment.changed ? <mark key={segmentIndex}>{segment.text}</mark> : <span key={segmentIndex}>{segment.text}</span>,
  );
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

/**
 * `hunks === null` means "not loaded yet" (renders a loading placeholder); `[]` is a real,
 * resolved empty diff (binary file, mode-only change, or no changes).
 *
 * Hunk actions are not tab stops: the diff container is one tab stop and handles `[`/`]`
 * (previous/next hunk), `s` (stage or unstage the active hunk) and `d` (discard; press twice).
 * The per-hunk buttons stay clickable but are removed from the tab order.
 */
export function DiffView({
  hunks,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
}: {
  hunks: DiffHunk[] | null;
  onStageHunk?: (oldStart: number, newStart: number) => void;
  onUnstageHunk?: (oldStart: number, newStart: number) => void;
  onDiscardHunk?: (oldStart: number, newStart: number) => void;
}) {
  const [previousHunks, setPreviousHunks] = useState(hunks);
  const [confirmingDiscardIndex, setConfirmingDiscardIndex] = useState<number | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);
  const [split, setSplit] = useState(false);
  const hunkRefs = useRef<Record<number, HTMLDivElement | null>>({});
  const wordSegments = useMemo(() => computeWordSegments(hunks), [hunks]);

  // A stale armed confirmation must never carry over to a different hunk list — `hunkIndex` is
  // only meaningful relative to the `hunks` array it was armed against. Reset-during-render (as
  // `TagPanel` does) rather than an effect, which would cost a second render per refetch.
  if (previousHunks !== hunks) {
    setPreviousHunks(hunks);
    setConfirmingDiscardIndex(null);
    setActiveIndex(0);
  }

  // The armed state must not linger: it disarms after a short timeout (also on blur / Escape).
  useEffect(() => {
    if (confirmingDiscardIndex === null) return;
    const timer = setTimeout(() => setConfirmingDiscardIndex(null), DISCARD_DISARM_MS);
    return () => clearTimeout(timer);
  }, [confirmingDiscardIndex]);

  if (hunks === null) {
    return (
      <p role="status" className={styles.placeholder}>
        Loading diff…
      </p>
    );
  }

  if (hunks.length === 0) {
    return <p className={styles.placeholder}>No text differences (binary file or mode-only change)</p>;
  }

  const hasActions = onStageHunk !== undefined || onUnstageHunk !== undefined || onDiscardHunk !== undefined;
  const active = Math.min(activeIndex, hunks.length - 1);

  const moveTo = (index: number) => {
    setActiveIndex(index);
    setConfirmingDiscardIndex(null);
    hunkRefs.current[index]?.scrollIntoView?.({ block: "nearest" });
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget || event.ctrlKey || event.metaKey || event.altKey) return;
    const hunk = hunks[active];
    let handled = true;
    if (event.key === "]") {
      moveTo(Math.min(active + 1, hunks.length - 1));
    } else if (event.key === "[") {
      moveTo(Math.max(active - 1, 0));
    } else if (event.key === "s") {
      if (onStageHunk !== undefined) onStageHunk(hunk.oldStart, hunk.newStart);
      else if (onUnstageHunk !== undefined) onUnstageHunk(hunk.oldStart, hunk.newStart);
    } else if (event.key === "d" && onDiscardHunk !== undefined) {
      if (confirmingDiscardIndex === active) {
        onDiscardHunk(hunk.oldStart, hunk.newStart);
        setConfirmingDiscardIndex(null);
      } else {
        setConfirmingDiscardIndex(active);
      }
    } else if (event.key === "Escape" && confirmingDiscardIndex !== null) {
      setConfirmingDiscardIndex(null);
    } else {
      handled = false;
    }
    if (handled) {
      // The enclosing file list owns j/k/s for file navigation; hunk keys must not leak to it.
      event.preventDefault();
      event.stopPropagation();
    }
  };

  return (
    <>
      <Toolbar aria-label="Diff view options">
        <button type="button" onClick={() => setSplit((prev) => !prev)}>
          {split ? "Unified view" : "Split view"}
        </button>
      </Toolbar>
      <div
        className={styles.diff}
        {...(hasActions
          ? {
              tabIndex: 0,
              role: "group",
              "aria-label": "Diff hunks",
              "aria-keyshortcuts": "[ ] s d",
              onKeyDown: handleKeyDown,
            }
          : {})}
      >
        {hunks.map((hunk, hunkIndex) => {
          let oldLine = hunk.oldStart;
          let newLine = hunk.newStart;
          const armed = confirmingDiscardIndex === hunkIndex;
          const splitRows = split ? buildSplitRows(hunk.lines) : null;
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
                {hasActions && (
                  <Toolbar>
                    {onStageHunk !== undefined && (
                      <button tabIndex={-1} onClick={() => onStageHunk(hunk.oldStart, hunk.newStart)}>
                        Stage hunk
                      </button>
                    )}
                    {onUnstageHunk !== undefined && (
                      <button tabIndex={-1} onClick={() => onUnstageHunk(hunk.oldStart, hunk.newStart)}>
                        Unstage hunk
                      </button>
                    )}
                    {onDiscardHunk !== undefined && (
                      <button
                        tabIndex={-1}
                        className={armed ? styles.armedDiscard : undefined}
                        title={armed ? "Permanently discards this hunk. Cannot be undone." : undefined}
                        onBlur={() => setConfirmingDiscardIndex(null)}
                        onKeyDown={(event) => {
                          if (event.key === "Escape" && armed) {
                            event.stopPropagation();
                            setConfirmingDiscardIndex(null);
                          }
                        }}
                        onClick={() => {
                          if (armed) {
                            onDiscardHunk(hunk.oldStart, hunk.newStart);
                            setConfirmingDiscardIndex(null);
                          } else {
                            setConfirmingDiscardIndex(hunkIndex);
                          }
                        }}
                      >
                        {armed ? "Confirm discard" : "Discard hunk"}
                      </button>
                    )}
                  </Toolbar>
                )}
              </div>
              {split ? (
                <div className={styles.lines}>
                  {splitRows!.map((row, rowIndex) => {
                    if (row.kind === "context") {
                      const oldNo = oldLine++;
                      const newNo = newLine++;
                      return (
                        <div
                          key={rowIndex}
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
                          <span className={styles.content}>{row.line.content}</span>
                        </div>
                      );
                    }
                    if (row.kind === "pair") {
                      const oldNo = oldLine++;
                      const newNo = newLine++;
                      return (
                        <div key={rowIndex} className={styles.splitRow}>
                          <div
                            data-diff-column="old"
                            className={`${styles.splitColumn} ${styles.line} ${styles.lineRemove} diff-line diff-line-remove`}
                          >
                            <span className={styles.gutter} aria-hidden="true">
                              {oldNo}
                            </span>
                            <span className={styles.marker} aria-hidden="true">
                              {originPrefix("Remove")}
                            </span>
                            <span className={styles.content}>{renderContent(row.oldLine, wordSegments)}</span>
                          </div>
                          <div
                            data-diff-column="new"
                            className={`${styles.splitColumn} ${styles.line} ${styles.lineAdd} diff-line diff-line-add`}
                          >
                            <span className={styles.gutter} aria-hidden="true">
                              {newNo}
                            </span>
                            <span className={styles.marker} aria-hidden="true">
                              {originPrefix("Add")}
                            </span>
                            <span className={styles.content}>{renderContent(row.newLine, wordSegments)}</span>
                          </div>
                        </div>
                      );
                    }
                    if (row.kind === "old-only") {
                      const oldNo = oldLine++;
                      return (
                        <div key={rowIndex} className={styles.splitRow}>
                          <div
                            data-diff-column="old"
                            className={`${styles.splitColumn} ${styles.line} ${styles.lineRemove} diff-line diff-line-remove`}
                          >
                            <span className={styles.gutter} aria-hidden="true">
                              {oldNo}
                            </span>
                            <span className={styles.marker} aria-hidden="true">
                              {originPrefix("Remove")}
                            </span>
                            <span className={styles.content}>{row.line.content}</span>
                          </div>
                          <div className={styles.splitColumn} aria-hidden="true" />
                        </div>
                      );
                    }
                    const newNo = newLine++;
                    return (
                      <div key={rowIndex} className={styles.splitRow}>
                        <div className={styles.splitColumn} aria-hidden="true" />
                        <div
                          data-diff-column="new"
                          className={`${styles.splitColumn} ${styles.line} ${styles.lineAdd} diff-line diff-line-add`}
                        >
                          <span className={styles.gutter} aria-hidden="true">
                            {newNo}
                          </span>
                          <span className={styles.marker} aria-hidden="true">
                            {originPrefix("Add")}
                          </span>
                          <span className={styles.content}>{row.line.content}</span>
                        </div>
                      </div>
                    );
                  })}
                </div>
              ) : (
                <pre className={styles.lines}>
                  {hunk.lines.map((line, lineIndex) => {
                    const oldNo = line.origin !== "Add" ? oldLine++ : null;
                    const newNo = line.origin !== "Remove" ? newLine++ : null;
                    return (
                      <div
                        key={lineIndex}
                        className={`${styles.line} ${styles[`line${line.origin}`]} diff-line diff-line-${line.origin.toLowerCase()}`}
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
                        <span className={styles.content}>{renderContent(line, wordSegments)}</span>
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
