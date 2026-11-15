import type { BranchInfo, GraphCommit } from "../../ipc/RepoClient";
import type { OperationBlockers } from "../operationStatus";
import type { RebaseTarget } from "./branchActions";
import { gate, UNAVAILABLE_REASON, type PaletteAction } from "./action";

/**
 * One action on a commit, as both the commit menu ("Cherry-pick onto main") and the command
 * palette ("Cherry-pick abc1234 onto main") offer it.
 */
export type CommitAction = PaletteAction;

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
  /** Opens the rebase planner onto the commit (#991). */
  rebaseOnto: CommitAction;
  /**
   * Opens the rebase planner from the commit (#991): onto its parent (`onto`, `null` for a merge
   * or root commit, which it refuses), the commit itself the plan's first Pick.
   */
  interactiveRebase: CommitAction & { onto: string | null };
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
/** The rule `planRebaseSelection` applies to a multi-selection, for one commit (#991). */
const NO_REBASE_BASE = "Merge and root commits can't be rebased.";

/**
 * Availability, reason and labels of the single-commit actions (#495), shared by `CommitGraph`'s
 * commit menu and the command palette so mouse and keyboard users get the same answer. The
 * palette's label is the menu's with the commit for "here" or added after the verb (#990).
 * Cherry-pick and Revert say where their commit lands (#1020).
 */
export function commitActions(context: CommitActionContext, commit: GraphCommit): CommitActions {
  const { blockers } = context;
  const short = commit.shortId;
  const isHead = context.headId === commit.id;
  const needsParent = commit.parentIds.length > 1;
  const ellipsis = needsParent ? "…" : "";
  const target = context.currentBranch ?? "HEAD";
  const action = (label: string, paletteLabel: string, refusal?: string): CommitAction => ({
    label,
    paletteLabel,
    ...gate(blockers, refusal),
  });
  const onto = commit.parentIds.length === 1 ? commit.parentIds[0] : null;
  return {
    // Only opens the dialog, whose Create button carries the busy rule and its reason.
    createTag: { label: "Create tag…", paletteLabel: `Create tag at ${short}…`, enabled: true, reason: undefined },
    cherryPick: {
      ...action(
        `Cherry-pick onto ${target}${ellipsis}`,
        `Cherry-pick ${short} onto ${target}${ellipsis}`,
        isHead ? HEAD_PICK : undefined,
      ),
      needsParent,
    },
    revert: {
      ...action(`Revert (new commit on ${target})${ellipsis}`, `Revert ${short} (new commit on ${target})${ellipsis}`),
      needsParent,
    },
    checkout: action("Switch to (detached)", `Switch to ${short} (detached)`),
    reset: action(
      `Reset ${target} to here…`,
      `Reset ${target} to ${short}…`,
      context.currentBranch === null ? NO_BRANCH : undefined,
    ),
    rebaseOnto: action("Rebase current branch onto here…", `Rebase current branch onto ${short}…`),
    interactiveRebase: {
      ...action(
        "Interactive rebase from here…",
        `Interactive rebase from ${short}…`,
        onto === null ? NO_REBASE_BASE : undefined,
      ),
      onto,
    },
  };
}

/** What the menu of commits dropped on a branch runs (#744): the commit menu's own Cherry-pick. */
export type CommitDropRun =
  | { kind: "pick"; commitId: string }
  /** A merge commit: the parent picker, as the menu's "Cherry-pick…". */
  | { kind: "choose-parent"; commitId: string }
  /** Several commits, oldest first, as the multi-selection's "Cherry-pick N commits". */
  | { kind: "pick-many"; commitIds: string[] };

/**
 * What commits dragged onto a branch do (#744): `menu` holds the one choice, `refused` says why
 * the drop can't run.
 */
export type CommitDrop =
  | { kind: "menu"; item: PaletteAction & { run: CommitDropRun } }
  | { kind: "refused"; reason: string };

/** The commits of a drag from the graph (oldest first) and HEAD at the time. */
export interface DraggedCommits {
  commits: GraphCommit[];
  headId: string | null;
}

export const COMMIT_DROP_NEEDS_BRANCH = "Check out a branch first: a cherry-pick applies to the current branch.";
const PICK_MERGES_ALONE = "Cherry-pick merge commits one at a time.";

/**
 * The choice for `dragged` dropped on `target` in the Branches section (#744). A cherry-pick lands
 * on the current branch, so only it takes the drop. One commit follows the commit menu's
 * Cherry-pick (`commitActions`: the busy rule, HEAD itself refused, a merge commit asks for a
 * parent); several follow the multi-selection's "Cherry-pick N commits" (the busy rule, no merge
 * commits). A disabled choice refuses the drop with its reason.
 */
export function commitDropActions(
  blockers: OperationBlockers,
  branches: BranchInfo[],
  dragged: DraggedCommits,
  target: RebaseTarget,
): CommitDrop {
  const current = branches.find((branch) => branch.isCurrent) ?? null;
  if (current === null) return { kind: "refused", reason: COMMIT_DROP_NEEDS_BRANCH };
  if (target.kind !== "local" || target.name !== current.name) {
    return {
      kind: "refused",
      reason: `Only the current branch, ${current.name}, can take a cherry-pick: drop on ${current.name}.`,
    };
  }
  const { commits } = dragged;
  if (commits.length === 1) {
    const [commit] = commits;
    const pick = commitActions({ blockers, headId: dragged.headId, currentBranch: current.name }, commit).cherryPick;
    if (!pick.enabled) return { kind: "refused", reason: pick.reason ?? UNAVAILABLE_REASON };
    const label = `Cherry-pick ${commit.shortId} onto ${current.name}${pick.needsParent ? "…" : ""}`;
    const run: CommitDropRun = pick.needsParent
      ? { kind: "choose-parent", commitId: commit.id }
      : { kind: "pick", commitId: commit.id };
    return { kind: "menu", item: { label, paletteLabel: label, enabled: true, reason: undefined, run } };
  }
  const hasMerge = commits.some((commit) => commit.parentIds.length > 1);
  const gated = gate(blockers, hasMerge ? PICK_MERGES_ALONE : undefined);
  if (!gated.enabled) return { kind: "refused", reason: gated.reason ?? UNAVAILABLE_REASON };
  const label = `Cherry-pick ${commits.length} commits onto ${current.name}`;
  return {
    kind: "menu",
    item: {
      label,
      paletteLabel: label,
      enabled: true,
      reason: undefined,
      run: { kind: "pick-many", commitIds: commits.map((commit) => commit.id) },
    },
  };
}
