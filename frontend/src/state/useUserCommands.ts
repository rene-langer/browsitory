import { useCallback, useEffect, useState } from "react";
import type { RepoClient, UserCommandInput, UserCommands } from "../ipc/RepoClient";

// Every mounted `useUserCommands` reloads when any of them stores a change, so the menus and the
// palette follow an edit in Preferences.
const listeners = new Set<() => void>();

function changed() {
  for (const listener of listeners) listener();
}

export interface UserCommandsApi {
  /** `null` while loading (or when loading failed). */
  commands: UserCommands | null;
  error: string | null;
  /**
   * Stores the app-wide list (`scope` `null`) or `scope`'s repository list. Rejects with the
   * backend's reason, leaving `commands` as they were.
   */
  save(scope: string | null, next: UserCommandInput[]): Promise<void>;
}

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

/** The user commands (#893) for `repoPath`: the app-wide ones and that repository's own. */
export function useUserCommands(client: RepoClient, repoPath: string | null): UserCommandsApi {
  const [commands, setCommands] = useState<UserCommands | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    const listener = () => setVersion((current) => current + 1);
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);

  useEffect(() => {
    let ignore = false;
    client.listUserCommands(repoPath).then(
      (loaded) => {
        if (ignore) return;
        setCommands(loaded);
        setError(null);
      },
      (err: unknown) => {
        if (!ignore) setError(message(err));
      },
    );
    return () => {
      ignore = true;
    };
  }, [client, repoPath, version]);

  const save = useCallback(
    async (scope: string | null, next: UserCommandInput[]) => {
      const stored = await client.setUserCommands(scope, next);
      setCommands((current) =>
        current === null ? current : scope === null ? { ...current, global: stored } : { ...current, repository: stored },
      );
      changed();
    },
    [client],
  );

  return { commands, error, save };
}
