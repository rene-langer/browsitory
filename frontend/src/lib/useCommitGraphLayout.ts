import { useMemo } from "react";
import type { GraphCommit } from "../ipc/RepoClient";
import { type CommitLayout, layoutLanes } from "./commitGraphLayout";
import { type BranchLaneColors, branchLaneColors } from "./laneColors";
import { expandedFrom, type MergeExpansion } from "./mergeExpansion";

export interface CommitGraphLayout {
  /** One per commit, in order. */
  layouts: CommitLayout[];
  /** How many lanes the layouts draw, computed once per layout instead of on every render. */
  laneCount: number;
  /** The merge each spliced-in commit of an expanded merge belongs to (#617). */
  mergeOwners: Map<string, string>;
  /** Each branch tip's lane colour (#673). */
  branchColors: BranchLaneColors;
  /** Each listed commit's row index (#837). */
  indexById: ReadonlyMap<string, number>;
  /** How many rows the last update laid out (#837). */
  rowsLaidOut: number;
}

/** The graph's lanes, computed once for the commit list and the sidebar's swatches (#673). */
export function useCommitGraphLayout(
  commits: GraphCommit[],
  expandedMerges: Readonly<Record<string, MergeExpansion>>,
): CommitGraphLayout {
  return useMemo(() => {
    const mergeOwners = expandedFrom(commits, expandedMerges);
    const lanes = layoutLanes(commits, new Set(mergeOwners.keys()), null);
    return {
      layouts: lanes.layouts,
      laneCount: lanes.laneCount,
      mergeOwners,
      branchColors: branchLaneColors(commits, lanes.layouts),
      indexById: lanes.indexById,
      rowsLaidOut: lanes.rowsLaidOut,
    };
  }, [commits, expandedMerges]);
}
