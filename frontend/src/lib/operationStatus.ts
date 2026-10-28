import type { PickOutcome, SequenceAbort, SequenceKind, SequenceOutcome, SequenceProgress } from "../ipc/RepoClient";

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

/** Success toast for a finished sequence; `null` while it is paused on a conflict. */
export function sequenceToast(kind: SequenceKind, outcome: SequenceOutcome): string | null {
  if (outcome.kind !== "Completed") return null;
  const commits = `${outcome.applied} ${outcome.applied === 1 ? "commit" : "commits"}`;
  const skipped = outcome.skipped.length > 0 ? `, skipped ${outcome.skipped.length} already applied` : "";
  return `${pastTense(kind)} ${commits}${skipped}`;
}

export function abortToast(kind: SequenceKind, abort: SequenceAbort): string {
  const noun = sequenceNoun(kind);
  return abort.rewound ? `Aborted ${noun}` : `Stopped the ${noun}. HEAD had moved, so it was left where it is.`;
}
