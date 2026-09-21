import { conflictReason } from "../lib/operationStatus";
import styles from "./OperationStatusStrip.module.css";

/**
 * Persistent strip shown for the whole time a merge or rebase is in progress, so the mode that
 * constrains every other action is never invisible (AUD-2026-09-20-FB-002).
 */
export function OperationStatusStrip({
  merging,
  rebaseProgress,
  conflictCount,
  onAbortMerge,
  onAbortRebase,
}: {
  merging: boolean;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  conflictCount: number;
  onAbortMerge: () => void;
  onAbortRebase: () => void;
}) {
  if (!merging && rebaseProgress === null) return null;
  const label =
    rebaseProgress !== null
      ? `Rebasing, step ${rebaseProgress.currentStep} of ${rebaseProgress.totalSteps}`
      : "Merging";
  const reason = conflictReason(conflictCount);
  return (
    <div role="status" aria-label="Operation status" className={styles.strip}>
      <strong>{label}</strong>
      <span>{reason ?? "No conflicts remaining"}</span>
      <span className={styles.spacer} />
      <button type="button" onClick={rebaseProgress !== null ? onAbortRebase : onAbortMerge}>
        {rebaseProgress !== null ? "Abort rebase" : "Abort merge"}
      </button>
    </div>
  );
}
