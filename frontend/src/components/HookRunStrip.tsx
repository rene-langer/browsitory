import { useState } from "react";
import type { HookSource } from "../ipc/RepoClient";
import { runningHookLabel } from "../lib/hookFailure";
import styles from "./HookRunStrip.module.css";

/**
 * Non-modal "Hook status" strip under the operation strip (#185): "Running post-checkout hook…"
 * with Cancel while a hook runs for a switch, a checkout, a fast-forward or squash merge, or a
 * cherry-pick/revert. A commit's, amend's or Continue's hooks (`source: "Commit"`) show in the
 * commit box and the sequence panel instead, and pushes and pulls in the transfer panel.
 *
 * Cancel stays enabled while the command is pending (the hook is what keeps it pending) and
 * latches per operation id, like `HookStatus`. Cancelling stops only the hook: the switch,
 * merge or pick it runs after stands.
 */
export function HookRunStrip({
  running,
  onCancel,
}: {
  running: { operationId: string; hook: string; source: HookSource } | null;
  onCancel: () => void;
}) {
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  if (running === null || running.source === "Commit") return null;
  const cancelling = cancellingId === running.operationId;
  return (
    <div role="status" aria-label="Hook status" className={styles.strip}>
      <span>{runningHookLabel(running.hook)}</span>
      <span className={styles.spacer} />
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
    </div>
  );
}
