import type { GraphCommit, HeadInfo, RebasePlanCommit, RebasePlanEntry, StatusEntry } from "../ipc/RepoClient";

/**
 * One-click history edits from the commit menu (#442). Each is a one-step interactive rebase:
 * the plan is every commit `commitsSince(onto)` lists, picked, with one entry changed. It runs
 * through `startRebase`, so conflicts pause it like any rebase and it is one Undo step.
 */
export type HistoryEditKind = "Reword" | "Squash" | "Fixup" | "MoveUp" | "MoveDown" | "Drop";

export const HISTORY_EDIT_KINDS: readonly HistoryEditKind[] = ["Reword", "Squash", "Fixup", "MoveUp", "MoveDown", "Drop"];

/** What a one-click edit needs on top of its kind: Reword's new message, Squash's combined one. */
export type HistoryEdit =
  | { kind: "Reword"; message: string }
  | { kind: "Squash"; combinedMessage: string }
  | { kind: "Fixup" }
  | { kind: "MoveUp" }
  | { kind: "MoveDown" }
  | { kind: "Drop" };

/** `onto`: the commit the one-step rebase starts from (passed to `commitsSince`/`startRebase`). */
export type HistoryEditCheck = { ok: true; onto: string } | { ok: false; reason: string };

export interface HistoryEditContext {
  /** The loaded graph, newest first. */
  commits: GraphCommit[];
  headInfo: HeadInfo;
  /** `null` on a detached HEAD. */
  currentBranch: string | null;
  status: StatusEntry[];
}

export const HISTORY_CHANGED = "The history changed. Refresh and try again.";

const NOT_ON_BRANCH = "Only commits of the current branch can be edited.";
const ROOT = "The first commit of the history can't be edited here.";
const PARENT_ROOT = "Its parent is the first commit of the history, which can't be edited here.";
const PARENT_MERGE = "Its parent is a merge commit, which can't be edited here.";

// The reason every edit of the commit is refused, before looking at the single edits.
function commonRefusal(context: HistoryEditContext, commitId: string, byId: Map<string, GraphCommit>): string | null {
  const { headInfo } = context;
  if (
    headInfo.operation !== null ||
    headInfo.rebase !== null ||
    headInfo.sequence !== null ||
    headInfo.terminalRebase !== null ||
    headInfo.terminalSequence !== null
  ) {
    return "Finish or abort the operation in progress first.";
  }
  if (context.currentBranch === null || headInfo.detachedAt !== null || headInfo.headId === null) {
    return "Check out a branch first.";
  }
  // As `start_rebase`: staged changes and unstaged changes to tracked files are in the way;
  // untracked files aren't.
  if (context.status.some((entry) => entry.staged || entry.kind !== "New")) {
    return "Commit or stash your changes first.";
  }
  // The rebase replays HEAD's first-parent history (`commitsSince`), so only commits on it can
  // be edited, and only while no merge commit sits between them and HEAD.
  let mergeAbove = false;
  let current = byId.get(headInfo.headId);
  while (current !== undefined && current.id !== commitId) {
    if (current.parentIds.length > 1) mergeAbove = true;
    current = current.parentIds.length === 0 ? undefined : byId.get(current.parentIds[0]);
  }
  if (current === undefined) return NOT_ON_BRANCH;
  if (current.parentIds.length > 1) return "Merge commits can't be edited here.";
  if (mergeAbove) return "A merge commit comes after this one. Use Rebase onto here… instead.";
  if (current.parentIds.length === 0) return ROOT;
  return null;
}

/** Whether each one-click edit is possible on `commitId`, and from which commit it rebases. */
export function historyEditChecks(context: HistoryEditContext, commitId: string): Record<HistoryEditKind, HistoryEditCheck> {
  const byId = new Map(context.commits.map((commit) => [commit.id, commit]));
  const refusal = commonRefusal(context, commitId, byId);
  if (refusal !== null) {
    const refused: HistoryEditCheck = { ok: false, reason: refusal };
    return { Reword: refused, Squash: refused, Fixup: refused, MoveUp: refused, MoveDown: refused, Drop: refused };
  }
  const parentId = byId.get(commitId)!.parentIds[0];
  const parent = byId.get(parentId);
  // Edits that rewrite the parent too start from the grandparent.
  const fromGrandparent: HistoryEditCheck =
    parent === undefined
      ? { ok: false, reason: "Load more history to edit this commit with its parent." }
      : parent.parentIds.length === 0
        ? { ok: false, reason: PARENT_ROOT }
        : parent.parentIds.length > 1
          ? { ok: false, reason: PARENT_MERGE }
          : { ok: true, onto: parent.parentIds[0] };
  const fromParent: HistoryEditCheck = { ok: true, onto: parentId };
  return {
    Reword: fromParent,
    Squash: fromGrandparent,
    Fixup: fromGrandparent,
    MoveUp: context.headInfo.headId === commitId ? { ok: false, reason: "This is already the newest commit." } : fromParent,
    MoveDown: fromGrandparent,
    Drop: fromParent,
  };
}

/**
 * The full plan for `edit` of `commitId`, from `span` (`commitsSince(onto)`, oldest first).
 * Edits based on the parent expect the commit first in the span, edits based on the
 * grandparent second; anything else means the history changed since the menu was built.
 */
export function buildHistoryEditPlan(edit: HistoryEdit, commitId: string, span: RebasePlanCommit[]): RebasePlanEntry[] {
  const index = span.findIndex((commit) => commit.id === commitId);
  const fromGrandparent = edit.kind === "Squash" || edit.kind === "Fixup" || edit.kind === "MoveDown";
  if (index !== (fromGrandparent ? 1 : 0) || (edit.kind === "MoveUp" && index + 1 >= span.length)) {
    throw new Error(HISTORY_CHANGED);
  }
  const plan: RebasePlanEntry[] = span.map((commit) => ({
    commitId: commit.id,
    action: { kind: "Pick" },
    combinedMessage: null,
  }));
  const swap = (a: number, b: number) => {
    [plan[a], plan[b]] = [plan[b], plan[a]];
  };
  switch (edit.kind) {
    case "Reword":
      plan[index] = { ...plan[index], action: { kind: "Reword", message: edit.message } };
      break;
    case "Squash":
      plan[index - 1] = { ...plan[index - 1], combinedMessage: edit.combinedMessage };
      plan[index] = { ...plan[index], action: { kind: "Squash" } };
      break;
    case "Fixup":
      // No combined message: the group keeps the parent's message, as `git rebase` fixup does.
      plan[index] = { ...plan[index], action: { kind: "Fixup" } };
      break;
    case "MoveUp":
      swap(index, index + 1);
      break;
    case "MoveDown":
      swap(index - 1, index);
      break;
    case "Drop":
      plan[index] = { ...plan[index], action: { kind: "Drop" } };
      break;
  }
  return plan;
}

/** git's squash message without the comment lines: the parent's message, then the commit's. */
export function squashMessage(parentMessage: string, message: string): string {
  return `${parentMessage.trimEnd()}\n\n${message.trimEnd()}\n`;
}

/**
 * What the commit menu and the palette ask for: Squash's combined message is built when the edit
 * runs (from both full messages), so it isn't part of the request.
 */
export type HistoryEditRequest = Exclude<HistoryEdit, { kind: "Squash" }> | { kind: "Squash" };

/** The toast once a one-click edit finished without pausing. */
export function historyEditDoneMessage(kind: HistoryEditKind, shortId: string): string {
  switch (kind) {
    case "Reword":
      return `Reworded ${shortId}`;
    case "Squash":
      return `Squashed ${shortId} into its parent`;
    case "Fixup":
      return `Fixed up ${shortId} into its parent`;
    case "MoveUp":
      return `Moved ${shortId} up`;
    case "MoveDown":
      return `Moved ${shortId} down`;
    case "Drop":
      return `Dropped ${shortId}`;
  }
}
