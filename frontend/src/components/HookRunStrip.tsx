import { useEffect, useRef, useState } from "react";
import type { HookSource } from "../ipc/RepoClient";
import { runningHookLabel } from "../lib/hookFailure";
import styles from "./HookRunStrip.module.css";

/** What the strip says after the user cancelled `hook` and it ended. */
function cancelledLabel(hook: string): string {
  return hook === "signing" ? "Signing cancelled." : `The ${hook} hook was cancelled.`;
}

/**
 * Non-modal "Hook status" strip under the operation strip (#185): "Running post-checkout hook…"
 * with Cancel while a hook runs for a switch, a checkout, a fast-forward or squash merge, or a
 * cherry-pick/revert. A commit's, amend's or Continue's hooks (`source: "Commit"`) show in the
 * commit box and the sequence panel instead, and pushes and pulls in the transfer panel.
 *
 * The live region stays mounted (empty and unstyled while no hook runs), so screen readers
 * announce each hook as it starts. Cancel stays enabled while the command is pending (the hook
 * is what keeps it pending) and latches per operation id, like `HookStatus`. Cancelling stops
 * only the hook: the switch, merge or pick it runs after stands. When a cancelled hook ends, the
 * strip says so ("The post-checkout hook was cancelled.") and, if focus went with the Cancel
 * button, takes focus itself, so it lands on visible text; the note goes once focus moves on
 * or the next hook starts.
 */
export function HookRunStrip({
  running,
  onCancel,
}: {
  running: { operationId: string; hook: string; source: HookSource } | null;
  onCancel: () => void;
}) {
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  // The hook the user cancelled; its note shows once it ended, until focus moves on or another
  // hook starts.
  const [cancelled, setCancelled] = useState<{ operationId: string; hook: string } | null>(null);
  const stripRef = useRef<HTMLDivElement>(null);
  const shown = running !== null && running.source !== "Commit" ? running : null;
  // Another hook started: the old note is stale (state adjusted during render, no effect).
  if (shown !== null && cancelled !== null && shown.operationId !== cancelled.operationId) {
    setCancelled(null);
  }
  const cancelledNote = shown === null && cancelled !== null ? cancelledLabel(cancelled.hook) : null;

  useEffect(() => {
    if (cancelledNote === null) return;
    // Only when focus fell back to the page with the button; never steal it from elsewhere.
    const active = document.activeElement;
    if (active === null || active === document.body) stripRef.current?.focus();
  }, [cancelledNote]);

  const cancelling = shown !== null && cancellingId === shown.operationId;
  const idle = shown === null && cancelledNote === null;
  return (
    <div
      ref={stripRef}
      role="status"
      aria-label="Hook status"
      tabIndex={-1}
      className={idle ? styles.idle : styles.strip}
      onBlur={() => {
        if (shown === null) setCancelled(null);
      }}
    >
      {shown !== null ? (
        <>
          <span>{runningHookLabel(shown.hook)}</span>
          <span className={styles.spacer} />
          <button
            type="button"
            aria-label={shown.hook === "signing" ? "Cancel signing" : `Cancel ${shown.hook} hook`}
            disabled={cancelling}
            onClick={() => {
              setCancelled({ operationId: shown.operationId, hook: shown.hook });
              setCancellingId(shown.operationId);
              onCancel();
            }}
          >
            {cancelling ? "Cancelling…" : "Cancel"}
          </button>
        </>
      ) : (
        cancelledNote !== null && <span>{cancelledNote}</span>
      )}
    </div>
  );
}
