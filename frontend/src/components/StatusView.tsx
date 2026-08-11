import { useEffect, useState } from "react";
import type { RepoClient, StatusEntry } from "../ipc/RepoClient";

export function StatusView({ client }: { client: RepoClient }) {
  const [entries, setEntries] = useState<StatusEntry[]>([]);

  useEffect(() => {
    client.getStatus().then(setEntries);
  }, [client]);

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
