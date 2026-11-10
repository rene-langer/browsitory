import type { BranchInfo, UpstreamInfo } from "../../ipc/RepoClient";
import type { OperationBlockers } from "../operationStatus";
import { gate, type MenuAction, type PaletteAction } from "./action";

export interface BranchActionContext {
  /** `operationBlockers` of the current state. */
  blockers: OperationBlockers;
  /** How many remotes are configured. */
  remoteCount: number;
  /** The current branch's upstream. */
  upstream: UpstreamInfo | null;
}

export interface BranchActions {
  /** Switch to the branch: the menu's "Checkout", the palette's "Switch to <name>". */
  checkout: PaletteAction;
  rename: MenuAction;
  /** Only for a branch other than the current one. */
  merge: PaletteAction;
  /** The current branch's "Push to <remote>", `null` without an upstream or on another branch. */
  pushToUpstream: MenuAction | null;
  /** The current branch's "Set upstream…", `null` on another branch. */
  setUpstream: MenuAction | null;
  /** Opens the Push dialog; the palette offers it for the current branch ("Push branch to…"). */
  pushTo: PaletteAction;
  delete: MenuAction;
}

export const CURRENT_BRANCH = "Already the current branch.";
export const NO_REMOTE = "No remote configured.";

/**
 * Availability, reason and labels of a local branch's actions (#509), shared by `BranchTree`'s
 * branch menu (and its double-click) and the command palette. Every one follows the one busy rule
 * (`operationBlockers`): switching, renaming or deleting a branch under a merge, a rebase or a
 * cherry-pick in progress would retarget or lose the branch that operation finishes on.
 */
export function branchActions(context: BranchActionContext, branch: BranchInfo): BranchActions {
  const { blockers } = context;
  const { name, isCurrent } = branch;
  const upstream = isCurrent ? context.upstream : null;
  return {
    checkout: { label: "Checkout", paletteLabel: `Switch to ${name}`, ...gate(blockers, isCurrent ? CURRENT_BRANCH : undefined) },
    rename: { label: "Rename", ...gate(blockers) },
    merge: {
      label: "Merge into current branch…",
      paletteLabel: `Merge ${name} into current branch…`,
      ...gate(blockers, isCurrent ? CURRENT_BRANCH : undefined),
    },
    pushToUpstream: upstream === null ? null : { label: `Push to ${upstream.remoteName}`, ...gate(blockers) },
    setUpstream: isCurrent ? { label: "Set upstream…", ...gate(blockers) } : null,
    pushTo: {
      label: "Push to…",
      paletteLabel: "Push branch to…",
      ...gate(blockers, context.remoteCount === 0 ? NO_REMOTE : undefined),
    },
    delete: { label: "Delete", ...gate(blockers) },
  };
}

export interface RemoteBranchActions {
  /** Switches to the same-named local branch, or creates it tracking this one. */
  checkout: MenuAction;
  setAsUpstream: MenuAction;
  delete: MenuAction;
}

/**
 * A remote-tracking branch's actions in `BranchTree` (#509). The palette has no per-branch
 * commands for them (the rows load lazily); it offers "Browse branches on <remote>" and "Delete
 * remote branch…" under the same busy rule.
 */
export function remoteBranchActions(
  blockers: OperationBlockers,
  remoteName: string,
  branchName: string,
): RemoteBranchActions {
  return {
    checkout: { label: "Checkout", ...gate(blockers) },
    setAsUpstream: { label: "Set as upstream for current branch", ...gate(blockers) },
    delete: { label: `Delete ${remoteName}/${branchName}…`, ...gate(blockers) },
  };
}
