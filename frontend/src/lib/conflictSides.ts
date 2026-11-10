import type { BranchInfo, HeadInfo } from "../ipc/RepoClient";

/** What the two sides of a conflict block are, for its labels; `null` when unknown. */
export interface ConflictSideNames {
  ours: string | null;
  theirs: string | null;
}

const MERGE_MESSAGE = /^Merge (?:remote-tracking branch|branch|commit|tag) '([^']+)'/;

/**
 * Names "ours" (stage 2) and "theirs" (stage 3) of a conflicted file (#654). A merge, pick or
 * revert has HEAD's branch as ours; a rebase replays the rebased branch's commits onto its base,
 * so ours is the base and theirs the rebased branch.
 */
export function conflictSideNames(
  head: HeadInfo,
  branches: BranchInfo[],
  mergeMessage: string | null,
): ConflictSideNames {
  const rebase = head.rebase ?? head.terminalRebase;
  if (rebase !== null && rebase.branch !== "") {
    return { ours: rebase.onto === "" ? null : `onto ${rebase.onto.slice(0, 7)}`, theirs: rebase.branch };
  }
  const current = branches.find((branch) => branch.isCurrent)?.name;
  const detached = head.detachedAt ?? head.headId;
  const ours = current ?? (detached === null ? null : detached.slice(0, 7));
  switch (head.operation) {
    case "Merge":
      return { ours, theirs: MERGE_MESSAGE.exec(mergeMessage ?? "")?.[1] ?? "incoming" };
    case "CherryPick":
      return { ours, theirs: "picked commit" };
    case "Revert":
      return { ours, theirs: "revert" };
    case null:
      return { ours, theirs: null };
  }
}
