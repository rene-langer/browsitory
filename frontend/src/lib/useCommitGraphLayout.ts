import { useMemo } from "react";
import type { GraphCommit } from "../ipc/RepoClient";
import { assignLanes, type CommitLayout } from "./commitGraphLayout";
import { type BranchLaneColors, branchLaneColors } from "./laneColors";
import { expandedFrom, type MergeExpansion } from "./mergeExpansion";

export interface CommitGraphLayout {
  /** One per commit, in order. */
  layouts: CommitLayout[];
  /** The merge each spliced-in commit of an expanded merge belongs to (#617). */
  mergeOwners: Map<string, string>;
  /** Each branch tip's lane colour (#673). */
  branchColors: BranchLaneColors;
}

/** The graph's lanes, computed once for the commit list and the sidebar's swatches (#673). */
export function useCommitGraphLayout(
  commits: GraphCommit[],
  expandedMerges: Readonly<Record<string, MergeExpansion>>,
): CommitGraphLayout {
  return useMemo(() => {
    const mergeOwners = expandedFrom(commits, expandedMerges);
    const layouts = assignLanes(commits, new Set(mergeOwners.keys()));
    return { layouts, mergeOwners, branchColors: branchLaneColors(commits, layouts) };
  }, [commits, expandedMerges]);
}
