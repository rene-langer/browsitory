import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { DEFAULT_APPEARANCE_SETTINGS, type AppearanceSettings, type RepoClient } from "../ipc/RepoClient";
import { clampAppearance } from "../lib/appearance";

export interface AppearanceSettingsApi {
  settings: AppearanceSettings;
  update(partial: Partial<AppearanceSettings>): void;
  error: string | null;
  dismissError(): void;
}

/** The app-wide appearance settings (`[appearance]`, #615), loaded once, clamped, and saved on every update. */
export function useAppearanceSettings(client: RepoClient): AppearanceSettingsApi {
  const [settings, setSettings] = useState<AppearanceSettings>(DEFAULT_APPEARANCE_SETTINGS);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(settings);
  // Set by `update`: a change made before the stored settings arrive wins over them.
  const changed = useRef(false);

  useEffect(() => {
    let cancelled = false;
    // `Promise.resolve().then` so a synchronous throw from the client is also swallowed.
    Promise.resolve()
      .then(() => client.getAppearanceSettings())
      .then(
        (loaded) => {
          if (cancelled || changed.current) return;
          const next = clampAppearance(loaded);
          latest.current = next;
          setSettings(next);
        },
        () => {}, // keep the built-in look; a broken config must not break the views
      );
    return () => {
      cancelled = true;
    };
  }, [client]);

  const update = useCallback(
    (partial: Partial<AppearanceSettings>) => {
      changed.current = true;
      const previous = latest.current;
      const next = clampAppearance({ ...previous, ...partial });
      latest.current = next;
      setSettings(next);
      Promise.resolve()
        .then(() => client.setAppearanceSettings(next))
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

const fallback: AppearanceSettingsApi = {
  settings: DEFAULT_APPEARANCE_SETTINGS,
  update: () => {},
  error: null,
  dismissError: () => {},
};

export const AppearanceSettingsContext = createContext<AppearanceSettingsApi>(fallback);

export function useAppearanceSettingsContext(): AppearanceSettingsApi {
  return useContext(AppearanceSettingsContext);
}
