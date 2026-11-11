import type { GraphCommit } from "../ipc/RepoClient";
import type { CommitLayout } from "./commitGraphLayout";

// Used by CommitLaneGraphic.tsx (branch-lane graphic), the branch badges and BranchTree's
// swatches. The colours are the `--color-lane-*` tokens (`styles/tokens.css`), one set per theme,
// each at least 3:1 against the graph's backgrounds (#666).
export const LANE_COLORS = [1, 2, 3, 4, 5, 6].map((lane) => `var(--color-lane-${lane})`);

/** The colour of graph lane `lane`. */
export function laneColor(lane: number): string {
  return LANE_COLORS[lane % LANE_COLORS.length];
}

/** Branch name → the colour of the lane its tip sits on in the graph (#673). */
export interface BranchLaneColors {
  local: ReadonlyMap<string, string>;
  /** `<remote>/<branch>` names. */
  remote: ReadonlyMap<string, string>;
}

export const NO_BRANCH_LANE_COLORS: BranchLaneColors = { local: new Map(), remote: new Map() };

/**
 * The lane colour of every branch whose tip is a row of the graph (#673), so the sidebar's
 * swatches and the row badges show the colour the graph draws the branch in. A branch whose tip
 * isn't loaded (hidden from the graph, older than the loaded pages, inside a collapsed merge) has
 * none. Lane colours follow the layout, so a branch's colour can change when history changes.
 */
export function branchLaneColors(commits: GraphCommit[], layouts: CommitLayout[]): BranchLaneColors {
  const local = new Map<string, string>();
  const remote = new Map<string, string>();
  commits.forEach((commit, index) => {
    const layout = layouts[index];
    if (layout === undefined) return;
    const color = laneColor(layout.lane);
    for (const name of commit.branchRefs) if (!local.has(name)) local.set(name, color);
    for (const name of commit.remoteBranchRefs) if (!remote.has(name)) remote.set(name, color);
  });
  return { local, remote };
}
