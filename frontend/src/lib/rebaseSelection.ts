import type { GraphCommit } from "../ipc/RepoClient";
import { isSquashableRange } from "./commitGraphLayout";

// Actions a graph multi-select can preset in the rebase planner. "Pick" is the planner's own
// default, so it is never preset.
export type PresetAction = "Squash" | "Fixup" | "Drop";

export interface RebaseSelection {
  // Parent of the oldest selected commit: the base the planner rebases onto.
  onto: string;
  // True when the selection is one unbroken first-parent run. Squash/Fixup fold into the plan
  // entry *before* them, so they only mean "squash these together" for such a run.
  contiguous: boolean;
  idsOldestFirst: string[];
}

export type RebaseSelectionResult =
  | { ok: true; selection: RebaseSelection }
  | { ok: false; reason: string };

export function planRebaseSelection(
  commits: GraphCommit[],
  selectedIds: ReadonlySet<string>,
): RebaseSelectionResult {
  // `commits` is newest-first, so a larger index is an older commit.
  const indices: number[] = [];
  commits.forEach((commit, index) => {
    if (selectedIds.has(commit.id)) indices.push(index);
  });
  if (indices.length < 2) return { ok: false, reason: "Select two or more commits." };

  if (indices.some((index) => commits[index].parentIds.length !== 1)) {
    return { ok: false, reason: "Merge and root commits can't be rebased." };
  }

  const newest = indices[0];
  const oldest = indices[indices.length - 1];
  return {
    ok: true,
    selection: {
      onto: commits[oldest].parentIds[0],
      contiguous:
        indices.length === oldest - newest + 1 && isSquashableRange(commits, newest, oldest),
      idsOldestFirst: [...indices].reverse().map((index) => commits[index].id),
    },
  };
}

export function presetForAction(
  selection: RebaseSelection,
  action: PresetAction,
): Map<string, PresetAction> {
  // The oldest selected commit is the first entry of the plan; Squash/Fixup there have no
  // predecessor to fold into (the planner disables them on row 0), so it stays a Pick and the
  // rest fold into it.
  const ids = action === "Drop" ? selection.idsOldestFirst : selection.idsOldestFirst.slice(1);
  return new Map(ids.map((id) => [id, action]));
}
