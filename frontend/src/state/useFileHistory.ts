import { useCallback, useEffect, useRef, useState } from "react";
import type { FileHistoryEntry, LineHistoryEntry, RepoClient } from "../ipc/RepoClient";
import { describeError, type DescribedError } from "../lib/errorMessages";
import type { SelectedRow } from "./useAppState";

// Same page size as the commit graph.
export const FILE_HISTORY_PAGE_SIZE = 300;

/** What a history lists: a file's commits, or the commits that changed a range of its lines
 *  as they are in `revision` (#447). */
export type HistoryTarget =
  | { kind: "file"; path: string }
  | { kind: "lines"; path: string; revision: string; startLine: number; endLine: number };

function targetKey(target: HistoryTarget): string {
  return target.kind === "file"
    ? `file:${target.path}`
    : `lines:${target.revision}:${target.startLine}-${target.endLine}:${target.path}`;
}

export interface FileHistoryState {
  target: HistoryTarget;
  /** `target.path`. */
  path: string;
  /** A line history's entries are `LineHistoryEntry`s. */
  entries: Array<FileHistoryEntry | LineHistoryEntry>;
  limit: number;
  // True when the last load filled its limit, so older changes probably exist.
  hasMore: boolean;
  /** A line history whose walk stopped at the backend's commit cap. */
  truncated: boolean;
  loading: boolean;
  error: DescribedError | null;
}

export interface UseFileHistoryResult {
  /** `null` while the graph shows the normal history. */
  history: FileHistoryState | null;
  open(path: string): void;
  /** Opens the history of lines `startLine`..`endLine` of `path` as they are in `revision`. */
  openLines(path: string, revision: string, startLine: number, endLine: number): void;
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
  const current = useRef<{ target: HistoryTarget; limit: number } | null>(null);
  // Bumped by every load and by close; a response from an older load is dropped.
  const generation = useRef(0);
  const previousRow = useRef<SelectedRow | null>(null);
  const selectFirst = useRef(false);
  const selectedRowRef = useRef(selectedRow);
  useEffect(() => {
    selectedRowRef.current = selectedRow;
  }, [selectedRow]);

  const load = useCallback(
    async (target: HistoryTarget, limit: number) => {
      const gen = ++generation.current;
      current.current = { target, limit };
      const key = targetKey(target);
      const path = target.path;
      setHistory((prev) => {
        const same = prev !== null && targetKey(prev.target) === key;
        return {
          target,
          path,
          limit,
          entries: same ? prev.entries : [],
          hasMore: same ? prev.hasMore : false,
          truncated: same ? prev.truncated : false,
          loading: true,
          error: null,
        };
      });
      try {
        let entries: Array<FileHistoryEntry | LineHistoryEntry>;
        let truncated = false;
        if (target.kind === "file") {
          entries = await client.getFileHistory(repoPath, path, limit);
        } else {
          const lines = await client.getLineHistory(repoPath, target.revision, path, target.startLine, target.endLine, limit);
          entries = lines.entries;
          truncated = lines.truncated;
        }
        if (gen !== generation.current) return;
        setHistory({
          target,
          path,
          limit,
          entries,
          hasMore: !truncated && entries.length >= limit,
          truncated,
          loading: false,
          error: null,
        });
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

  const openTarget = useCallback(
    (target: HistoryTarget) => {
      if (current.current === null) previousRow.current = selectedRowRef.current;
      selectFirst.current = true;
      void load(target, FILE_HISTORY_PAGE_SIZE);
    },
    [load],
  );
  const open = useCallback((path: string) => openTarget({ kind: "file", path }), [openTarget]);
  const openLines = useCallback(
    (path: string, revision: string, startLine: number, endLine: number) =>
      openTarget({ kind: "lines", path, revision, startLine, endLine }),
    [openTarget],
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
    if (open !== null) void load(open.target, open.limit + FILE_HISTORY_PAGE_SIZE);
  }, [load]);

  const retry = useCallback(() => {
    const open = current.current;
    if (open !== null) void load(open.target, open.limit);
  }, [load]);

  // Any repo refresh (commit, checkout, fetch…) can change the file's history.
  useEffect(() => {
    const open = current.current;
    if (open !== null) void load(open.target, open.limit);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshGeneration]);

  return { history, open, openLines, close, loadMore, retry };
}
