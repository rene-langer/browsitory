import type { GraphCommit, GraphPage } from "../ipc/RepoClient";

/**
 * The rows a refresh answer makes of the rows the client sent as `loaded` (#835): the layout's
 * segments in order, each the next of the page's `commits` or a run of `loaded`, then the new
 * badges of kept rows whose branches or tags changed. Without `refresh` the page's `commits` are
 * the rows. `loaded` itself is left as it is.
 */
export function applyGraphRefresh(loaded: GraphCommit[], page: GraphPage): GraphCommit[] {
  const refresh = page.refresh;
  if (refresh === null) return page.commits;
  const rows: GraphCommit[] = [];
  let next = 0;
  for (const segment of refresh.layout) {
    if (segment.kind === "New") {
      rows.push(...page.commits.slice(next, next + segment.count));
      next += segment.count;
    } else {
      rows.push(...loaded.slice(segment.from, segment.from + segment.count));
    }
  }
  const badges = new Map(refresh.badges.map((b) => [b.id, b]));
  if (badges.size === 0) return rows;
  return rows.map((row) => {
    const now = badges.get(row.id);
    return now === undefined
      ? row
      : { ...row, branchRefs: now.branchRefs, remoteBranchRefs: now.remoteBranchRefs, tags: now.tags };
  });
}
