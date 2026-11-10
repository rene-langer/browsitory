import { useEffect, useRef, useState } from "react";
import type { RemoteInfo } from "../ipc/RepoClient";
import { loadPersistedOpen, persistOpen } from "../lib/persistedOpenState";

function remoteFolderKey(remoteName: string): string {
  return `branchtree.remote.${remoteName}`;
}

export type RemoteBranchCache = {
  /** Whether a remote's folder is expanded (persisted per remote, closed by default). */
  isOpen: (remoteName: string) => boolean;
  /** Expands or collapses a remote's folder; the first expand loads its branch list. */
  toggle: (remoteName: string) => void;
  /** The loaded branch list per remote; a remote missing here hasn't loaded (or was forgotten). */
  branches: Record<string, string[]>;
  /** The last failed list load, shown inline; `null` when none. */
  error: string | null;
  clearError: () => void;
  /** Drops a remote's list and reloads it if its folder is open (after a Fetch from its menu). */
  reload: (remoteName: string) => void;
  /**
   * Marks a remote's folder expanded without persisting or loading, so it can run during render
   * (a reveal request, #494); `persistOpenAndLoad` then finishes the expand from an effect.
   */
  markOpen: (remoteName: string) => void;
  /** Persists a remote's folder as expanded and loads its list if it hasn't loaded. */
  persistOpenAndLoad: (remoteName: string) => void;
};

/**
 * The remote folders of the Branches section and the lazy branch list of each, a cache of
 * `listRemoteBranches` that a folder fills on its first expand. A change of `refreshGeneration`
 * (`AppState.refreshGeneration`: a fetch from anywhere, a push that published a branch, a remote
 * branch delete) means the lists may have changed: open folders reload in place and closed ones
 * forget theirs, so their next expand reloads.
 */
export function useRemoteBranchCache({
  remotes,
  listRemoteBranches,
  refreshGeneration,
}: {
  remotes: RemoteInfo[];
  listRemoteBranches: (remoteName: string) => Promise<string[]>;
  refreshGeneration: number;
}): RemoteBranchCache {
  const [openRemotes, setOpenRemotes] = useState<Record<string, boolean>>({});
  const [remoteBranches, setRemoteBranches] = useState<Record<string, string[]>>({});
  const [error, setError] = useState<string | null>(null);

  function isOpen(remoteName: string): boolean {
    return openRemotes[remoteName] ?? loadPersistedOpen(remoteFolderKey(remoteName), false);
  }

  // Shared by the initial lazy-expand fetch, the refresh reload and `reload`, so all get the same
  // error handling — matching the Set-upstream dialog's `listRemoteBranches(...).catch(...)`
  // pattern instead of leaving this call's promise unhandled.
  const fetchRemoteBranchList = (remoteName: string) => {
    void listRemoteBranches(remoteName)
      .then((names) => {
        setRemoteBranches((prev) => ({ ...prev, [remoteName]: names }));
        setError(null);
      })
      .catch(() => setError(`Could not list branches for ${remoteName}.`));
  };

  // Detected during render (like `TagPanel`'s `previousTags`) so no state is set synchronously
  // inside an effect: closed folders forget their list, so their next expand reloads.
  const [cacheGeneration, setCacheGeneration] = useState(refreshGeneration);
  if (cacheGeneration !== refreshGeneration) {
    setCacheGeneration(refreshGeneration);
    setRemoteBranches((prev) => Object.fromEntries(Object.entries(prev).filter(([name]) => isOpen(name))));
  }
  const mountGeneration = useRef(refreshGeneration);
  useEffect(() => {
    if (refreshGeneration === mountGeneration.current) return;
    // Open folders reload in place, keeping their current rows until the new list lands (no
    // "Loading…" flash).
    for (const remote of remotes) {
      if (isOpen(remote.name)) fetchRemoteBranchList(remote.name);
    }
    // Only a new refresh triggers this; opening a folder has its own load path.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshGeneration]);

  const toggle = (remoteName: string) => {
    const willOpen = !isOpen(remoteName);
    setOpenRemotes((prev) => ({ ...prev, [remoteName]: willOpen }));
    persistOpen(remoteFolderKey(remoteName), willOpen);
    if (willOpen && remoteBranches[remoteName] === undefined) {
      fetchRemoteBranchList(remoteName);
    }
  };

  const reload = (remoteName: string) => {
    setRemoteBranches((prev) => {
      const next = { ...prev };
      delete next[remoteName];
      return next;
    });
    if (isOpen(remoteName)) fetchRemoteBranchList(remoteName);
  };

  const markOpen = (remoteName: string) => setOpenRemotes((prev) => ({ ...prev, [remoteName]: true }));

  const persistOpenAndLoad = (remoteName: string) => {
    persistOpen(remoteFolderKey(remoteName), true);
    if (remoteBranches[remoteName] === undefined) fetchRemoteBranchList(remoteName);
  };

  return {
    isOpen,
    toggle,
    branches: remoteBranches,
    error,
    clearError: () => setError(null),
    reload,
    markOpen,
    persistOpenAndLoad,
  };
}
