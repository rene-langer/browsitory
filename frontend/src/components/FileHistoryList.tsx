import { useEffect, useRef } from "react";
import type { KeyboardEvent } from "react";
import { formatShortDate } from "../lib/formatDate";
import type { SelectedRow } from "../state/useAppState";
import type { FileHistoryState } from "../state/useFileHistory";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import graphStyles from "./CommitGraph.module.css";
import styles from "./FileHistoryList.module.css";

/** The commit graph's place while a file's history is open: flat rows, no lanes (history
 *  parents are rarely adjacent rows). */
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
  onSelectRow: (row: SelectedRow) => void;
  onClose: () => void;
  onLoadMore: () => void;
  onRetry: () => void;
}) {
  const selectedId = typeof selectedRow === "object" ? selectedRow.commitId : null;
  const selectedIndex = history.entries.findIndex((e) => e.commit.id === selectedId);

  const listRef = useRef<HTMLUListElement>(null);

  // Keep the keyboard-driven selection visible in a long history.
  useEffect(() => {
    const selected = listRef.current?.children[selectedIndex] as HTMLElement | undefined;
    if (typeof selected?.scrollIntoView === "function") selected.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);

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
    onSelectRow({ commitId: history.entries[next].commit.id });
  };

  return (
    <>
      <div className={styles.banner} role="toolbar" aria-label="File history">
        <span>
          History: <code className={styles.path}>{history.path}</code>
        </span>
        <button type="button" aria-label="Close file history" onClick={onClose}>
          ×
        </button>
      </div>
      {history.error !== null && (
        <InlineError message={history.error.message} hint={history.error.hint} onDismiss={onClose} onRetry={onRetry} />
      )}
      {history.error === null && !history.loading && history.entries.length === 0 && (
        <p className={styles.empty}>
          No committed history for <code className={styles.path}>{history.path}</code>.
        </p>
      )}
      {history.loading && history.entries.length === 0 && history.error === null && (
        <p className={styles.empty}>Loading history…</p>
      )}
      <ul
        ref={listRef}
        className={graphStyles.list}
        role="listbox"
        aria-label="File history commits"
        tabIndex={0}
        onKeyDown={handleKeyDown}
        aria-keyshortcuts="ArrowUp ArrowDown Escape"
      >
        {history.entries.map((entry) => (
          <ListRow
            key={entry.commit.id}
            className="commit-row"
            data-commit-id={entry.commit.id}
            selected={entry.commit.id === selectedId}
            onClick={() => onSelectRow({ commitId: entry.commit.id })}
          >
            {entry.commit.branchRefs.map((ref) => (
              <span key={ref} className={graphStyles.branchBadge}>
                {ref}
              </span>
            ))}
            {entry.commit.remoteBranchRefs.map((ref) => (
              <span key={ref} className={graphStyles.remoteBranchBadge}>
                {ref}
              </span>
            ))}
            <span className={graphStyles.commitSummary}>
              {entry.commit.shortId} {entry.commit.summary}
            </span>
            {entry.oldPath !== null && <span className={styles.renameChip}>renamed from {entry.oldPath}</span>}
            <span className={graphStyles.author} title={entry.commit.authorEmail}>
              {entry.commit.authorName}
            </span>
            <span className={graphStyles.date} title={new Date(entry.commit.timestamp * 1000).toLocaleString()}>
              {formatShortDate(entry.commit.timestamp)}
            </span>
          </ListRow>
        ))}
      </ul>
      {history.hasMore && (
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
