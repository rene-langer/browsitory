// Used by CommitLaneGraphic.tsx (branch-lane graphic) and BranchTree's swatches. Mid-tone,
// slightly muted hues so the graph reads on both the light and dark palettes without shouting.
export const LANE_COLORS = ["#e8833a", "#6aa84f", "#4f86c6", "#a66bb5", "#d9534f", "#3a9e9e"];

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
