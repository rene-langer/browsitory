import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import {
  COMMIT_MESSAGE_COLUMNS,
  DEFAULT_COMMIT_MESSAGE_SETTINGS,
  type CommitMessageSettings,
  type RepoClient,
} from "../ipc/RepoClient";

export interface CommitMessageSettingsApi {
  settings: CommitMessageSettings;
  update(partial: Partial<CommitMessageSettings>): void;
  error: string | null;
  dismissError(): void;
}

export function clampColumn(value: number, fallback: number): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.min(Math.max(Math.round(value), COMMIT_MESSAGE_COLUMNS.min), COMMIT_MESSAGE_COLUMNS.max);
}

function clamp(settings: CommitMessageSettings): CommitMessageSettings {
  return {
    summaryLimit: clampColumn(settings.summaryLimit, DEFAULT_COMMIT_MESSAGE_SETTINGS.summaryLimit),
    bodyRuler: clampColumn(settings.bodyRuler, DEFAULT_COMMIT_MESSAGE_SETTINGS.bodyRuler),
    showRuler: settings.showRuler,
  };
}

/** The commit message counter and ruler settings, loaded once and saved on every update. */
export function useCommitMessageSettings(client: RepoClient): CommitMessageSettingsApi {
  const [settings, setSettings] = useState<CommitMessageSettings>(DEFAULT_COMMIT_MESSAGE_SETTINGS);
  const [error, setError] = useState<string | null>(null);
  const latest = useRef(settings);
  useEffect(() => {
    latest.current = settings;
  }, [settings]);

  useEffect(() => {
    let cancelled = false;
    // `Promise.resolve().then` so a synchronous throw from the client is also swallowed.
    Promise.resolve()
      .then(() => client.getCommitMessageSettings())
      .then(
        (loaded) => {
          if (!cancelled) {
            const next = clamp(loaded);
            latest.current = next;
            setSettings(next);
          }
        },
        () => {}, // keep defaults; a missing/broken config must not block committing
      );
    return () => {
      cancelled = true;
    };
  }, [client]);

  const update = useCallback(
    (partial: Partial<CommitMessageSettings>) => {
      const previous = latest.current;
      const next = clamp({ ...previous, ...partial });
      latest.current = next;
      setSettings(next);
      Promise.resolve()
        .then(() => client.setCommitMessageSettings(next))
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

const fallback: CommitMessageSettingsApi = {
  settings: DEFAULT_COMMIT_MESSAGE_SETTINGS,
  update: () => {},
  error: null,
  dismissError: () => {},
};

export const CommitMessageSettingsContext = createContext<CommitMessageSettingsApi>(fallback);

export function useCommitMessageSettingsContext(): CommitMessageSettingsApi {
  return useContext(CommitMessageSettingsContext);
}
