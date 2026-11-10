import { useCallback, useEffect, useRef, useState } from "react";
import type { RepoClient, Workspace } from "../ipc/RepoClient";

export interface OpenRepo {
  path: string;
  displayName: string;
  workspaceId: string | null;
}

export interface UseOpenReposResult {
  openRepos: OpenRepo[];
  activePath: string | null;
  loading: boolean;
  // Set when the mount-time restore itself failed (see the `.catch` on the restore effect).
  // `App` renders it next to its own `openError`, so a failed restore is visible rather than
  // silently indistinguishable from "no tabs were persisted".
  restoreError: string | null;
  // Clears `restoreError` without waiting for anything else to happen — its dismiss control
  // (issue #30/UX-002).
  dismissRestoreError(): void;
  openRepo(path: string): Promise<void>;
  openWorkspace(workspace: Workspace): Promise<void>;
  closeRepo(path: string): void;
  switchTo(path: string): void;
  /** Whether "Reopen closed tab" has a tab to bring back (#613). */
  canReopenClosedRepo: boolean;
  /**
   * Reopens the most recently closed tab at its old position and in its old workspace group, and
   * activates it. A closed repository that no longer opens is dropped and the next one back is
   * tried; the dropped ones are returned, for the caller to report.
   */
  reopenClosedRepo(): Promise<ReopenFailure[]>;
}

export interface ReopenFailure {
  path: string;
  message: string;
}

interface ClosedRepo {
  path: string;
  workspaceId: string | null;
  index: number;
}

// How many closed tabs the session remembers, newest last (#613).
const CLOSED_STACK_LIMIT = 10;

// Idempotent for the same entry, as `closeRepo`'s state updater may run twice.
function pushClosed(stack: ClosedRepo[], closed: ClosedRepo): ClosedRepo[] {
  return [...stack.filter((entry) => entry.path !== closed.path), closed].slice(-CLOSED_STACK_LIMIT);
}

// The old position, moved next to the workspace group's other open tabs when it would land
// outside the group (`RepoTabs` groups contiguous tabs only).
function reopenIndex(repos: OpenRepo[], closed: ClosedRepo): number {
  const index = Math.min(closed.index, repos.length);
  if (closed.workspaceId === null) return index;
  const members = repos.flatMap((repo, i) => (repo.workspaceId === closed.workspaceId ? [i] : []));
  if (members.length === 0) return index;
  return Math.min(Math.max(index, members[0]), members[members.length - 1] + 1);
}

function displayNameFor(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const segments = trimmed.split(/[\\/]/);
  return segments[segments.length - 1] || trimmed;
}

function toEntries(repos: OpenRepo[]): { path: string; workspaceId: string | null }[] {
  return repos.map((repo) => ({ path: repo.path, workspaceId: repo.workspaceId }));
}

export function useOpenRepos(client: RepoClient): UseOpenReposResult {
  const [openRepos, setOpenRepos] = useState<OpenRepo[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const closedRepos = useRef<ClosedRepo[]>([]);
  const [closedCount, setClosedCount] = useState(0);

  useEffect(() => {
    let ignore = false;
    client.listOpenRepos().then(async ({ entries, activePath: restoredActive }) => {
      if (ignore) return;
      // `listOpenRepos` only reports what *was* open — the backend's worker registry starts
      // empty every launch (a fresh process), so each restored path has to be (re-)opened for
      // real, the same as a user-driven `openRepo` does, or its `RepoWorkspace` would render
      // with a live tab bound to a worker that was never spawned (every fetch permanently
      // failing with "repo not open"). A path that fails to reopen (removed/renamed/permissions
      // changed since last launch) is dropped rather than kept as a tab stuck in that state.
      const reopened = await Promise.all(
        entries.map((entry) => client.openRepo(entry.path).then(() => entry, () => null)),
      );
      if (ignore) return;
      const restored = reopened.filter((entry): entry is { path: string; workspaceId: string | null } => entry !== null);
      setOpenRepos(restored.map((entry) => ({
        path: entry.path,
        displayName: displayNameFor(entry.path),
        workspaceId: entry.workspaceId,
      })));
      setActivePath(
        restoredActive !== null && restored.some((entry) => entry.path === restoredActive)
          ? restoredActive
          : restored[0]?.path ?? null,
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
      void client.persistOpenRepos(toEntries(repos), active);
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
          : [...prev, { path, displayName: displayNameFor(path), workspaceId: null }];
        persist(next, path);
        return next;
      });
      setActivePath(path);
    },
    [client, persist],
  );

  const openWorkspace = useCallback(
    async (workspace: Workspace) => {
      const opened: string[] = [];
      for (const path of workspace.memberPaths) {
        try {
          await client.openRepo(path);
          opened.push(path);
        } catch {
          // Skipped, same silent-drop rule the mount-time restore uses for a path that fails
          // to (re-)open — moved, deleted, or permissions changed since the workspace was saved.
        }
      }
      if (opened.length === 0) return;
      setOpenRepos((prev) => {
        const openedPaths = new Set(opened);
        const existingByPath = new Map(prev.map((repo) => [repo.path, repo] as const));
        const workspaceRepos = opened.map((path) => {
          const existing = existingByPath.get(path);
          return existing === undefined
            ? { path, displayName: displayNameFor(path), workspaceId: workspace.id }
            : { ...existing, workspaceId: workspace.id };
        });
        const firstExistingMemberIndex = prev.findIndex((repo) => openedPaths.has(repo.path));
        const remaining = prev.filter((repo) => !openedPaths.has(repo.path));
        const insertionIndex = firstExistingMemberIndex === -1 ? remaining.length : firstExistingMemberIndex;
        const next = [
          ...remaining.slice(0, insertionIndex),
          ...workspaceRepos,
          ...remaining.slice(insertionIndex),
        ];
        persist(next, opened[opened.length - 1]);
        return next;
      });
      setActivePath(opened[opened.length - 1]);
    },
    [client, persist],
  );

  const closeRepo = useCallback(
    (path: string) => {
      void client.closeRepo(path);
      setOpenRepos((prev) => {
        const closingIndex = prev.findIndex((r) => r.path === path);
        const next = prev.filter((r) => r.path !== path);
        if (closingIndex !== -1) {
          const closed = { path, workspaceId: prev[closingIndex].workspaceId, index: closingIndex };
          closedRepos.current = pushClosed(closedRepos.current, closed);
          setClosedCount(closedRepos.current.length);
        }
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

  const reopenClosedRepo = useCallback(async () => {
    const failures: ReopenFailure[] = [];
    for (let closed = closedRepos.current.pop(); closed !== undefined; closed = closedRepos.current.pop()) {
      const reopening = closed;
      try {
        await client.openRepo(reopening.path);
      } catch (error) {
        failures.push({ path: reopening.path, message: String(error) });
        continue;
      }
      setOpenRepos((prev) => {
        if (prev.some((repo) => repo.path === reopening.path)) {
          persist(prev, reopening.path);
          return prev;
        }
        const reopened = { path: reopening.path, displayName: displayNameFor(reopening.path), workspaceId: reopening.workspaceId };
        const index = reopenIndex(prev, reopening);
        const next = [...prev.slice(0, index), reopened, ...prev.slice(index)];
        persist(next, reopening.path);
        return next;
      });
      setActivePath(reopening.path);
      break;
    }
    setClosedCount(closedRepos.current.length);
    return failures;
  }, [client, persist]);

  const dismissRestoreError = useCallback(() => {
    setRestoreError(null);
  }, []);

  return {
    openRepos,
    activePath,
    loading,
    restoreError,
    dismissRestoreError,
    openRepo,
    closeRepo,
    switchTo,
    openWorkspace,
    canReopenClosedRepo: closedCount > 0,
    reopenClosedRepo,
  };
}
