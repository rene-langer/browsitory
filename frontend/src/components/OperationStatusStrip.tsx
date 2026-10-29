import type { OperationKind, RebaseInfo, SequenceProgress } from "../ipc/RepoClient";
import { conflictReason, rebaseInterruption, sequenceLabel, sequenceNoun } from "../lib/operationStatus";
import styles from "./OperationStatusStrip.module.css";

/**
 * Persistent strip shown for the whole time a merge, cherry-pick, revert or rebase is in progress, so the mode that
 * constrains every other action is never invisible (AUD-2026-09-20-FB-002). A multi-commit
 * cherry-pick/revert shows its step and Continue, also when it stopped on an error rather than a
 * conflict (then git itself has nothing in progress). A stored rebase that no longer matches the
 * repository (changed outside Browsitory while paused) shows as interrupted, with Abort only.
 */
export function OperationStatusStrip({
  merging,
  operation,
  rebaseProgress,
  rebase = null,
  sequence = null,
  conflictCount,
  pending = false,
  onAbortMerge,
  onAbortRebase,
  onContinueSequence,
  onAbortSequence,
}: {
  merging: boolean;
  operation: OperationKind | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  /** The stored rebase with its health (`HeadInfo.rebase`). */
  rebase?: RebaseInfo | null;
  sequence?: SequenceProgress | null;
  conflictCount: number;
  /** An action is running: Continue/Abort are disabled so a second click can't double-submit. */
  pending?: boolean;
  onAbortMerge: () => void;
  onAbortRebase: () => void;
  onContinueSequence?: () => void;
  onAbortSequence?: () => void;
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
  if (!merging && rebaseProgress === null) return null;
  const mergeLabel =
    operation === "CherryPick" ? "Cherry-picking" : operation === "Revert" ? "Reverting" : "Merging";
  const abortLabel =
    operation === "CherryPick" ? "Abort cherry-pick" : operation === "Revert" ? "Abort revert" : "Abort merge";
  const label =
    rebaseProgress !== null
      ? `Rebasing, step ${rebaseProgress.currentStep} of ${rebaseProgress.totalSteps}`
      : mergeLabel;
  return (
    <div role="status" aria-label="Operation status" className={styles.strip}>
      <strong>{label}</strong>
      <span>{reason ?? "No conflicts remaining"}</span>
      <span className={styles.spacer} />
      <button type="button" onClick={rebaseProgress !== null ? onAbortRebase : onAbortMerge}>
        {rebaseProgress !== null ? "Abort rebase" : abortLabel}
      </button>
    </div>
  );
}
