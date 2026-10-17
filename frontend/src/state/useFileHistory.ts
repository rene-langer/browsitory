import { useCallback, useEffect, useRef, useState } from "react";
import type { FileHistoryEntry, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import type { SelectedRow } from "./useAppState";

// Same page size as the commit graph.
export const FILE_HISTORY_PAGE_SIZE = 300;

export interface FileHistoryState {
  path: string;
  entries: FileHistoryEntry[];
  limit: number;
  // True when the last load filled its limit, so older changes probably exist.
  hasMore: boolean;
  loading: boolean;
  error: DescribedError | null;
}

export interface UseFileHistoryResult {
  /** `null` while the graph shows the normal history. */
  history: FileHistoryState | null;
  open(path: string): void;
  close(): void;
  loadMore(): void;
  retry(): void;
}

export function useFileHistory(
  client: RepoClient,
  repoPath: string,
  refreshGeneration: number,
  selectedRow: SelectedRow,
  selectRow: (row: SelectedRow) => void,
): UseFileHistoryResult {
  const [history, setHistory] = useState<FileHistoryState | null>(null);
  // What is open right now, readable from callbacks without re-creating them.
  const current = useRef<{ path: string; limit: number } | null>(null);
  // Bumped by every load and by close; a response from an older load is dropped.
  const generation = useRef(0);
  const previousRow = useRef<SelectedRow | null>(null);
  const selectFirst = useRef(false);
  const selectedRowRef = useRef(selectedRow);
  useEffect(() => {
    selectedRowRef.current = selectedRow;
  }, [selectedRow]);

  const load = useCallback(
    async (path: string, limit: number) => {
      const gen = ++generation.current;
      current.current = { path, limit };
      setHistory((prev) => ({
        path,
        limit,
        entries: prev?.path === path ? prev.entries : [],
        hasMore: prev?.path === path ? prev.hasMore : false,
        loading: true,
        error: null,
      }));
      try {
        const entries = await client.getFileHistory(repoPath, path, limit);
        if (gen !== generation.current) return;
        setHistory({ path, limit, entries, hasMore: entries.length >= limit, loading: false, error: null });
        if (selectFirst.current) {
          selectFirst.current = false;
          if (entries.length > 0) selectRow({ commitId: entries[0].commit.id });
        }
      } catch (err: unknown) {
        if (gen !== generation.current) return;
        selectFirst.current = false;
        setHistory((prev) => (prev === null ? null : { ...prev, loading: false, error: describeError(err) }));
      }
    },
    [client, repoPath, selectRow],
  );

  const open = useCallback(
    (path: string) => {
      if (current.current === null) previousRow.current = selectedRowRef.current;
      selectFirst.current = true;
      void load(path, FILE_HISTORY_PAGE_SIZE);
    },
    [load],
  );

  const close = useCallback(() => {
    generation.current++;
    current.current = null;
    selectFirst.current = false;
    setHistory(null);
    const prev = previousRow.current;
    previousRow.current = null;
    if (prev !== null) selectRow(prev);
  }, [selectRow]);

  const loadMore = useCallback(() => {
    const open = current.current;
    if (open !== null) void load(open.path, open.limit + FILE_HISTORY_PAGE_SIZE);
  }, [load]);

  const retry = useCallback(() => {
    const open = current.current;
    if (open !== null) void load(open.path, open.limit);
  }, [load]);

  // Any repo refresh (commit, checkout, fetch…) can change the file's history.
  useEffect(() => {
    const open = current.current;
    if (open !== null) void load(open.path, open.limit);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshGeneration]);

  return { history, open, close, loadMore, retry };
}
