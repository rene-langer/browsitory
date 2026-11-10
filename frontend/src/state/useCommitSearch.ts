import { useCallback, useEffect, useRef, useState } from "react";
import { errorKind, type GraphCommit, type RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import { readRecentSearches, withRecentSearch, writeRecentSearches } from "../lib/recentSearches";
import type { SelectedRow } from "./useAppState";

/** Quiet time after the last keystroke before a search starts. */
export const SEARCH_DEBOUNCE_MS = 250;
/** One round keeps requesting pages until it has this many new matches or the walk ends. */
export const SEARCH_ROUND_SIZE = 100;

export interface CommitSearchResultsState {
  /** Trimmed query these results belong to. */
  query: string;
  commits: GraphCommit[];
  nextCursor: string | null;
  scanned: number;
  loading: boolean;
  /** A failed search, shown with the results. */
  error: DescribedError | null;
  /** The backend's message when the query itself is invalid (#450); the search field shows it. */
  queryError: string | null;
}

export interface UseCommitSearchResult {
  /** Text in the field. */
  query: string;
  /** `null` while the graph shows (empty field). */
  results: CommitSearchResultsState | null;
  setQuery(text: string): void;
  loadMore(): void;
  retry(): void;
  close(): void;
  /** This repository's recent searches, newest first (#540): queries that ran without an
   *  `InvalidSearchQuery` error, one per search (the last query that ran before the field was
   *  cleared). */
  recentSearches: string[];
  removeRecentSearch(query: string): void;
}

function rowKey(row: SelectedRow): string {
  return row === "uncommitted" ? row : `commit:${row.commitId}`;
}

export function useCommitSearch(
  client: RepoClient,
  repoPath: string,
  refreshGeneration: number,
  refs: string[] | null,
  // The graph's remote selection (`null` = none walked), so search walks the graph's commits.
  remoteRefs: string[] | null,
  selectedRow: SelectedRow,
  selectRow: (row: SelectedRow) => void,
  options?: { debounceMs?: number },
): UseCommitSearchResult {
  const debounceMs = options?.debounceMs ?? SEARCH_DEBOUNCE_MS;
  const [query, setQueryText] = useState("");
  const [results, setResults] = useState<CommitSearchResultsState | null>(null);

  // Bumped by every round and by close; a response from an older round is dropped.
  const generation = useRef(0);
  // The query the open results belong to, or `null` while the graph shows.
  const activeQuery = useRef<string | null>(null);
  // True while a round is waiting on the backend, so close/unmount know to cancel it.
  const inFlight = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const previousRow = useRef<SelectedRow | null>(null);
  const resultsRef = useRef(results);
  const selectedRowRef = useRef(selectedRow);
  const refsRef = useRef(refs);
  const remoteRefsRef = useRef(remoteRefs);
  useEffect(() => {
    resultsRef.current = results;
  }, [results]);
  useEffect(() => {
    selectedRowRef.current = selectedRow;
  }, [selectedRow]);
  useEffect(() => {
    refsRef.current = refs;
    remoteRefsRef.current = remoteRefs;
  }, [refs, remoteRefs]);

  const [recent, setRecent] = useState(() => ({ repoPath, searches: readRecentSearches(repoPath) }));
  if (recent.repoPath !== repoPath) setRecent({ repoPath, searches: readRecentSearches(repoPath) });
  // The list as it was when the current search started (`null` while the graph shows). Each
  // query that runs replaces the search's entry, so typing "author:ada" leaves one entry, not
  // one per debounced prefix.
  const recentBeforeSearch = useRef<string[] | null>(null);

  const recordSearch = useCallback(
    (q: string) => {
      const base = recentBeforeSearch.current ?? readRecentSearches(repoPath);
      recentBeforeSearch.current = base;
      const searches = withRecentSearch(base, q);
      writeRecentSearches(repoPath, searches);
      setRecent({ repoPath, searches });
    },
    [repoPath],
  );

  const removeRecentSearch = useCallback(
    (q: string) => {
      const searches = readRecentSearches(repoPath).filter((s) => s !== q);
      if (recentBeforeSearch.current !== null) {
        recentBeforeSearch.current = recentBeforeSearch.current.filter((s) => s !== q);
      }
      writeRecentSearches(repoPath, searches);
      setRecent({ repoPath, searches });
    },
    [repoPath],
  );

  const clearTimer = () => {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };

  const runRound = useCallback(
    async (gen: number, q: string, from: string | null, selectFirst: boolean) => {
      inFlight.current = true;
      let cursor = from;
      let found = 0;
      let selectFirstPending = selectFirst;
      // A row picked by the user (or file history) after the round started wins over the
      // auto-selected first match.
      const rowAtStart = rowKey(selectedRowRef.current);
      try {
        do {
          const page = await client.searchCommits(repoPath, q, refsRef.current, remoteRefsRef.current ?? [], cursor);
          if (gen !== generation.current) return;
          // The query parsed and ran: remember it (a "load more" round already has).
          if (from === null && cursor === null) recordSearch(q);
          found += page.commits.length;
          cursor = page.nextCursor;
          setResults(
            (prev) =>
              prev && {
                ...prev,
                commits: [...prev.commits, ...page.commits],
                nextCursor: page.nextCursor,
                scanned: page.scanned,
              },
          );
          if (selectFirstPending && page.commits.length > 0) {
            selectFirstPending = false;
            if (rowKey(selectedRowRef.current) === rowAtStart) {
              selectRow({ commitId: page.commits[0].id });
            }
          }
        } while (found < SEARCH_ROUND_SIZE && cursor !== null);
        inFlight.current = false;
        setResults((prev) => prev && { ...prev, loading: false });
      } catch (err: unknown) {
        if (gen !== generation.current) return;
        inFlight.current = false;
        if (errorKind(err) === "InvalidSearchQuery") {
          const message = err instanceof Error ? err.message : String(err);
          setResults((prev) => prev && { ...prev, loading: false, queryError: message });
        } else {
          setResults((prev) => prev && { ...prev, loading: false, error: describeError(err) });
        }
      }
    },
    [client, repoPath, selectRow, recordSearch],
  );

  const start = useCallback(
    (q: string, { selectFirst }: { selectFirst: boolean }) => {
      if (activeQuery.current === null) previousRow.current = selectedRowRef.current;
      activeQuery.current = q;
      const gen = ++generation.current;
      setResults({ query: q, commits: [], nextCursor: null, scanned: 0, loading: true, error: null, queryError: null });
      void runRound(gen, q, null, selectFirst);
    },
    [runRound],
  );

  /** Back to the graph: drops the results, cancels a running search, restores the selection. */
  const closeResults = useCallback(() => {
    generation.current++;
    clearTimer();
    if (inFlight.current) {
      inFlight.current = false;
      void client.cancelSearch(repoPath).catch(() => {});
    }
    activeQuery.current = null;
    recentBeforeSearch.current = null;
    setResults(null);
    const prev = previousRow.current;
    previousRow.current = null;
    if (prev !== null) selectRow(prev);
  }, [client, repoPath, selectRow]);

  const close = useCallback(() => {
    setQueryText("");
    closeResults();
  }, [closeResults]);

  const setQuery = useCallback(
    (text: string) => {
      setQueryText(text);
      clearTimer();
      const q = text.trim();
      if (q === "") {
        closeResults();
        return;
      }
      timer.current = setTimeout(() => {
        timer.current = null;
        start(q, { selectFirst: true });
      }, debounceMs);
    },
    [closeResults, start, debounceMs],
  );

  const loadMore = useCallback(() => {
    const current = resultsRef.current;
    if (current === null || current.loading || current.nextCursor === null) return;
    const gen = ++generation.current;
    setResults((prev) => prev && { ...prev, loading: true, error: null, queryError: null });
    void runRound(gen, current.query, current.nextCursor, false);
  }, [runRound]);

  const retry = useCallback(() => {
    if (activeQuery.current !== null) start(activeQuery.current, { selectFirst: false });
  }, [start]);

  // A repo refresh (commit, fetch…) or a different graph branch selection can change the
  // matches: re-run from the first page, keeping the selection.
  const refsKey = JSON.stringify(refs);
  const mounted = useRef(false);
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    if (activeQuery.current !== null) start(activeQuery.current, { selectFirst: false });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshGeneration, refsKey]);

  useEffect(
    () => () => {
      clearTimer();
      generation.current++;
      if (inFlight.current) {
        inFlight.current = false;
        void client.cancelSearch(repoPath).catch(() => {});
      }
    },
    [client, repoPath],
  );

  return { query, results, setQuery, loadMore, retry, close, recentSearches: recent.searches, removeRecentSearch };
}
