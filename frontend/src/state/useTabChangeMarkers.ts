import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { subscribeHostVisibility } from "../ipc/hostVisibility";
import type { RepoClient, StatusEntry } from "../ipc/RepoClient";
import { subscribeLastFetch } from "../lib/autoFetch";
import type { OpenRepo } from "./useOpenRepos";

export interface TabChangeMarkers {
  /** Open repositories known to have uncommitted changes (staged, unstaged or untracked). */
  changedPaths: ReadonlySet<string>;
  /** A mounted workspace's live status (the shown tab, and tabs kept in the background, #642). */
  onLiveStatus: (repoPath: string, status: StatusEntry[]) => void;
}

/**
 * The tab strip's "uncommitted changes" marker (#613). A mounted workspace reports its live
 * status (the shown tab follows changes, #650); every other tab is checked with `getStatus` when it
 * first shows up, when the user leaves it, when the window becomes visible again and after a
 * fetch (an automatic fetch tick included). No polling of its own. A failed check keeps the
 * marker as it was, and nothing waits for a check.
 */
function bump(generations: Map<string, number>, path: string): number {
  const next = (generations.get(path) ?? 0) + 1;
  generations.set(path, next);
  return next;
}

export function useTabChangeMarkers(client: RepoClient, openRepos: OpenRepo[], activePath: string | null): TabChangeMarkers {
  const [changed, setChanged] = useState<ReadonlyMap<string, boolean>>(new Map());
  // Per path, the newest check or live update: an older answer never overwrites a newer one.
  const generation = useRef(new Map<string, number>());
  const current = useRef({ openRepos, activePath });
  useEffect(() => {
    current.current = { openRepos, activePath };
  });

  const record = useCallback((path: string, dirty: boolean) => {
    setChanged((prev) => {
      if (prev.get(path) === dirty) return prev;
      const next = new Map(prev);
      next.set(path, dirty);
      return next;
    });
  }, []);

  const check = useCallback(
    (paths: string[]) => {
      for (const path of paths) {
        const ticket = bump(generation.current, path);
        client.getStatus(path).then(
          (status) => {
            if (generation.current.get(path) !== ticket) return;
            if (!current.current.openRepos.some((repo) => repo.path === path)) return;
            record(path, status.length > 0);
          },
          () => {},
        );
      }
    },
    [client, record],
  );

  const checkInactive = useCallback(() => {
    const { openRepos: repos, activePath: active } = current.current;
    check(repos.map((repo) => repo.path).filter((path) => path !== active));
  }, [check]);

  // Leaving a tab: its workspace stops checking for changes (or unmounts), so its status is
  // checked once more.
  const previousActive = useRef(activePath);
  useEffect(() => {
    const left = previousActive.current;
    previousActive.current = activePath;
    if (left !== null && left !== activePath && openRepos.some((repo) => repo.path === left)) check([left]);
  }, [activePath, openRepos, check]);

  // A tab that appears inactive (restored at launch, opened with a workspace) gets a first check.
  const seen = useRef(new Set<string>());
  useEffect(() => {
    const open = new Set(openRepos.map((repo) => repo.path));
    const fresh = [...open].filter((path) => !seen.current.has(path) && path !== activePath);
    for (const path of open) seen.current.add(path);
    for (const path of seen.current) {
      if (!open.has(path)) seen.current.delete(path);
    }
    if (fresh.length > 0) check(fresh);
  }, [openRepos, activePath, check]);

  useEffect(
    () =>
      subscribeHostVisibility((visible) => {
        if (visible) checkInactive();
      }),
    [checkInactive],
  );
  useEffect(() => subscribeLastFetch(checkInactive), [checkInactive]);

  const onLiveStatus = useCallback(
    (repoPath: string, status: StatusEntry[]) => {
      bump(generation.current, repoPath);
      record(repoPath, status.length > 0);
    },
    [record],
  );

  const changedPaths = useMemo(() => {
    const open = new Set(openRepos.map((repo) => repo.path));
    return new Set([...changed].filter(([path, dirty]) => dirty && open.has(path)).map(([path]) => path));
  }, [changed, openRepos]);

  return { changedPaths, onLiveStatus };
}
