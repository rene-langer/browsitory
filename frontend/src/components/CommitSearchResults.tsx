import { useEffect, useRef } from "react";
import type { KeyboardEvent, RefObject } from "react";
import { formatShortDate } from "../lib/formatDate";
import { searchStatusText } from "../lib/searchStatus";
import type { SelectedRow } from "../state/useAppState";
import type { CommitSearchResultsState } from "../state/useCommitSearch";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import graphStyles from "./CommitGraph.module.css";
import styles from "./CommitSearchResults.module.css";

/** The commit graph's place while a search is open: flat rows of matching commits. */
export function CommitSearchResults({
  results,
  selectedRow,
  onSelectRow,
  onClose,
  onLoadMore,
  onRetry,
  onRequestSearch,
  listRef,
}: {
  results: CommitSearchResultsState;
  selectedRow: SelectedRow;
  onSelectRow: (row: SelectedRow) => void;
  onClose: () => void;
  onLoadMore: () => void;
  onRetry: () => void;
  /** Ctrl/Cmd+F in the list: focus the search field. */
  onRequestSearch: () => void;
  listRef?: RefObject<HTMLUListElement | null>;
}) {
  const selectedId = typeof selectedRow === "object" ? selectedRow.commitId : null;
  const selectedIndex = results.commits.findIndex((c) => c.id === selectedId);

  const ownRef = useRef<HTMLUListElement>(null);
  const ref = listRef ?? ownRef;

  // Keep the keyboard-driven selection visible in a long result list.
  useEffect(() => {
    const selected = ref.current?.children[selectedIndex] as HTMLElement | undefined;
    if (typeof selected?.scrollIntoView === "function") selected.scrollIntoView({ block: "nearest" });
  }, [selectedIndex, ref]);

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
    const step =
      event.key === "ArrowDown" || event.key === "j" ? 1 : event.key === "ArrowUp" || event.key === "k" ? -1 : 0;
    if (step === 0 || results.commits.length === 0) return;
    event.preventDefault();
    const next =
      selectedIndex === -1 ? 0 : Math.min(Math.max(selectedIndex + step, 0), results.commits.length - 1);
    onSelectRow({ commitId: results.commits[next].id });
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
        ref={ref}
        className={graphStyles.list}
        role="listbox"
        aria-label="Search results"
        tabIndex={0}
        onKeyDown={handleKeyDown}
        aria-keyshortcuts="ArrowUp ArrowDown Escape Control+F Meta+F"
      >
        {results.commits.map((commit) => (
          <ListRow
            key={commit.id}
            className="commit-row"
            data-commit-id={commit.id}
            selected={commit.id === selectedId}
            onClick={() => onSelectRow({ commitId: commit.id })}
          >
            {commit.branchRefs.map((name) => (
              <span key={name} className={graphStyles.branchBadge}>
                {name}
              </span>
            ))}
            {commit.remoteBranchRefs.map((name) => (
              <span key={name} className={graphStyles.remoteBranchBadge}>
                {name}
              </span>
            ))}
            <span className={graphStyles.commitSummary}>
              {commit.shortId} {commit.summary}
            </span>
            <span className={graphStyles.author} title={commit.authorEmail}>
              {commit.authorName}
            </span>
            <span className={graphStyles.date} title={new Date(commit.timestamp * 1000).toLocaleString()}>
              {formatShortDate(commit.timestamp)}
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
