import { useCallback, useEffect, useMemo, useState } from "react";
import type { DiffSource, RepoClient } from "../ipc/RepoClient";

export type MergeToolResult = "Resolved" | "Unresolved";

export interface ExternalToolsApi {
  openInEditor(path: string, line: number | null): void;
  reveal(path: string | null): void;
  openTerminal(): void;
  openDiffTool(path: string, source: DiffSource): void;
  openMergeTool(path: string): void;
  /** Paths whose merge tool is open right now. */
  mergeToolRunning: ReadonlySet<string>;
  /** How each finished merge tool ended, until dismissed or the path is no longer conflicted. */
  mergeToolResults: ReadonlyMap<string, MergeToolResult>;
  dismissMergeToolResult(path: string): void;
  error: string | null;
  dismissError(): void;
}

const message = (err: unknown) => (err instanceof Error ? err.message : String(err));

function without<K, V>(map: ReadonlyMap<K, V>, key: K): ReadonlyMap<K, V> {
  if (!map.has(key)) return map;
  const next = new Map(map);
  next.delete(key);
  return next;
}

/**
 * Launches the user's own tools. Never goes through `runMutation`: opening a tool changes
 * nothing in the repo, so it must not set `pending` or block other actions. A merge tool exit
 * refreshes, because the tool rewrote the work tree file.
 */
export function useExternalTools(
  client: RepoClient,
  repoPath: string,
  conflictedPaths: readonly string[],
  refresh: () => Promise<void>,
): ExternalToolsApi {
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState<ReadonlySet<string>>(() => new Set());
  const [results, setResults] = useState<ReadonlyMap<string, MergeToolResult>>(() => new Map());

  const launch = useCallback((run: () => Promise<unknown>) => {
    setError(null);
    Promise.resolve()
      .then(run)
      .catch((err: unknown) => setError(message(err)));
  }, []);

  const openMergeTool = useCallback(
    (path: string) => {
      setError(null);
      setRunning((prev) => new Set(prev).add(path));
      setResults((prev) => without(prev, path));
      void client
        .openMergeTool(repoPath, path)
        .then((outcome) => {
          if (outcome !== "HandedOff") setResults((prev) => new Map(prev).set(path, outcome));
        })
        .catch((err: unknown) => setError(message(err)))
        .finally(() => {
          setRunning((prev) => {
            const next = new Set(prev);
            next.delete(path);
            return next;
          });
          void refresh();
        });
    },
    [client, repoPath, refresh],
  );

  // Results for paths that are no longer conflicted (resolved elsewhere) are dropped.
  const conflictedKey = conflictedPaths.join("\0");
  useEffect(() => {
    const conflicted = new Set(conflictedKey === "" ? [] : conflictedKey.split("\0"));
    // eslint-disable-next-line react-hooks/set-state-in-effect -- derived from props; a no-op when nothing changes
    setResults((prev) => {
      const kept = [...prev].filter(([path]) => conflicted.has(path));
      return kept.length === prev.size ? prev : new Map(kept);
    });
  }, [conflictedKey]);

  return useMemo(
    () => ({
      openInEditor: (path: string, line: number | null) => launch(() => client.openInEditor(repoPath, path, line)),
      reveal: (path: string | null) => launch(() => client.revealInFileManager(repoPath, path)),
      openTerminal: () => launch(() => client.openTerminal(repoPath)),
      openDiffTool: (path: string, source: DiffSource) => launch(() => client.openDiffTool(repoPath, path, source)),
      openMergeTool,
      mergeToolRunning: running,
      mergeToolResults: results,
      dismissMergeToolResult: (path: string) => setResults((prev) => without(prev, path)),
      error,
      dismissError: () => setError(null),
    }),
    [client, repoPath, launch, openMergeTool, running, results, error],
  );
}
