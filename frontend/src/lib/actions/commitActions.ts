import type { GraphCommit } from "../../ipc/RepoClient";
import type { OperationBlockers } from "../operationStatus";

/** One action on a commit, as both the commit menu and the command palette offer it. */
export interface CommitAction {
  /** The commit menu's label ("Cherry-pick…"). */
  label: string;
  /** The palette's label, which names the commit ("Cherry-pick abc1234…"). */
  paletteLabel: string;
  /** The menu disables the item, the palette leaves the command out. */
  enabled: boolean;
  /** Why it is disabled, for the menu item's title. */
  reason: string | undefined;
}

/** A merge commit's cherry-pick and revert first ask which parent to apply against. */
export interface PickAction extends CommitAction {
  needsParent: boolean;
}

export interface CommitActions {
  createTag: CommitAction;
  cherryPick: PickAction;
  revert: PickAction;
  checkout: CommitAction;
  reset: CommitAction;
}

export interface CommitActionContext {
  /** `operationBlockers` of the current state. */
  blockers: OperationBlockers;
  headId: string | null;
  /** The checked-out branch, `null` on a detached HEAD. */
  currentBranch: string | null;
}

const HEAD_PICK = "HEAD already contains this commit.";
const NO_BRANCH = "Check out a branch first.";

/**
 * Availability, reason and labels of the single-commit actions (#495), shared by `CommitGraph`'s
 * commit menu and the command palette so mouse and keyboard users get the same answer.
 */
export function commitActions(context: CommitActionContext, commit: GraphCommit): CommitActions {
  const { blockers } = context;
  const busy = blockers.reason ?? undefined;
  const short = commit.shortId;
  const isHead = context.headId === commit.id;
  const needsParent = commit.parentIds.length > 1;
  const ellipsis = needsParent ? "…" : "";
  const action = (label: string, paletteLabel: string, refusal?: string): CommitAction => {
    const reason = blockers.blocked ? busy : refusal;
    return { label, paletteLabel, enabled: !blockers.blocked && refusal === undefined, reason };
  };
  return {
    // Only opens the dialog, whose Create button carries the busy rule and its reason.
    createTag: { label: "Create tag…", paletteLabel: `Create tag at ${short}…`, enabled: true, reason: undefined },
    cherryPick: {
      ...action(`Cherry-pick${ellipsis}`, `Cherry-pick ${short}${ellipsis}`, isHead ? HEAD_PICK : undefined),
      needsParent,
    },
    revert: { ...action(`Revert${ellipsis}`, `Revert ${short}${ellipsis}`), needsParent },
    checkout: action("Checkout (detached)", `Checkout ${short} (detached)`),
    reset: action(
      `Reset ${context.currentBranch ?? "HEAD"} to here…`,
      `Reset to ${short}…`,
      context.currentBranch === null ? NO_BRANCH : undefined,
    ),
  };
}
