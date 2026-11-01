import { useEffect, useRef, useState } from "react";
import type { HookSource } from "../ipc/RepoClient";
import { runningHookLabel } from "../lib/hookFailure";
import styles from "./HookRunStrip.module.css";

/**
 * Non-modal "Hook status" strip under the operation strip (#185): "Running post-checkout hook…"
 * with Cancel while a hook runs for a switch, a checkout, a fast-forward or squash merge, or a
 * cherry-pick/revert. A commit's, amend's or Continue's hooks (`source: "Commit"`) show in the
 * commit box and the sequence panel instead, and pushes and pulls in the transfer panel.
 *
 * The live region stays mounted (empty and unstyled while no hook runs), so screen readers
 * announce each hook as it starts. Cancel stays enabled while the command is pending (the hook
 * is what keeps it pending) and latches per operation id, like `HookStatus`. Cancelling stops
 * only the hook: the switch, merge or pick it runs after stands. After Cancel, when the hook ends
 * and the button goes, focus moves to the strip itself instead of falling back to the page.
 */
export function HookRunStrip({
  running,
  onCancel,
}: {
  running: { operationId: string; hook: string; source: HookSource } | null;
  onCancel: () => void;
}) {
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  const stripRef = useRef<HTMLDivElement>(null);
  const cancelHadFocus = useRef(false);
  const shown = running !== null && running.source !== "Commit" ? running : null;

  useEffect(() => {
    if (shown !== null || !cancelHadFocus.current) return;
    cancelHadFocus.current = false;
    // Only when focus fell back to the page with the button; never steal it from elsewhere.
    const active = document.activeElement;
    if (active === null || active === document.body) stripRef.current?.focus();
  }, [shown]);

  const cancelling = shown !== null && cancellingId === shown.operationId;
  return (
    <div
      ref={stripRef}
      role="status"
      aria-label="Hook status"
      tabIndex={-1}
      className={shown === null ? styles.idle : styles.strip}
    >
      {shown !== null && (
        <>
          <span>{runningHookLabel(shown.hook)}</span>
          <span className={styles.spacer} />
          <button
            type="button"
            aria-label={shown.hook === "signing" ? "Cancel signing" : `Cancel ${shown.hook} hook`}
            disabled={cancelling}
            onClick={() => {
              cancelHadFocus.current = true;
              setCancellingId(shown.operationId);
              onCancel();
            }}
          >
            {cancelling ? "Cancelling…" : "Cancel"}
          </button>
        </>
      )}
    </div>
  );
}
