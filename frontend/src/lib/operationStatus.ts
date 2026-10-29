import type {
  PickOutcome,
  RebaseAbort,
  RebaseDivergence,
  RebaseInfo,
  SequenceAbort,
  SequenceKind,
  SequenceOutcome,
  SequenceProgress,
} from "../ipc/RepoClient";

/** Visible reason a merge/rebase step is blocked, or `undefined` when nothing blocks it. */
export function conflictReason(conflictCount: number): string | undefined {
  if (conflictCount <= 0) return undefined;
  return `Resolve ${conflictCount} ${conflictCount === 1 ? "conflict" : "conflicts"} to continue`;
}

/** "Cherry-picking 2/5" / "Reverting 2/5". */
export function sequenceLabel(progress: SequenceProgress): string {
  const verb = progress.kind === "CherryPick" ? "Cherry-picking" : "Reverting";
  return `${verb} ${progress.currentStep}/${progress.totalSteps}`;
}

/** The noun the UI uses for a sequence: "cherry-pick" / "revert". */
export function sequenceNoun(kind: SequenceKind): string {
  return kind === "CherryPick" ? "cherry-pick" : "revert";
}

function pastTense(kind: SequenceKind): string {
  return kind === "CherryPick" ? "Cherry-picked" : "Reverted";
}

/** Success toast for a single pick; `null` for a conflict (the status strip says it). */
export function pickToast(kind: SequenceKind, outcome: PickOutcome): string | null {
  return outcome.kind === "Committed" ? pastTense(kind) : null;
}

/**
 * Result toast for a finished sequence; `null` while it is paused on a conflict. Skipped steps
 * came out empty, either because the branch already had their changes or because a conflict
 * was resolved to nothing, so they are "with no changes", not "already applied".
 */
export function sequenceToast(kind: SequenceKind, outcome: SequenceOutcome): string | null {
  if (outcome.kind !== "Completed") return null;
  const count = (n: number) => `${n} ${n === 1 ? "commit" : "commits"}`;
  const skipped = outcome.skipped.length;
  if (outcome.applied === 0) {
    return `Nothing to ${sequenceNoun(kind)}: ${count(skipped)} had no changes`;
  }
  const suffix = skipped > 0 ? `, skipped ${skipped} with no changes` : "";
  return `${pastTense(kind)} ${count(outcome.applied)}${suffix}`;
}

export function abortToast(kind: SequenceKind, abort: SequenceAbort): string {
  const noun = sequenceNoun(kind);
  return abort.rewound ? `Aborted ${noun}` : `Stopped the ${noun}. HEAD had moved, so it was left where it is.`;
}

/** Why a stored rebase can only be aborted: what changed outside Browsitory while it was paused. */
export function rebaseDivergenceText(reason: RebaseDivergence, branch: string): string {
  switch (reason) {
    case "HeadMoved":
      return "HEAD was moved outside Browsitory.";
    case "BranchMoved":
      return `${branch} was changed outside Browsitory.`;
    case "BranchMissing":
      return `${branch} was deleted outside Browsitory.`;
    case "OtherOperation":
      return "Another git operation is in progress.";
  }
}

/** The divergence text of an interrupted rebase, `undefined` while it is simply paused. */
export function rebaseInterruption(rebase: RebaseInfo | null): string | undefined {
  if (rebase === null || rebase.health.kind !== "Diverged") return undefined;
  return rebaseDivergenceText(rebase.health.reason, rebase.branch);
}

/** Result toast for Abort rebase; `info` is the rebase as it was before the abort. */
export function rebaseAbortToast(abort: RebaseAbort, info: RebaseInfo | null): string {
  if (abort.restored) return "Aborted rebase";
  if (info !== null && info.health.kind === "Diverged" && info.health.reason === "BranchMissing") {
    return `Stopped the rebase. ${info.branch} no longer exists, so HEAD was left detached.`;
  }
  if (info !== null && info.health.kind === "Diverged" && info.health.reason === "BranchMoved") {
    return `Stopped the rebase. ${info.branch} had changed, so HEAD was left where it is.`;
  }
  return "Stopped the rebase. HEAD had moved, so it was left where it is.";
}
