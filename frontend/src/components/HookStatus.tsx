import { useState } from "react";
import { runningHookLabel } from "../lib/hookFailure";
import styles from "./HookStatus.module.css";

/**
 * "Running {hook} hook…" (or "Waiting for the signing program…") with a Cancel button, shown by
 * the commit box and by the sequence panel while Continue runs the commit hooks.
 */
export function HookStatus({
  running,
  onCancel,
}: {
  running: { operationId: string; hook: string };
  onCancel: () => void;
}) {
  // Keyed by operation id (like `TransferPanel`), so Cancel re-arms for the next hook run.
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  const cancelling = cancellingId === running.operationId;
  return (
    <p role="status" className={styles.hookStatus}>
      {runningHookLabel(running.hook)}{" "}
      <button
        type="button"
        disabled={cancelling}
        onClick={() => {
          setCancellingId(running.operationId);
          onCancel();
        }}
      >
        {cancelling ? "Cancelling…" : "Cancel"}
      </button>
    </p>
  );
}
