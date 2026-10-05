import type { GraphCommit } from "../ipc/RepoClient";

// Which graph commits are selected for a multi-commit action. `anchorId` is the pivot for
// Shift-extend; it is always a member of `ids` unless `ids` is empty.
export interface CommitSelection {
  ids: ReadonlySet<string>;
  anchorId: string | null;
}

export const EMPTY_SELECTION: CommitSelection = { ids: new Set(), anchorId: null };

export function selectOnly(id: string): CommitSelection {
  return { ids: new Set([id]), anchorId: id };
}

export function toggleCommit(selection: CommitSelection, id: string): CommitSelection {
  const ids = new Set(selection.ids);
  if (ids.has(id)) {
    ids.delete(id);
    if (ids.size === 0) return EMPTY_SELECTION;
    const anchorId =
      selection.anchorId !== null && ids.has(selection.anchorId)
        ? selection.anchorId
        : [...ids][0];
    return { ids, anchorId };
  }
  ids.add(id);
  return { ids, anchorId: id };
}

// Replaces the selection with every commit between the anchor and `id` (inclusive), in graph
// order. Without a usable anchor it degrades to a plain single selection.
export function extendSelection(
  selection: CommitSelection,
  commits: GraphCommit[],
  id: string,
): CommitSelection {
  const anchorIndex = commits.findIndex((commit) => commit.id === selection.anchorId);
  const targetIndex = commits.findIndex((commit) => commit.id === id);
  if (anchorIndex === -1 || targetIndex === -1) return selectOnly(id);
  const start = Math.min(anchorIndex, targetIndex);
  const end = Math.max(anchorIndex, targetIndex);
  return {
    ids: new Set(commits.slice(start, end + 1).map((commit) => commit.id)),
    anchorId: selection.anchorId,
  };
}
