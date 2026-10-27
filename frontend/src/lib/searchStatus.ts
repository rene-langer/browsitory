import type { CommitSearchResultsState } from "../state/useCommitSearch";

const count = (n: number) => n.toLocaleString("en-US");

/** The status line above the search results. */
export function searchStatusText(results: CommitSearchResultsState): string {
  if (results.loading) {
    return results.scanned === 0 ? "Searching…" : `Scanned ${count(results.scanned)} commits…`;
  }
  const matches = results.commits.length;
  if (matches === 0) return "No commits match this search.";
  const label = matches === 1 ? "1 match" : `${count(matches)} matches`;
  return results.nextCursor === null ? label : `${label} in the first ${count(results.scanned)} commits`;
}
