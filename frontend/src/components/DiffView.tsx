import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { DiffHunk, DiffLineOrigin } from "../ipc/RepoClient";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./DiffView.module.css";

const DISCARD_DISARM_MS = 5000;

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
  const hunkRefs = useRef<Record<number, HTMLDivElement | null>>({});

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
                      Stage Hunk
                    </button>
                  )}
                  {onUnstageHunk !== undefined && (
                    <button tabIndex={-1} onClick={() => onUnstageHunk(hunk.oldStart, hunk.newStart)}>
                      Unstage Hunk
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
                      {armed ? "Confirm Discard" : "Discard Hunk"}
                    </button>
                  )}
                </Toolbar>
              )}
            </div>
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
                    <span className={styles.content}>{line.content}</span>
                  </div>
                );
              })}
            </pre>
          </div>
        );
      })}
    </div>
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
