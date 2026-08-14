import type { TransferProgress } from "../ipc/RepoClient";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(1)} KB`;
}

export function TransferPanel({ progress }: { progress: TransferProgress | null }) {
  if (progress === null) return null;

  return (
    <section className="transfer-panel" aria-live="polite" aria-label="Fetch progress">
      <h2>Fetching</h2>
      <p>{progress.phase}</p>
      <p>{progress.current} / {progress.total} objects</p>
      <p>{formatBytes(progress.receivedBytes)} received</p>
    </section>
  );
}
