import { useCallback, useEffect, useMemo, useRef } from "react";
import type { GraphCommit } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import type { CommitSearchResultsState } from "./useCommitSearch";

const count = (n: number) => n.toLocaleString("en-US");

export interface SearchMatches {
  /** The matching commits' ids, for the graph to dim the others; `null` without a search. */
  ids: ReadonlySet<string> | null;
  /** The selected commit's index among the matches, -1 when it is none of them. */
  current: number;
  /** "3 of 42", "42 matches", "3 of 100+" while more pages exist, "Searching…". */
  text: string;
  canGoPrevious: boolean;
  canGoNext: boolean;
  /** Selects the previous (`-1`) or next (`1`) match, reading more pages when needed. */
  step(direction: -1 | 1): Promise<void>;
}

/** What the search bar's match count says (#687). */
export function matchCountText(results: CommitSearchResultsState, current: number): string {
  const total = results.commits.length;
  const more = results.nextCursor !== null ? "+" : "";
  if (results.queryError !== null) return "";
  if (total === 0) return results.loading ? "Searching…" : "No matches";
  if (current >= 0) return `${count(current + 1)} of ${count(total)}${more}`;
  return total === 1 && more === "" ? "1 match" : `${count(total)}${more} matches`;
}

/**
 * Previous/next through a search's matches (#687) and their count. Matches come in the graph's
 * order; from a selected commit that is no match, Next picks the first match below it in the
 * graph and Previous the last one above it. Past the last loaded match, Next reads the search's
 * next round. `reveal` selects a match and shows it in the graph (loading history or expanding
 * a merge as Go to does) or in the list.
 */
export function useSearchMatches(
  results: CommitSearchResultsState | null,
  selectedRow: SelectedRow,
  graphCommits: GraphCommit[],
  loadMore: () => Promise<GraphCommit[]>,
  reveal: (commitId: string) => void,
): SearchMatches {
  const selectedId = typeof selectedRow === "object" ? selectedRow.commitId : null;
  const matchList = results?.commits;
  const ids = useMemo(() => (matchList === undefined ? null : new Set(matchList.map((c) => c.id))), [matchList]);
  const current = matchList === undefined || selectedId === null ? -1 : matchList.findIndex((c) => c.id === selectedId);
  const latest = useRef({ results, current, selectedId, graphCommits });
  useEffect(() => {
    latest.current = { results, current, selectedId, graphCommits };
  }, [results, current, selectedId, graphCommits]);

  const step = useCallback(
    async (direction: -1 | 1) => {
      const { results: now, current: index, selectedId: selected, graphCommits: rows } = latest.current;
      if (now === null) return;
      let matches = now.commits;
      let target: string | undefined;
      if (index >= 0) {
        target = matches[index + direction]?.id;
      } else {
        const rowIndex = new Map(rows.map((commit, i) => [commit.id, i]));
        const from = selected === null ? -1 : (rowIndex.get(selected) ?? -1);
        const positions = matches.map((match) => rowIndex.get(match.id) ?? Number.POSITIVE_INFINITY);
        if (direction === 1) target = matches.find((_, i) => positions[i] > from)?.id;
        else target = [...matches].reverse().find((_, i) => positions[matches.length - 1 - i] < from)?.id;
      }
      if (target === undefined && direction === 1 && now.nextCursor !== null && !now.loading) {
        const before = matches.length;
        matches = await loadMore();
        target = matches[before]?.id;
      }
      if (target !== undefined) reveal(target);
    },
    [loadMore, reveal],
  );

  if (results === null) {
    return { ids: null, current: -1, text: "", canGoPrevious: false, canGoNext: false, step };
  }
  const total = results.commits.length;
  return {
    ids,
    current,
    text: matchCountText(results, current),
    canGoPrevious: total > 0 && current !== 0,
    canGoNext: total > 0 && (current < total - 1 || results.nextCursor !== null),
    step,
  };
}
