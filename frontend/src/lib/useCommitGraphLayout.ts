import { useMemo, useRef } from "react";
import type { GraphCommit } from "../ipc/RepoClient";
import { type CommitLayout, type LaneLayout, layoutLanes, type RowIndex } from "./commitGraphLayout";
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
  indexById: RowIndex;
  /** How many rows the last update laid out (#837). */
  rowsLaidOut: number;
}

/** The graph's lanes, computed once for the commit list and the sidebar's swatches (#673). */
export function useCommitGraphLayout(
  commits: GraphCommit[],
  expandedMerges: Readonly<Record<string, MergeExpansion>>,
): CommitGraphLayout {
  // The last layout and colours: a page appended to the list lays out only the new rows (#837),
  // and colours that didn't change keep their identity, so the sidebar's swatches don't render.
  const previous = useRef<{ lanes: LaneLayout; branchColors: BranchLaneColors } | null>(null);
  /* eslint-disable react-hooks/refs -- a cache of the last result, read and written only here */
  return useMemo(() => {
    const mergeOwners = expandedFrom(commits, expandedMerges);
    const last = previous.current;
    const lanes = layoutLanes(commits, new Set(mergeOwners.keys()), last?.lanes ?? null);
    const colors = branchLaneColors(commits, lanes.layouts);
    const branchColors = last !== null && sameColors(last.branchColors, colors) ? last.branchColors : colors;
    previous.current = { lanes, branchColors };
    return {
      layouts: lanes.layouts,
      laneCount: lanes.laneCount,
      mergeOwners,
      branchColors,
      indexById: lanes.indexById,
      rowsLaidOut: lanes.rowsLaidOut,
    };
  }, [commits, expandedMerges]);
  /* eslint-enable react-hooks/refs */
}

function sameMap(a: ReadonlyMap<string, string>, b: ReadonlyMap<string, string>): boolean {
  if (a.size !== b.size) return false;
  for (const [key, value] of a) if (b.get(key) !== value) return false;
  return true;
}

function sameColors(a: BranchLaneColors, b: BranchLaneColors): boolean {
  return sameMap(a.local, b.local) && sameMap(a.remote, b.remote);
}
