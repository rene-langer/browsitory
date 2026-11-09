import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import {
  DEFAULT_DIFF_SETTINGS,
  DIFF_ALGORITHM_VALUES,
  MAX_CONTEXT_LINES,
  WORD_WRAP_VALUES,
  type DiffSettings,
  type RepoClient,
} from "../ipc/RepoClient";

export interface DiffSettingsApi {
  settings: DiffSettings;
  update(partial: Partial<DiffSettings>): void;
  error: string | null;
  dismissError(): void;
}

function clamp(settings: DiffSettings): DiffSettings {
  const contextLines = Math.min(Math.max(Math.round(settings.contextLines), 0), MAX_CONTEXT_LINES);
  // A missing or unknown value falls back to `Auto`.
  const wordWrap = WORD_WRAP_VALUES.includes(settings.wordWrap) ? settings.wordWrap : DEFAULT_DIFF_SETTINGS.wordWrap;
  const algorithm = DIFF_ALGORITHM_VALUES.includes(settings.algorithm)
    ? settings.algorithm
    : DEFAULT_DIFF_SETTINGS.algorithm;
  return { ...settings, contextLines, wordWrap, algorithm };
}

export function useDiffSettings(client: RepoClient): DiffSettingsApi {
  const [settings, setSettings] = useState<DiffSettings>(DEFAULT_DIFF_SETTINGS);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(settings);
  useEffect(() => {
    latest.current = settings;
  }, [settings]);

  useEffect(() => {
    let cancelled = false;
    // `Promise.resolve().then` so a synchronous throw from the client is also swallowed.
    Promise.resolve()
      .then(() => client.getDiffSettings())
      .then(
        (loaded) => {
          if (!cancelled) {
            const next = clamp(loaded);
            latest.current = next;
            setSettings(next);
          }
        },
        () => {}, // keep defaults; a missing/broken config must not block the diff view
      );
    return () => {
      cancelled = true;
    };
  }, [client]);

  const update = useCallback(
    (partial: Partial<DiffSettings>) => {
      const previous = latest.current;
      const next = clamp({ ...previous, ...partial });
      latest.current = next;
      setSettings(next);
      Promise.resolve()
        .then(() => client.setDiffSettings(next))
        .catch((err: unknown) => {
          setSettings(previous);
          latest.current = previous;
          setError(err instanceof Error ? err.message : String(err));
        });
    },
    [client],
  );

  const dismissError = useCallback(() => setError(null), []);

  return { settings, update, error, dismissError };
}

const fallback: DiffSettingsApi = {
  settings: DEFAULT_DIFF_SETTINGS,
  update: () => {},
  error: null,
  dismissError: () => {},
};

export const DiffSettingsContext = createContext<DiffSettingsApi>(fallback);

export function useDiffSettingsContext(): DiffSettingsApi {
  return useContext(DiffSettingsContext);
}
