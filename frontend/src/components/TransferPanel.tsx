import { useState } from "react";
import type { TransferProgress } from "../ipc/RepoClient";
import { Panel } from "./primitives/Panel";
import styles from "./TransferPanel.module.css";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function TransferPanel({
  progress,
  onCancel,
}: {
  progress: TransferProgress | null;
  onCancel: (operationId: string) => void;
}) {
  // Keyed by operation id rather than a boolean, so the button re-arms by itself when the next
  // transfer starts — no effect, and no stale "Cancelling…" carried over from the last one.
  const [cancellingId, setCancellingId] = useState<string | null>(null);

  if (progress === null) return null;

  const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;
  // Cancelling is a request, not an event: the transfer keeps reporting progress until git
  // reaches its next callback and aborts, so the button stays visible but latched.
  const cancelling = cancellingId === progress.operationId;

  return (
    <Panel title="Transferring" ariaLive="polite" ariaLabel="Transfer progress">
      <p>{progress.phase}</p>
      <div className={styles.progressTrack} aria-hidden="true">
        <div className={styles.progressFill} style={{ width: `${percent}%` }} />
      </div>
      <p>
        {progress.current} / {progress.total} objects
      </p>
      <p>{formatBytes(progress.receivedBytes)} received</p>
      <div className={styles.actions}>
        <button
          type="button"
          disabled={cancelling}
          onClick={() => {
            setCancellingId(progress.operationId);
            onCancel(progress.operationId);
          }}
        >
          {cancelling ? "Cancelling…" : "Cancel"}
        </button>
      </div>
    </Panel>
  );
}
