import { useCallback, useEffect, useRef, useState } from "react";
import type { GraphCommit, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
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
  error: DescribedError | null;
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
}

export function useCommitSearch(
  client: RepoClient,
  repoPath: string,
  refreshGeneration: number,
  refs: string[] | null,
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
  useEffect(() => {
    resultsRef.current = results;
  }, [results]);
  useEffect(() => {
    selectedRowRef.current = selectedRow;
  }, [selectedRow]);
  useEffect(() => {
    refsRef.current = refs;
  }, [refs]);

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
      try {
        do {
          const page = await client.searchCommits(repoPath, q, refsRef.current, cursor);
          if (gen !== generation.current) return;
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
            selectRow({ commitId: page.commits[0].id });
          }
        } while (found < SEARCH_ROUND_SIZE && cursor !== null);
        inFlight.current = false;
        setResults((prev) => prev && { ...prev, loading: false });
      } catch (err: unknown) {
        if (gen !== generation.current) return;
        inFlight.current = false;
        setResults((prev) => prev && { ...prev, loading: false, error: describeError(err) });
      }
    },
    [client, repoPath, selectRow],
  );

  const start = useCallback(
    (q: string, { selectFirst }: { selectFirst: boolean }) => {
      if (activeQuery.current === null) previousRow.current = selectedRowRef.current;
      activeQuery.current = q;
      const gen = ++generation.current;
      setResults({ query: q, commits: [], nextCursor: null, scanned: 0, loading: true, error: null });
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
    setResults((prev) => prev && { ...prev, loading: true, error: null });
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

  return { query, results, setQuery, loadMore, retry, close };
}
