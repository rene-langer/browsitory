import type { BranchInfo, UpstreamInfo } from "../../ipc/RepoClient";
import type { OperationBlockers } from "../operationStatus";
import { gate, UNAVAILABLE_REASON, type MenuAction, type PaletteAction } from "./action";

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

/** A branch to rebase the current branch onto (#619). */
export type RebaseTarget =
  | { kind: "local"; name: string }
  | { kind: "remote"; remoteName: string; branchName: string };

/**
 * "Rebase current branch onto <name>…" (#619), in the local and remote-tracking branch menus and,
 * for local branches, the palette. `onto` is the branch's full ref name, so a branch named like a
 * commit id or a tag can't be misread. Absent (`null`) without a current branch (a detached HEAD):
 * there is no branch to rebase. Refused on the current branch itself, and under the one busy rule,
 * like Merge.
 */
export function rebaseOntoAction(
  blockers: OperationBlockers,
  currentBranch: string | null,
  target: RebaseTarget,
): (PaletteAction & { onto: string }) | null {
  if (currentBranch === null) return null;
  const isLocal = target.kind === "local";
  const name = isLocal ? target.name : `${target.remoteName}/${target.branchName}`;
  const onto = isLocal ? `refs/heads/${target.name}` : `refs/remotes/${target.remoteName}/${target.branchName}`;
  const label = `Rebase current branch onto ${name}…`;
  return {
    label,
    paletteLabel: label,
    onto,
    ...gate(blockers, isLocal && target.name === currentBranch ? CURRENT_BRANCH : undefined),
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

/** What one item of a branch drop's menu runs: the menus' Merge or Rebase (#744). */
export type BranchDropRun = { kind: "merge"; name: string } | { kind: "rebase"; onto: string };

/**
 * What a branch dropped on a branch does (#744): `menu` lists the choices, `refused` says why the
 * drop can't run, `none` is a branch dropped on itself.
 */
export type BranchDrop =
  | { kind: "menu"; items: (PaletteAction & { run: BranchDropRun })[] }
  | { kind: "refused"; reason: string }
  | { kind: "none" };

export const DROP_NEEDS_BRANCH = "Check out a branch first: a drop merges into or rebases the current branch.";

function displayName(ref: RebaseTarget): string {
  return ref.kind === "local" ? ref.name : `${ref.remoteName}/${ref.branchName}`;
}

function sameRef(a: RebaseTarget, b: RebaseTarget): boolean {
  return displayName(a) === displayName(b) && a.kind === b.kind;
}

/**
 * The choices for `source` dropped on `target` in the Branches section (#744), from the menus' own
 * rules: Merge (`branchActions`' `merge`) and Rebase (`rebaseOntoAction`) under labels naming both
 * branches. Both act on the current branch, so one side of the drop must be it: a branch dropped
 * on it is merged into it (or it is rebased onto that branch), and it dropped on a branch is
 * rebased onto that branch (or that branch is merged into it), the natural choice first. A remote
 * branch can't be merged from the menus, so with one only Rebase is offered. When every choice is
 * disabled (the busy rule), the drop is refused with that reason.
 */
export function branchDropActions(
  blockers: OperationBlockers,
  branches: BranchInfo[],
  source: RebaseTarget,
  target: RebaseTarget,
): BranchDrop {
  if (sameRef(source, target)) return { kind: "none" };
  const current = branches.find((branch) => branch.isCurrent) ?? null;
  if (current === null) return { kind: "refused", reason: DROP_NEEDS_BRANCH };
  const isCurrent = (ref: RebaseTarget) => ref.kind === "local" && ref.name === current.name;
  if (!isCurrent(source) && !isCurrent(target)) {
    return {
      kind: "refused",
      reason: `Only the current branch, ${current.name}, can be merged into or rebased: drop on ${current.name}, or drag ${current.name}.`,
    };
  }
  const draggingCurrent = isCurrent(source);
  const other = draggingCurrent ? target : source;
  const name = displayName(other);
  const items: (PaletteAction & { run: BranchDropRun })[] = [];
  const rebase = rebaseOntoAction(blockers, current.name, other);
  if (rebase !== null) {
    const label = `Rebase ${current.name} onto ${name}…`;
    items.push({ label, paletteLabel: label, enabled: rebase.enabled, reason: rebase.reason, run: { kind: "rebase", onto: rebase.onto } });
  }
  const otherBranch = other.kind === "local" ? branches.find((branch) => branch.name === other.name) : undefined;
  if (otherBranch !== undefined) {
    const merge = branchActions({ blockers, remoteCount: 0, upstream: null }, otherBranch).merge;
    const label = `Merge ${name} into ${current.name}…`;
    const item = { label, paletteLabel: label, enabled: merge.enabled, reason: merge.reason, run: { kind: "merge" as const, name } };
    if (draggingCurrent) items.push(item);
    else items.unshift(item);
  }
  const enabled = items.find((item) => item.enabled);
  if (enabled === undefined) return { kind: "refused", reason: items[0]?.reason ?? UNAVAILABLE_REASON };
  return { kind: "menu", items };
}
