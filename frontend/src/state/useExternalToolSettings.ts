import { useCallback, useEffect, useState } from "react";
import type { ExternalToolSettings, RepoClient } from "../ipc/RepoClient";

export interface ExternalToolSettingsApi {
  /** `null` while loading. */
  settings: ExternalToolSettings | null;
  /** Resolves `true` once saved; on failure `settings` is unchanged and `error` is set. */
  save(next: ExternalToolSettings): Promise<boolean>;
  error: string | null;
}

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

export function useExternalToolSettings(client: RepoClient): ExternalToolSettingsApi {
  const [settings, setSettings] = useState<ExternalToolSettings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let ignore = false;
    client.getExternalToolSettings().then(
      (loaded) => {
        if (!ignore) setSettings(loaded);
      },
      (err: unknown) => {
        if (ignore) return;
        setSettings({ editor: null, terminal: null });
        setError(message(err));
      },
    );
    return () => {
      ignore = true;
    };
  }, [client]);

  const save = useCallback(
    async (next: ExternalToolSettings) => {
      try {
        await client.setExternalToolSettings(next);
        setSettings(next);
        setError(null);
        return true;
      } catch (err) {
        setError(message(err));
        return false;
      }
    },
    [client],
  );

  return { settings, save, error };
}
