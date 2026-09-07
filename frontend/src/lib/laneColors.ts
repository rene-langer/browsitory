// Shared by CommitLaneGraphic.tsx (branch-lane graphic) and LaneBraid.tsx (header
// signature strip) — the header deliberately foreshadows the branch graph using the
// same six hues in the same order.
export const LANE_COLORS = ["#e36209", "#1a7f37", "#0969da", "#8250df", "#cf222e", "#bf8700"];

// A branch's actual lane in CommitGraph is assigned dynamically by the graph layout algorithm
// and shifts as history changes, so BranchTree — which has no access to that layout — instead
// gives each branch name a stable color from the same palette via a simple string hash. This is
// a per-branch identity color, not a live readout of the branch's current graph lane.
export function branchSwatchColor(branchName: string): string {
  let hash = 0;
  for (let i = 0; i < branchName.length; i += 1) {
    hash = (hash * 31 + branchName.charCodeAt(i)) | 0;
  }
  return LANE_COLORS[Math.abs(hash) % LANE_COLORS.length];
}
