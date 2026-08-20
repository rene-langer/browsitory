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

  useEffect(() => {
    let ignore = false;
    client.listOpenRepos().then(({ paths, activePath: restoredActive }) => {
      if (ignore) return;
      setOpenRepos(paths.map((path) => ({ path, displayName: displayNameFor(path) })));
      setActivePath(restoredActive ?? paths[0] ?? null);
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

  return { openRepos, activePath, loading, openRepo, closeRepo, switchTo };
}
