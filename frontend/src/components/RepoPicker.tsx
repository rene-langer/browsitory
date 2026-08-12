import { useEffect, useState } from "react";
import type { RepoClient } from "../ipc/RepoClient";

export function RepoPicker({
  client,
  onOpenRepo,
}: {
  client: RepoClient;
  onOpenRepo: (path: string) => void;
}) {
  const [recentRepos, setRecentRepos] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    client
      .listRecentRepos()
      .then(setRecentRepos)
      .catch((err: unknown) => setError(String(err)));
  }, [client]);

  const handleOpenFolder = () => {
    client
      .pickRepoFolder()
      .then((path) => {
        if (path !== null) {
          onOpenRepo(path);
        }
      })
      .catch((err: unknown) => setError(String(err)));
  };

  return (
    <div>
      <button onClick={handleOpenFolder}>Open Folder</button>
      {error !== null && <p role="alert">{error}</p>}
      {recentRepos.length === 0 ? (
        <p>No recent repositories</p>
      ) : (
        <ul>
          {recentRepos.map((path) => (
            <li key={path}>
              <button onClick={() => onOpenRepo(path)}>{path}</button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
