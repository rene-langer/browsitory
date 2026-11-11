import type {
  BisectInfo,
  HeadInfo,
  OperationKind,
  PickOutcome,
  RebaseAbort,
  RebaseDivergence,
  RebaseInfo,
  SequenceAbort,
  SequenceKind,
  SequenceOutcome,
  SequenceProgress,
  TerminalRebase,
  TerminalSequence,
} from "../ipc/RepoClient";
import { type TransferSubject, transferBusyReason } from "./transferCopy";

/** Visible reason a merge/rebase step is blocked, or `undefined` when nothing blocks it. */
/**
 * What a merge merges, from git's `MERGE_MSG` (#665): "Merge branch 'feature/x'" → "feature/x";
 * also a remote-tracking branch, a tag or a commit. `null` when the message names none (a
 * message the user edited, or an octopus merge of several branches).
 */
export function mergeSource(mergeMessage: string | null): string | null {
  if (mergeMessage === null) return null;
  const match = /^Merge (?:remote-tracking branch|branch|tag|commit) '([^']+)'/.exec(mergeMessage.trimStart());
  return match === null ? null : match[1];
}

/** The strip's "Merging feature/x into main" (#665), with whatever of the two is known. */
export function mergeContext(mergeMessage: string | null, currentBranch: string | null): string | null {
  const source = mergeSource(mergeMessage);
  if (source !== null && currentBranch !== null) return `${source} into ${currentBranch}`;
  if (source !== null) return source;
  return currentBranch === null ? null : `into ${currentBranch}`;
}

export function conflictReason(conflictCount: number): string | undefined {
  if (conflictCount <= 0) return undefined;
  return `Resolve ${conflictCount} ${conflictCount === 1 ? "conflict" : "conflicts"} to continue`;
}

/**
 * A leftover merge, cherry-pick or revert (`HeadInfo.operationLeftover`, #240): its changes are
 * already in HEAD, either because its own commit was made and git's state stayed behind, or
 * because a duplicate pick in a terminal stopped as empty.
 */
function leftoverNoun(kind: OperationKind): string {
  return kind === "CherryPick" ? "cherry-pick" : kind === "Revert" ? "revert" : "merge";
}

/** The strip label of a leftover: "Cherry-pick already applied". */
export function leftoverLabel(kind: OperationKind): string {
  const noun = leftoverNoun(kind);
  return `${noun.charAt(0).toUpperCase()}${noun.slice(1)} already applied`;
}

/** The strip text of a leftover. */
export function leftoverText(kind: OperationKind): string {
  return `This ${leftoverNoun(kind)}'s changes are already in the last commit. Git's state for it was left behind. Your next commit or amend clears it, and so does Abort.`;
}

/** Why repository actions are disabled while a leftover is there. */
export function leftoverReason(kind: OperationKind): string {
  return `This ${leftoverNoun(kind)}'s changes are already in the last commit. Commit, amend or abort it first.`;
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

/**
 * Success toast for a single pick of `commitId` ("Cherry-picked 1234567"); `null` for a conflict
 * (the status strip says it).
 */
export function pickToast(kind: SequenceKind, outcome: PickOutcome, commitId: string): string | null {
  return outcome.kind === "Committed" ? `${pastTense(kind)} ${commitId.slice(0, 7)}` : null;
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

/** A finished sequence that committed something is one undo step, so its toast offers Undo. */
export function sequenceUndoable(outcome: SequenceOutcome): boolean {
  return outcome.kind === "Completed" && outcome.applied > 0;
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

/**
 * The strip label of a rebase or `git am` started in a terminal (#263): "Rebasing topic, step 2 of
 * 3 (started in a terminal)", "Applying patches, 1 of 3 (started in a terminal)".
 */
export function terminalRebaseLabel(rebase: TerminalRebase): string {
  const known = rebase.totalSteps > 0 && rebase.currentStep > 0;
  if (rebase.kind === "Am") {
    const step = known ? `, ${rebase.currentStep} of ${rebase.totalSteps}` : "";
    return `Applying patches${step} (started in a terminal)`;
  }
  const branch = rebase.branch === "" ? "" : ` ${rebase.branch}`;
  const step = known ? `, step ${rebase.currentStep} of ${rebase.totalSteps}` : "";
  return `Rebasing${branch}${step} (started in a terminal)`;
}

/** What to do with a terminal rebase or `git am`: it is continued or aborted in the terminal. */
export function terminalRebaseText(rebase: TerminalRebase, conflictCount: number): string {
  const command = rebase.kind === "Am" ? "git am" : "git rebase";
  if (conflictCount > 0) {
    const conflicts = `${conflictCount} ${conflictCount === 1 ? "conflict" : "conflicts"}`;
    return `Resolve ${conflicts}, then run ${command} --continue in the terminal.`;
  }
  return `Continue or abort it there with ${command} --continue or ${command} --abort.`;
}

/** Why repository actions are disabled during a terminal rebase or `git am`. */
export function terminalRebaseReason(rebase: TerminalRebase): string {
  return rebase.kind === "Am"
    ? "A git am started in a terminal is in progress."
    : "A rebase started in a terminal is in progress.";
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

/**
 * The parts of the app state that decide whether repository operations are blocked. Structural,
 * so the full `AppState` passes as it is.
 */
export interface OperationState {
  pending: boolean;
  transfer: object | null;
  /** The fetch, pull or push this tab started, from its request until it ends (#659). */
  transferSubject?: TransferSubject | null;
  mergeMessage: string | null;
  rebaseProgress: object | null;
  headInfo: Pick<HeadInfo, "sequence" | "terminalRebase" | "operation" | "operationLeftover">;
}

/** Whether repository operations are blocked, and the reason disabled controls show. */
export interface OperationBlockers {
  blocked: boolean;
  /** `null` exactly when nothing blocks. */
  reason: string | null;
}

/**
 * The one "repository busy" rule (#495) for menus, buttons, the command palette and shortcuts:
 * an action in flight, a transfer, a merge/cherry-pick/revert (MERGE_MSG), a rebase run here, a
 * stored cherry-pick/revert sequence, or a rebase or `git am` started in a terminal (#503). The
 * reason names the first match in that order of precedence.
 */
export function operationBlockers(state: OperationState): OperationBlockers {
  const reason = operationBlockReason(state);
  return { blocked: reason !== null, reason };
}

/**
 * Why an action that is allowed during a paused merge, rebase or cherry-pick/revert step is
 * blocked, or `null` when it is allowed: only an action in flight or a transfer blocks it.
 * Narrower than `operationBlockers`, for discarding changes and dropping a stash, which users do
 * while resolving a step (#521). The reasons are `operationBlockers`' own.
 */
export function busyReason(state: Pick<OperationState, "pending" | "transfer" | "transferSubject">): string | null {
  // A running transfer names itself, so a disabled control says what it waits for (#659).
  if (state.transferSubject != null) return transferBusyReason(state.transferSubject);
  if (state.pending) return "Another action is in progress.";
  if (state.transfer !== null) return "A transfer is in progress.";
  return null;
}

function operationBlockReason(state: OperationState): string | null {
  const { headInfo } = state;
  const busy = busyReason(state);
  if (busy !== null) return busy;
  if (headInfo.sequence !== null) return `A ${sequenceNoun(headInfo.sequence.kind)} is in progress.`;
  if (headInfo.terminalRebase !== null) return terminalRebaseReason(headInfo.terminalRebase);
  if (state.mergeMessage !== null) {
    if (headInfo.operationLeftover && headInfo.operation !== null) return leftoverReason(headInfo.operation);
    return headInfo.operation === "CherryPick" || headInfo.operation === "Revert"
      ? `A ${sequenceNoun(headInfo.operation)} is in progress.`
      : "A merge is in progress.";
  }
  if (state.rebaseProgress !== null) return "A rebase is in progress.";
  return null;
}

/**
 * Why stashing is unavailable, or `null` when it is allowed. Narrower than
 * `operationBlockers`: stashing during a merge is fine, but stashing a paused rebase or
 * cherry-pick/revert step's working tree away and continuing lands an empty (or wrong) commit,
 * which nothing undoes. Shared by DiffPane's Stash button, the file menu's "Stash file…" and the
 * palette's stash commands (#495).
 */
export function stashBlockReason(state: {
  status: readonly unknown[];
  rebaseProgress: object | null;
  headInfo: Pick<HeadInfo, "sequence" | "terminalRebase">;
}): string | null {
  const { headInfo } = state;
  if (state.rebaseProgress !== null) return "A rebase is in progress.";
  if (headInfo.sequence !== null) return `A ${sequenceNoun(headInfo.sequence.kind)} is in progress.`;
  if (headInfo.terminalRebase !== null) return terminalRebaseReason(headInfo.terminalRebase);
  if (state.status.length === 0) return "No changes to stash.";
  return null;
}
