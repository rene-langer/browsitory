import { useEffect, useState } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./RepoPicker.module.css";

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
    <Panel title="Open a repository">
      <Toolbar>
        <button onClick={handleOpenFolder}>Open Folder</button>
      </Toolbar>
      {error !== null && <p role="alert">{error}</p>}
      {recentRepos.length === 0 ? (
        <p>No recent repositories</p>
      ) : (
        <ul className={styles.list}>
          {recentRepos.map((path) => (
            <ListRow key={path} onClick={() => onOpenRepo(path)}>
              {path}
            </ListRow>
          ))}
        </ul>
      )}
    </Panel>
  );
}
