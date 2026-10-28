import type { OperationKind, SequenceProgress } from "../ipc/RepoClient";
import { conflictReason, sequenceLabel, sequenceNoun } from "../lib/operationStatus";
import styles from "./OperationStatusStrip.module.css";

/**
 * Persistent strip shown for the whole time a merge, cherry-pick, revert or rebase is in progress, so the mode that
 * constrains every other action is never invisible (AUD-2026-09-20-FB-002). A multi-commit
 * cherry-pick/revert shows its step and Continue, also when it stopped on an error rather than a
 * conflict (then git itself has nothing in progress).
 */
export function OperationStatusStrip({
  merging,
  operation,
  rebaseProgress,
  sequence = null,
  conflictCount,
  onAbortMerge,
  onAbortRebase,
  onContinueSequence,
  onAbortSequence,
}: {
  merging: boolean;
  operation: OperationKind | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  sequence?: SequenceProgress | null;
  conflictCount: number;
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
        <button type="button" disabled={conflictCount > 0} title={reason} onClick={onContinueSequence}>
          Continue
        </button>
        <button type="button" onClick={onAbortSequence}>
          Abort {sequenceNoun(sequence.kind)}
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
