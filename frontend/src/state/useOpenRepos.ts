import { useCallback, useEffect, useState } from "react";
import type { RepoClient } from "../ipc/RepoClient";

export interface OpenRepo {
  path: string;
  displayName: string;
}

export interface UseOpenReposResult {
  openRepos: OpenRepo[];
  activePath: string | null;
  loading: boolean;
  // Set when the mount-time restore itself failed (see the `.catch` on the restore effect).
  // `App` renders it next to its own `openError`, so a failed restore is visible rather than
  // silently indistinguishable from "no tabs were persisted".
  restoreError: string | null;
  openRepo(path: string): Promise<void>;
  closeRepo(path: string): void;
  switchTo(path: string): void;
}

function displayNameFor(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const segments = trimmed.split(/[\\/]/);
  return segments[segments.length - 1] || trimmed;
}

export function useOpenRepos(client: RepoClient): UseOpenReposResult {
  const [openRepos, setOpenRepos] = useState<OpenRepo[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [restoreError, setRestoreError] = useState<string | null>(null);

  useEffect(() => {
    let ignore = false;
    client.listOpenRepos().then(async ({ paths, activePath: restoredActive }) => {
      if (ignore) return;
      // `listOpenRepos` only reports what *was* open — the backend's worker registry starts
      // empty every launch (a fresh process), so each restored path has to be (re-)opened for
      // real, the same as a user-driven `openRepo` does, or its `RepoWorkspace` would render
      // with a live tab bound to a worker that was never spawned (every fetch permanently
      // failing with "repo not open"). A path that fails to reopen (removed/renamed/permissions
      // changed since last launch) is dropped rather than kept as a tab stuck in that state.
      const reopened = await Promise.all(
        paths.map((path) => client.openRepo(path).then(() => path, () => null)),
      );
      if (ignore) return;
      const restoredPaths = reopened.filter((path): path is string => path !== null);
      setOpenRepos(restoredPaths.map((path) => ({ path, displayName: displayNameFor(path) })));
      setActivePath(
        restoredActive !== null && restoredPaths.includes(restoredActive)
          ? restoredActive
          : restoredPaths[0] ?? null,
      );
      setLoading(false);
    }).catch((error: unknown) => {
      if (ignore) return;
      // Without this, a rejected `listOpenRepos` (unreadable/corrupt config, IPC failure) would
      // leave `loading` stuck at `true` forever — and `App` renders nothing at all while loading,
      // i.e. a permanently blank window with no error and no way back short of editing
      // config.toml by hand. Clearing `loading` instead falls through to the empty-state
      // `RepoPicker` (no tabs were restored), with the error surfaced next to it.
      setRestoreError(String(error));
      setLoading(false);
    });
    return () => {
      ignore = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const persist = useCallback(
    (repos: OpenRepo[], active: string | null) => {
      void client.persistOpenRepos(repos.map((r) => r.path), active);
    },
    [client],
  );

  const openRepo = useCallback(
    async (path: string) => {
      // Let a rejection from client.openRepo propagate to the caller: don't touch
      // openRepos/activePath/persist if the repo failed to open (e.g. not a git repo).
      await client.openRepo(path);
      setOpenRepos((prev) => {
        const next = prev.some((r) => r.path === path)
          ? prev
          : [...prev, { path, displayName: displayNameFor(path) }];
        persist(next, path);
        return next;
      });
      setActivePath(path);
    },
    [client, persist],
  );

  const closeRepo = useCallback(
    (path: string) => {
      void client.closeRepo(path);
      setOpenRepos((prev) => {
        const closingIndex = prev.findIndex((r) => r.path === path);
        const next = prev.filter((r) => r.path !== path);
        setActivePath((prevActive) => {
          if (prevActive !== path) {
            persist(next, prevActive);
            return prevActive;
          }
          const nextActive = next[closingIndex]?.path ?? next[closingIndex - 1]?.path ?? null;
          persist(next, nextActive);
          return nextActive;
        });
        return next;
      });
    },
    [client, persist],
  );

  const switchTo = useCallback(
    (path: string) => {
      setActivePath(path);
      persist(openRepos, path);
    },
    [openRepos, persist],
  );

  return { openRepos, activePath, loading, restoreError, openRepo, closeRepo, switchTo };
}
