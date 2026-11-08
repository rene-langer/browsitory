import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { DEFAULT_FETCH_SETTINGS, type FetchSettings, type RepoClient } from "../ipc/RepoClient";

export interface FetchSettingsApi {
  settings: FetchSettings;
  update(partial: Partial<FetchSettings>): void;
  error: string | null;
  dismissError(): void;
}

/** The app-wide fetch settings (`[fetch]`), loaded once and saved on every update. */
export function useFetchSettings(client: RepoClient): FetchSettingsApi {
  const [settings, setSettings] = useState<FetchSettings>(DEFAULT_FETCH_SETTINGS);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(settings);

  useEffect(() => {
    let cancelled = false;
    // `Promise.resolve().then` so a synchronous throw from the client is also swallowed.
    Promise.resolve()
      .then(() => client.getFetchSettings())
      .then(
        (loaded) => {
          if (cancelled) return;
          latest.current = loaded;
          setSettings(loaded);
        },
        () => {}, // keep the default (off); a broken config must not fetch on its own
      );
    return () => {
      cancelled = true;
    };
  }, [client]);

  const update = useCallback(
    (partial: Partial<FetchSettings>) => {
      const previous = latest.current;
      const next = { ...previous, ...partial };
      latest.current = next;
      setSettings(next);
      Promise.resolve()
        .then(() => client.setFetchSettings(next))
        .catch((err: unknown) => {
          latest.current = previous;
          setSettings(previous);
          setError(err instanceof Error ? err.message : String(err));
        });
    },
    [client],
  );

  const dismissError = useCallback(() => setError(null), []);
  return { settings, update, error, dismissError };
}

const fallback: FetchSettingsApi = {
  settings: DEFAULT_FETCH_SETTINGS,
  update: () => {},
  error: null,
  dismissError: () => {},
};

export const FetchSettingsContext = createContext<FetchSettingsApi>(fallback);

export function useFetchSettingsContext(): FetchSettingsApi {
  return useContext(FetchSettingsContext);
}
