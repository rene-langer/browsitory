import type { OperationKind } from "../ipc/RepoClient";
import { conflictReason } from "../lib/operationStatus";
import styles from "./OperationStatusStrip.module.css";

/**
 * Persistent strip shown for the whole time a merge, cherry-pick, revert or rebase is in progress, so the mode that
 * constrains every other action is never invisible (AUD-2026-09-20-FB-002).
 */
export function OperationStatusStrip({
  merging,
  operation,
  rebaseProgress,
  conflictCount,
  onAbortMerge,
  onAbortRebase,
}: {
  merging: boolean;
  operation: OperationKind | null;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  conflictCount: number;
  onAbortMerge: () => void;
  onAbortRebase: () => void;
}) {
  if (!merging && rebaseProgress === null) return null;
  const mergeLabel =
    operation === "CherryPick" ? "Cherry-picking" : operation === "Revert" ? "Reverting" : "Merging";
  const abortLabel =
    operation === "CherryPick" ? "Abort cherry-pick" : operation === "Revert" ? "Abort revert" : "Abort merge";
  const label =
    rebaseProgress !== null
      ? `Rebasing, step ${rebaseProgress.currentStep} of ${rebaseProgress.totalSteps}`
      : mergeLabel;
  const reason = conflictReason(conflictCount);
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
