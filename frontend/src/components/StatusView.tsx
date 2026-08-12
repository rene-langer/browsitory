import { useEffect, useState } from "react";
import type { RepoClient, StatusEntry } from "../ipc/RepoClient";

export function StatusView({ client }: { client: RepoClient }) {
  const [entries, setEntries] = useState<StatusEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    client
      .getStatus()
      .then((next) => {
        setEntries(next);
        setError(null);
      })
      .catch((err: unknown) => setError(String(err)));
  }, [client]);

  if (error !== null) {
    return <p role="alert">Failed to load status: {error}</p>;
  }

  if (entries.length === 0) {
    return <p>No changes</p>;
  }

  return (
    <ul>
      {entries.map((entry) => (
        <li key={`${entry.staged}:${entry.path}`}>
          <span aria-hidden="true">{entry.staged ? "●" : "○"}</span>{" "}
          <span>{entry.path}</span>{" "}
          <span>({entry.kind})</span>
        </li>
      ))}
    </ul>
  );
}
