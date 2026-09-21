import type { TransferProgress } from "../ipc/RepoClient";
import { Panel } from "./primitives/Panel";
import styles from "./TransferPanel.module.css";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function TransferPanel({ progress }: { progress: TransferProgress | null }) {
  if (progress === null) return null;

  const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;

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
    </Panel>
  );
}
