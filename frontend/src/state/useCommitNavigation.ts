import { useCallback, useMemo } from "react";
import { navigationTarget, recordVisit } from "../lib/navigationHistory";
import type { SelectedRow, UseAppStateResult } from "./useAppState";
import type { UseCommitSearchResult } from "./useCommitSearch";
import type { UseFileHistoryResult } from "./useFileHistory";

export interface CommitNavigation {
  canGoBack: boolean;
  canGoForward: boolean;
  goBack(): void;
  goForward(): void;
}

/**
 * Back and Forward through the rows this tab visited (#618). An entry is available while a list
 * shows its row: the graph's loaded history, or the open search results or file history. Others
 * (a commit rewritten away, or on a branch since hidden) are skipped. A row the open list shows
 * is selected in it; any other is revealed in the graph, as Go to does.
 */
export function useCommitNavigation(
  appState: UseAppStateResult,
  fileHistory: UseFileHistoryResult,
  commitSearch: UseCommitSearchResult,
  reveal: (row: SelectedRow) => void,
  selectRow: (row: SelectedRow) => void,
): CommitNavigation {
  const { commits, navigation, selectedRow } = appState.state;
  const historyEntries = fileHistory.history?.entries;
  const searchCommits = commitSearch.results?.commits;
  const listed = useMemo(() => {
    const ids = new Set<string>();
    for (const entry of historyEntries ?? []) ids.add(entry.commit.id);
    for (const commit of searchCommits ?? []) ids.add(commit.id);
    return ids;
  }, [historyEntries, searchCommits]);
  const loaded = useMemo(() => new Set(commits.map((commit) => commit.id)), [commits]);
  const isAvailable = useCallback(
    (row: SelectedRow) => row === "uncommitted" || loaded.has(row.commitId) || listed.has(row.commitId),
    [loaded, listed],
  );

  const { stepNavigation } = appState;
  const step = useCallback(
    (direction: -1 | 1) => {
      const row = stepNavigation(direction, isAvailable);
      if (row === null) return;
      if (row !== "uncommitted" && listed.has(row.commitId)) selectRow(row);
      else reveal(row);
    },
    [stepNavigation, isAvailable, listed, selectRow, reveal],
  );
  const goBack = useCallback(() => step(-1), [step]);
  const goForward = useCallback(() => step(1), [step]);

  const synced = recordVisit(navigation, selectedRow, "jump");
  return {
    canGoBack: navigationTarget(synced, -1, isAvailable) !== null,
    canGoForward: navigationTarget(synced, 1, isAvailable) !== null,
    goBack,
    goForward,
  };
}
