import { useEffect, useState } from "react";
import type { DiffHunk, DiffLineOrigin } from "../ipc/RepoClient";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./DiffView.module.css";

export function DiffView({
  hunks,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
}: {
  hunks: DiffHunk[];
  onStageHunk?: (oldStart: number, newStart: number) => void;
  onUnstageHunk?: (oldStart: number, newStart: number) => void;
  onDiscardHunk?: (oldStart: number, newStart: number) => void;
}) {
  const [confirmingDiscardIndex, setConfirmingDiscardIndex] = useState<number | null>(null);

  // A stale armed confirmation must never carry over to a different hunk list (switching files,
  // or any hunk mutation refetching this file's own hunks) — `hunkIndex` is only meaningful
  // relative to the specific `hunks` array it was armed against. Same class of bug
  // `BranchSwitcher`'s `closePopoverState` guards against for `pendingForceFor`.
  useEffect(() => {
    setConfirmingDiscardIndex(null);
  }, [hunks]);

  if (hunks.length === 0) {
    return <p>No differences</p>;
  }

  return (
    <div>
      {hunks.map((hunk, hunkIndex) => (
        <div key={hunkIndex}>
          <div className={styles.hunkHeader}>
            <span>
              @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},{hunk.newLines} @@
            </span>
            {(onStageHunk !== undefined || onUnstageHunk !== undefined || onDiscardHunk !== undefined) && (
              <Toolbar>
                {onStageHunk !== undefined && (
                  <button onClick={() => onStageHunk(hunk.oldStart, hunk.newStart)}>Stage Hunk</button>
                )}
                {onUnstageHunk !== undefined && (
                  <button onClick={() => onUnstageHunk(hunk.oldStart, hunk.newStart)}>Unstage Hunk</button>
                )}
                {onDiscardHunk !== undefined &&
                  (confirmingDiscardIndex === hunkIndex ? (
                    <button
                      onClick={() => {
                        onDiscardHunk(hunk.oldStart, hunk.newStart);
                        setConfirmingDiscardIndex(null);
                      }}
                    >
                      Confirm Discard
                    </button>
                  ) : (
                    <button onClick={() => setConfirmingDiscardIndex(hunkIndex)}>Discard Hunk</button>
                  ))}
              </Toolbar>
            )}
          </div>
          <pre>
            {hunk.lines.map((line, lineIndex) => (
              <div
                key={lineIndex}
                className={`${styles.line} ${styles[`line${line.origin}`]} diff-line diff-line-${line.origin.toLowerCase()}`}
              >
                <span aria-hidden="true">{originPrefix(line.origin)}</span>
                {line.content}
              </div>
            ))}
          </pre>
        </div>
      ))}
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
