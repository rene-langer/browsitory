import type { GraphCommit } from "../ipc/RepoClient";

/** How many commits of a merge one expand (or "Show more") loads (#617). */
export const MERGE_PAGE_SIZE = 100;

/** An expanded merge (#617): the commits `getMergedCommits` returned for `limit`. */
export interface MergeExpansion {
  commits: GraphCommit[];
  limit: number;
}

/** True when an expanded merge has more commits than it loaded. */
export function expansionHasMore(expansion: MergeExpansion): boolean {
  return expansion.commits.length >= expansion.limit;
}

/**
 * The commit list the graph shows (#617): the walk's `rows`, each expanded merge's commits right
 * below it. A merge's commits are never ancestors of the rows below it on its first-parent line,
 * so the list stays children-before-parents, as `assignLanes` needs. A commit that is already a
 * row (another selected branch walks it) or that an earlier merge listed is not repeated.
 */
export function withExpandedMerges(
  rows: GraphCommit[],
  expansions: Readonly<Record<string, MergeExpansion>>,
): GraphCommit[] {
  if (Object.keys(expansions).length === 0) return rows;
  const shown = new Set(rows.map((commit) => commit.id));
  const result: GraphCommit[] = [];
  for (const row of rows) {
    result.push(row);
    const expansion = expansions[row.id];
    if (expansion === undefined) continue;
    for (const commit of expansion.commits) {
      if (shown.has(commit.id)) continue;
      shown.add(commit.id);
      result.push(commit);
    }
  }
  return result;
}

/**
 * The expanded merge each commit `withExpandedMerges` spliced into `commits` belongs to: the run
 * of a merge's commits right below its row. Commits that are rows of their own are not in it.
 */
export function expandedFrom(
  commits: GraphCommit[],
  expansions: Readonly<Record<string, MergeExpansion>>,
): Map<string, string> {
  const owner = new Map<string, string>();
  commits.forEach((merge, index) => {
    const expansion = expansions[merge.id];
    if (expansion === undefined) return;
    const listed = new Set(expansion.commits.map((commit) => commit.id));
    for (let next = index + 1; next < commits.length && listed.has(commits[next].id); next++) {
      owner.set(commits[next].id, merge.id);
    }
  });
  return owner;
}
