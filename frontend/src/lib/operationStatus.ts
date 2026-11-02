import type {
  BisectInfo,
  PickOutcome,
  RebaseAbort,
  RebaseDivergence,
  RebaseInfo,
  SequenceAbort,
  SequenceKind,
  SequenceOutcome,
  SequenceProgress,
  TerminalSequence,
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

/**
 * "Bisecting (started from main)". A bisect started on a detached HEAD names its commit by the
 * short id; an unknown start (no `BISECT_START`) gives just "Bisecting".
 */
export function bisectLabel(bisect: BisectInfo): string {
  const start = /^[0-9a-f]{40}([0-9a-f]{24})?$/.test(bisect.startedFrom)
    ? bisect.startedFrom.slice(0, 7)
    : bisect.startedFrom;
  return start === "" ? "Bisecting" : `Bisecting (started from ${start})`;
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
  // Cancel after a step's commit: not a failure, the commit stands (#183, D9-A).
  if (outcome.kind === "Stopped") return `Stopped after "${outcome.kept}"; its commit was kept. Continue to resume.`;
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

/**
 * The strip label of a cherry-pick/revert started in a terminal (#258): "Cherry-pick paused in a
 * terminal" between steps, "Cherry-picking (started in a terminal)" while a step is paused on a
 * conflict.
 */
export function terminalSequenceLabel(sequence: TerminalSequence): string {
  if (sequence.pausedOnConflict) {
    const verb = sequence.kind === "CherryPick" ? "Cherry-picking" : "Reverting";
    return `${verb} (started in a terminal)`;
  }
  return `${sequence.kind === "CherryPick" ? "Cherry-pick" : "Revert"} paused in a terminal`;
}

/** What to do with a terminal sequence paused between steps. */
export function terminalSequenceText(kind: SequenceKind): string {
  return `Run git ${sequenceNoun(kind)} --continue there to apply the remaining commits.`;
}

/**
 * The confirmation before Abort removes the commits a terminal sequence made (#253, D7-A), like
 * `git cherry-pick --abort`.
 */
export function terminalAbortConfirm(sequence: TerminalSequence): {
  title: string;
  body: string;
  confirmLabel: string;
} {
  const noun = sequenceNoun(sequence.kind);
  // The backend counts up to 1001 (more than 1000).
  const commits =
    sequence.commitsToRemove === 1
      ? "the commit it made"
      : sequence.commitsToRemove > 1000
        ? "more than 1000 commits it made"
        : `the ${sequence.commitsToRemove} commits it made`;
  const at = `${sequence.startedAt.slice(0, 7)} "${sequence.startedAtSummary}"`;
  // Abort refuses rather than overwrite other uncommitted changes (#281 review).
  const kept = sequence.pausedOnConflict
    ? "The paused step's changes and conflict resolutions are discarded; other uncommitted changes stay."
    : "Uncommitted changes stay.";
  return {
    title: `Abort the whole ${noun}?`,
    body: `This also removes ${commits} and puts the branch back at ${at}. ${kept}`,
    confirmLabel: `Abort ${noun}`,
  };
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
  if (info !== null && info.health.kind === "Diverged" && info.health.reason === "OtherOperation") {
    return "Stopped the rebase. Another git operation is in progress, so HEAD was left where it is.";
  }
  return "Stopped the rebase. HEAD had moved, so it was left where it is.";
}

/**
 * The branch the abort toast offers to switch to: after aborting a rebase whose branch moved,
 * HEAD stays on the rebased work, and going back is one (safe) switch away.
 */
export function rebaseAbortSwitchTarget(abort: RebaseAbort, info: RebaseInfo | null): string | null {
  if (abort.restored || info === null) return null;
  return info.health.kind === "Diverged" && info.health.reason === "BranchMoved" ? info.branch : null;
}
