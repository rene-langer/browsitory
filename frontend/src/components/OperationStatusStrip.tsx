import type {
  BisectInfo,
  OperationKind,
  RebaseInfo,
  SequenceProgress,
  TerminalRebase,
  TerminalSequence,
} from "../ipc/RepoClient";
import {
  bisectLabel,
  conflictReason,
  leftoverLabel,
  leftoverText,
  rebaseInterruption,
  sequenceLabel,
  sequenceNoun,
  terminalRebaseLabel,
  terminalRebaseText,
  terminalSequenceLabel,
  terminalSequenceText,
} from "../lib/operationStatus";
import styles from "./OperationStatusStrip.module.css";

/**
 * Persistent strip shown for the whole time a merge, cherry-pick, revert or rebase is in progress, so the mode that
 * constrains every other action is never invisible (AUD-2026-09-20-FB-002). A multi-commit
 * cherry-pick/revert shows its step and Continue, also when it stopped on an error rather than a
 * conflict (then git itself has nothing in progress). A stored rebase that no longer matches the
 * repository (changed outside Browsitory while paused) shows as interrupted, with Abort only.
 * A `git cherry-pick A B C` started in a terminal shows too (#258): paused between steps with
 * Abort only (it is continued in the terminal); paused on a step as the step's strip, labelled as
 * started in a terminal. Its Abort ends the whole sequence (`onAbortTerminalSequence`, #253).
 * A `git rebase` or `git am` started in a terminal shows as such, never as the merge its
 * `MERGE_MSG` suggests, with no buttons: it is continued or aborted in the terminal (#263).
 * When none of these is shown, a `git bisect` started in a terminal shows as information only:
 * it blocks nothing, and it is ended in the terminal (#250).
 * A merge, cherry-pick or revert whose changes are already in HEAD but whose state is left
 * (D7-A, or a duplicate pick that stopped as empty) says so, with Abort: the next commit or
 * amend clears it too (#240).
 */
export function OperationStatusStrip({
  merging,
  operation,
  operationLeftover = false,
  rebaseProgress,
  rebase = null,
  sequence = null,
  bisect = null,
  terminalSequence = null,
  terminalRebase = null,
  conflictCount,
  pending = false,
  onAbortMerge,
  onAbortRebase,
  onContinueSequence,
  onAbortSequence,
  onAbortTerminalSequence,
}: {
  merging: boolean;
  operation: OperationKind | null;
  /** `operation`'s changes are already in HEAD; only git's state files are left (#240). */
  operationLeftover?: boolean;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  /** The stored rebase with its health (`HeadInfo.rebase`). */
  rebase?: RebaseInfo | null;
  sequence?: SequenceProgress | null;
  /** A `git bisect` started in a terminal (`HeadInfo.bisect`). */
  bisect?: BisectInfo | null;
  /** A cherry-pick/revert started in a terminal (`HeadInfo.terminalSequence`). */
  terminalSequence?: TerminalSequence | null;
  /** A rebase or `git am` started in a terminal (`HeadInfo.terminalRebase`). */
  terminalRebase?: TerminalRebase | null;
  conflictCount: number;
  /** An action is running: Continue/Abort are disabled so a second click can't double-submit. */
  pending?: boolean;
  onAbortMerge: () => void;
  onAbortRebase: () => void;
  onContinueSequence?: () => void;
  onAbortSequence?: () => void;
  onAbortTerminalSequence?: () => void;
}) {
  const reason = conflictReason(conflictCount);
  if (sequence !== null) {
    return (
      <div role="status" aria-label="Operation status" className={styles.strip}>
        <strong>{sequenceLabel(sequence)}</strong>
        <span>{reason ?? "No conflicts remaining"}</span>
        <span className={styles.spacer} />
        <button
          type="button"
          disabled={pending || conflictCount > 0}
          title={reason}
          onClick={onContinueSequence}
        >
          Continue
        </button>
        <button type="button" disabled={pending} onClick={onAbortSequence}>
          Abort {sequenceNoun(sequence.kind)}
        </button>
      </div>
    );
  }
  const interruption = rebaseInterruption(rebase);
  if (rebase !== null && interruption !== undefined) {
    return (
      <div role="status" aria-label="Operation status" className={styles.strip}>
        <strong>Rebase of {rebase.branch} interrupted</strong>
        <span>{interruption}</span>
        <span className={styles.spacer} />
        <button
          type="button"
          disabled={pending}
          title="Stops the rebase. HEAD stays where it is."
          onClick={onAbortRebase}
        >
          Abort rebase
        </button>
      </div>
    );
  }
  if (terminalRebase !== null) {
    return (
      <div role="status" aria-label="Operation status" className={styles.strip}>
        <strong>{terminalRebaseLabel(terminalRebase)}</strong>
        <span>{terminalRebaseText(terminalRebase, conflictCount)}</span>
      </div>
    );
  }
  if (!merging && rebaseProgress === null && terminalSequence !== null) {
    const noun = sequenceNoun(terminalSequence.kind);
    return (
      <div role="status" aria-label="Operation status" className={styles.strip}>
        <strong>{terminalSequenceLabel(terminalSequence)}</strong>
        <span>
          {terminalSequence.pausedOnConflict
            ? (reason ?? "No conflicts remaining")
            : terminalSequenceText(terminalSequence.kind)}
        </span>
        <span className={styles.spacer} />
        <button type="button" disabled={pending} onClick={onAbortTerminalSequence}>
          Abort {noun}
        </button>
      </div>
    );
  }
  if (!merging && rebaseProgress === null) {
    if (bisect === null) return null;
    return (
      <div role="status" aria-label="Operation status" className={styles.strip}>
        <strong>{bisectLabel(bisect)}</strong>
        <span>Use git bisect good, bad or reset in a terminal.</span>
      </div>
    );
  }
  const mergeLabel =
    operation === "CherryPick" ? "Cherry-picking" : operation === "Revert" ? "Reverting" : "Merging";
  const abortLabel =
    operation === "CherryPick" ? "Abort cherry-pick" : operation === "Revert" ? "Abort revert" : "Abort merge";
  // A terminal sequence's paused step: its Abort ends the whole sequence, as git's `--abort`.
  const terminalStep = terminalSequence !== null && (operation === "CherryPick" || operation === "Revert");
  // The operation's changes are already in HEAD; only git's state files are left (#240).
  const leftover = operationLeftover && operation !== null && rebaseProgress === null && !terminalStep;
  const label =
    rebaseProgress !== null
      ? `Rebasing, step ${rebaseProgress.currentStep} of ${rebaseProgress.totalSteps}`
      : terminalStep
        ? terminalSequenceLabel({ ...terminalSequence, pausedOnConflict: true })
        : leftover
          ? leftoverLabel(operation)
          : mergeLabel;
  return (
    <div role="status" aria-label="Operation status" className={styles.strip}>
      <strong>{label}</strong>
      <span>{leftover ? leftoverText(operation) : (reason ?? "No conflicts remaining")}</span>
      <span className={styles.spacer} />
      <button
        type="button"
        onClick={rebaseProgress !== null ? onAbortRebase : terminalStep ? onAbortTerminalSequence : onAbortMerge}
      >
        {rebaseProgress !== null ? "Abort rebase" : abortLabel}
      </button>
    </div>
  );
}
