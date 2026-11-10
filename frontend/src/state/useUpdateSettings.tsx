import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { DEFAULT_UPDATE_SETTINGS, type RepoClient, type UpdateSettings } from "../ipc/RepoClient";

/**
 * Where the saved settings stand: `loading` until the first read answers, `failed` when it
 * couldn't be read. The automatic update check waits for `loaded` and stays off on `failed`, so
 * a setting turned off is never overridden by the default (on) while loading or after a
 * broken read (#630).
 */
export type UpdateSettingsStatus = "loading" | "loaded" | "failed";

export interface UpdateSettingsApi {
  settings: UpdateSettings;
  status: UpdateSettingsStatus;
  update(partial: Partial<UpdateSettings>): void;
  error: string | null;
  dismissError(): void;
}

/**
 * The desktop app's update settings (`[updates]`), loaded once and saved on every update. Not
 * `enabled` (no updater, as in VSCode), they are never read and stay `loading`.
 */
export function useUpdateSettings(client: RepoClient, enabled = true): UpdateSettingsApi {
  const [settings, setSettings] = useState<UpdateSettings>(DEFAULT_UPDATE_SETTINGS);
  const [status, setStatus] = useState<UpdateSettingsStatus>("loading");
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(settings);

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    // `Promise.resolve().then` so a synchronous throw from the client is also caught.
    Promise.resolve()
      .then(() => client.getUpdateSettings())
      .then(
        (loaded) => {
          if (cancelled) return;
          latest.current = loaded;
          setSettings(loaded);
          setStatus("loaded");
        },
        () => {
          if (!cancelled) setStatus("failed");
        },
      );
    return () => {
      cancelled = true;
    };
  }, [client, enabled]);

  const update = useCallback(
    (partial: Partial<UpdateSettings>) => {
      const previous = latest.current;
      const next = { ...previous, ...partial };
      latest.current = next;
      setSettings(next);
      // What the user just chose is known even if the first read failed.
      setStatus("loaded");
      Promise.resolve()
        .then(() => client.setUpdateSettings(next))
        .catch((err: unknown) => {
          latest.current = previous;
          setSettings(previous);
          setError(err instanceof Error ? err.message : String(err));
        });
    },
    [client],
  );

  const dismissError = useCallback(() => setError(null), []);
  return { settings, status, update, error, dismissError };
}

/** Without a provider (tests, a host without updates): the defaults, as if loaded. */
const fallback: UpdateSettingsApi = {
  settings: DEFAULT_UPDATE_SETTINGS,
  status: "loaded",
  update: () => {},
  error: null,
  dismissError: () => {},
};

export const UpdateSettingsContext = createContext<UpdateSettingsApi>(fallback);

export function useUpdateSettingsContext(): UpdateSettingsApi {
  return useContext(UpdateSettingsContext);
}
