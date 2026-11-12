import { useEffect, useState } from "react";
import { X } from "lucide-react";
import type { KeyboardEvent } from "react";
import { formatAbsoluteDate, formatShortDate } from "../lib/formatDate";
import { useDateFormat } from "../state/useDateFormat";
import { lineRangeLabel } from "../lib/selectedLines";
import type { VisitKind } from "../lib/navigationHistory";
import type { SelectedRow } from "../state/useAppState";
import type { FileHistoryState } from "../state/useFileHistory";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { RefBadges } from "./RefBadges";
import { useVirtualRows } from "./useVirtualRows";
import graphStyles from "./CommitGraph.module.css";
import styles from "./FileHistoryList.module.css";

/** The commit graph's place while a file's (or a line range's, #447) history is open: flat rows,
 *  no lanes (history parents are rarely adjacent rows). */
export function FileHistoryList({
  history,
  selectedRow,
  onSelectRow,
  onClose,
  onLoadMore,
  onRetry,
}: {
  history: FileHistoryState;
  selectedRow: SelectedRow;
  /** `kind` `"step"` for a keyboard move by row (#618). */
  onSelectRow: (row: SelectedRow, kind?: VisitKind) => void;
  onClose: () => void;
  onLoadMore: () => void;
  onRetry: () => void;
}) {
  const dateFormat = useDateFormat();
  const selectedId = typeof selectedRow === "object" ? selectedRow.commitId : null;
  const selectedIndex = history.entries.findIndex((e) => e.commit.id === selectedId);

  // Only the rows in view render (#775), as in the commit graph (#685).
  const [listElement, setListElement] = useState<HTMLUListElement | null>(null);
  const virtual = useVirtualRows(listElement, history.entries.length);

  // Keep the keyboard-driven selection visible in a long history.
  const { scrollToIndex } = virtual;
  useEffect(() => {
    if (selectedIndex >= 0) scrollToIndex(selectedIndex);
  }, [selectedIndex, scrollToIndex]);

  const target = history.target;
  const what = target.kind === "lines" ? `${lineRangeLabel(target.startLine, target.endLine)} of ` : "";

  const handleKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    const step =
      event.key === "ArrowDown" || event.key === "j" ? 1 : event.key === "ArrowUp" || event.key === "k" ? -1 : 0;
    if (step === 0 || history.entries.length === 0) return;
    event.preventDefault();
    const next =
      selectedIndex === -1 ? 0 : Math.min(Math.max(selectedIndex + step, 0), history.entries.length - 1);
    onSelectRow({ commitId: history.entries[next].commit.id }, "step");
  };

  return (
    <>
      <div className={styles.banner} role="toolbar" aria-label={target.kind === "lines" ? "Line history" : "File history"}>
        <span>
          History{target.kind === "lines" ? ` of ${what.trimEnd()}` : ":"} <code className={styles.path}>{history.path}</code>
          {target.kind === "lines" && target.revision !== "HEAD" && <> at {target.revision.slice(0, 7)}</>}
        </span>
        <button type="button" aria-label={target.kind === "lines" ? "Close line history" : "Close file history"} onClick={onClose}>
          <X size={14} aria-hidden="true" />
        </button>
      </div>
      {history.error !== null && (
        <InlineError message={history.error.message} hint={history.error.hint} onDismiss={onClose} onRetry={onRetry} />
      )}
      {history.error === null && !history.loading && history.entries.length === 0 && (
        <p className={styles.empty}>
          No committed history for {what}<code className={styles.path}>{history.path}</code>.
        </p>
      )}
      {history.loading && history.entries.length === 0 && history.error === null && (
        <p className={styles.empty}>Loading history…</p>
      )}
      <ul
        ref={setListElement}
        className={graphStyles.list}
        role="listbox"
        aria-label="File history commits"
        tabIndex={0}
        onKeyDown={handleKeyDown}
        aria-keyshortcuts="ArrowUp ArrowDown Escape"
        style={virtual.windowed ? { paddingTop: virtual.padTop, paddingBottom: virtual.padBottom } : undefined}
      >
        {history.entries.slice(virtual.start, virtual.end).map((entry, offset) => (
          <ListRow
            key={entry.commit.id}
            rowIndex={virtual.start + offset}
            setSize={history.entries.length}
            className="commit-row"
            data-commit-id={entry.commit.id}
            selected={entry.commit.id === selectedId}
            onClick={() => onSelectRow({ commitId: entry.commit.id })}
          >
            <RefBadges commit={entry.commit} />
            <span className={graphStyles.commitSummary}>
              {entry.commit.shortId} {entry.commit.summary}
            </span>
            {entry.oldPath !== null && <span className={styles.renameChip}>renamed from {entry.oldPath}</span>}
            {"startLine" in entry && (
              <span className={styles.renameChip} title={`The lines in this commit's version of ${entry.path}`}>
                {lineRangeLabel(entry.startLine, entry.endLine)}
              </span>
            )}
            <span className={graphStyles.author} title={entry.commit.authorEmail}>
              {entry.commit.authorName}
            </span>
            <span className={graphStyles.date} title={formatAbsoluteDate(entry.commit.timestamp, dateFormat)}>
              {formatShortDate(entry.commit.timestamp, dateFormat)}
            </span>
          </ListRow>
        ))}
      </ul>
      {history.truncated && (
        <p className={styles.empty}>
          Stopped after a long stretch of history; older changes to these lines may be missing.
        </p>
      )}
      {history.loading && history.entries.length > 0 && history.error === null && (
        <p className={graphStyles.loadMore} role="status">
          Searching older commits… ({history.entries.length} {history.entries.length === 1 ? "change" : "changes"} so far)
        </p>
      )}
      {history.hasMore && !history.loading && (
        <div className={graphStyles.loadMore}>
          <span>Showing latest {history.entries.length} changes</span>
          <button type="button" onClick={onLoadMore}>
            Load more
          </button>
        </div>
      )}
    </>
  );
}
