import { useCallback, useEffect, useState } from "react";
import type { KeyboardEvent, RefObject } from "react";
import { formatAbsoluteDate, formatShortDate } from "../lib/formatDate";
import { useDateFormat } from "../state/useDateFormat";
import { searchStatusText } from "../lib/searchStatus";
import type { VisitKind } from "../lib/navigationHistory";
import type { SelectedRow } from "../state/useAppState";
import type { CommitSearchResultsState } from "../state/useCommitSearch";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { RefBadges } from "./RefBadges";
import { useVirtualRows } from "./useVirtualRows";
import graphStyles from "./CommitGraph.module.css";
import styles from "./CommitSearchResults.module.css";

/** The search's "List" view (#687): flat rows of the matching commits in the graph's place. */
export function CommitSearchResults({
  results,
  selectedRow,
  onSelectRow,
  onClose,
  onLoadMore,
  onRetry,
  onRequestSearch,
  listRef,
  onStepMatch,
}: {
  results: CommitSearchResultsState;
  selectedRow: SelectedRow;
  /** `kind` `"step"` for a keyboard move by row (#618). */
  onSelectRow: (row: SelectedRow, kind?: VisitKind) => void;
  onClose: () => void;
  onLoadMore: () => void;
  onRetry: () => void;
  /** Ctrl/Cmd+F in the list: focus the search field. */
  onRequestSearch: () => void;
  listRef?: RefObject<HTMLUListElement | null>;
  /** F3 / Shift+F3: the next / previous match (#687). */
  onStepMatch?: (direction: -1 | 1) => void;
}) {
  const dateFormat = useDateFormat();
  const selectedId = typeof selectedRow === "object" ? selectedRow.commitId : null;
  const selectedIndex = results.commits.findIndex((c) => c.id === selectedId);

  // Only the rows in view render (#775), as in the commit graph (#685).
  const [listElement, setListElement] = useState<HTMLUListElement | null>(null);
  const attachList = useCallback(
    (element: HTMLUListElement | null) => {
      if (listRef !== undefined) listRef.current = element;
      setListElement(element);
    },
    [listRef],
  );
  const virtual = useVirtualRows(listElement, results.commits.length);

  // Keep the keyboard-driven selection visible in a long result list.
  const { scrollToIndex } = virtual;
  useEffect(() => {
    if (selectedIndex >= 0) scrollToIndex(selectedIndex);
  }, [selectedIndex, scrollToIndex]);

  const handleKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "f") {
      event.preventDefault();
      onRequestSearch();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key === "F3" && onStepMatch !== undefined) {
      event.preventDefault();
      onStepMatch(event.shiftKey ? -1 : 1);
      return;
    }
    const step =
      event.key === "ArrowDown" || event.key === "j" ? 1 : event.key === "ArrowUp" || event.key === "k" ? -1 : 0;
    if (step === 0 || results.commits.length === 0) return;
    event.preventDefault();
    const next =
      selectedIndex === -1 ? 0 : Math.min(Math.max(selectedIndex + step, 0), results.commits.length - 1);
    onSelectRow({ commitId: results.commits[next].id }, "step");
  };

  return (
    <>
      {results.error !== null ? (
        <InlineError message={results.error.message} hint={results.error.hint} onDismiss={onClose} onRetry={onRetry} />
      ) : (
        <p className={styles.status} role="status">
          {searchStatusText(results)}
        </p>
      )}
      <ul
        ref={attachList}
        className={graphStyles.list}
        role="listbox"
        aria-label="Search results"
        tabIndex={0}
        onKeyDown={handleKeyDown}
        aria-keyshortcuts="ArrowUp ArrowDown Escape Control+F Meta+F"
        style={virtual.windowed ? { paddingTop: virtual.padTop, paddingBottom: virtual.padBottom } : undefined}
      >
        {results.commits.slice(virtual.start, virtual.end).map((commit, offset) => (
          <ListRow
            key={commit.id}
            rowIndex={virtual.start + offset}
            setSize={results.commits.length}
            className="commit-row"
            data-commit-id={commit.id}
            selected={commit.id === selectedId}
            onClick={() => onSelectRow({ commitId: commit.id })}
          >
            <RefBadges commit={commit} />
            <span className={graphStyles.commitSummary}>
              {commit.shortId} {commit.summary}
            </span>
            <span className={graphStyles.author} title={commit.authorEmail}>
              {commit.authorName}
            </span>
            <span className={graphStyles.date} title={formatAbsoluteDate(commit.timestamp, dateFormat)}>
              {formatShortDate(commit.timestamp, dateFormat)}
            </span>
          </ListRow>
        ))}
      </ul>
      {results.nextCursor !== null && !results.loading && (
        <div className={graphStyles.loadMore}>
          <button type="button" onClick={onLoadMore}>
            Load more
          </button>
        </div>
      )}
    </>
  );
}
