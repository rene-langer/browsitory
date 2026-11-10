// Used by CommitLaneGraphic.tsx (branch-lane graphic) and BranchTree's swatches. The colours are
// the `--color-lane-*` tokens (`styles/tokens.css`), one set per theme, each at least 3:1 against
// the graph's backgrounds (#666).
export const LANE_COLORS = [1, 2, 3, 4, 5, 6].map((lane) => `var(--color-lane-${lane})`);

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
