import { useCallback, useEffect, useRef, useState } from "react";
import type { CommitStats, RepoClient } from "../ipc/RepoClient";

/** Commits asked for in one `getCommitStats` call; the backend's time budget may answer fewer. */
export const LINE_STATS_BATCH = 40;

const EMPTY: ReadonlyMap<string, CommitStats | null> = new Map();

/** The counts of one repository, through one client. */
interface Counted {
  client: RepoClient;
  repoPath: string;
  map: Map<string, CommitStats | null>;
}

/** The batching state, kept across renders and only touched from callbacks. */
interface Batcher {
  cache: Counted | null;
  /** The commits on screen, `null` once the hook unmounted. */
  wanted: { client: RepoClient; repoPath: string; ids: readonly string[] } | null;
  inFlight: boolean;
}

/**
 * Sends the next batch of wanted commits not counted yet, unless one is in flight; when it
 * settles, the next one. `publish` gets the counts after each batch.
 */
function pump(batcher: Batcher, publish: (counted: Counted) => void) {
  const target = batcher.wanted;
  if (target === null || batcher.inFlight) return;
  let current = batcher.cache;
  if (current === null || current.client !== target.client || current.repoPath !== target.repoPath) {
    current = { client: target.client, repoPath: target.repoPath, map: new Map() };
    batcher.cache = current;
  }
  const counted = current;
  const known = counted.map;
  const batch = target.ids.filter((id) => !known.has(id)).slice(0, LINE_STATS_BATCH);
  if (batch.length === 0) return;
  batcher.inFlight = true;
  Promise.resolve()
    .then(() => target.client.getCommitStats(target.repoPath, batch, false))
    .then((entries) => {
      const asked = new Set(batch);
      for (const entry of entries) {
        if (asked.has(entry.commitId)) known.set(entry.commitId, entry.stats);
      }
      // The backend always answers the first commit; never ask for it again if it didn't.
      if (!known.has(batch[0])) known.set(batch[0], null);
    })
    .catch(() => {
      // A nicety, like the commit header's signature: this batch gets no badges.
      for (const id of batch) known.set(id, null);
    })
    .finally(() => {
      batcher.inFlight = false;
      if (batcher.cache === counted) publish({ ...counted, map: new Map(known) });
      pump(batcher, publish);
    });
}

export interface CommitLineStats {
  /** Counted commits; `null` for one over the backend's cap (it gets no badge). A commit not in
   *  the map has no numbers yet. */
  stats: ReadonlyMap<string, CommitStats | null>;
  /**
   * The commits now on screen, in graph order. Counts the ones not known yet, one batch at a time
   * (one call in flight, so the repository's worker serves other requests between batches). A
   * commit that scrolled away before its batch went out is not counted.
   */
  request: (commitIds: readonly string[]) => void;
}

/**
 * The commit list's lines-changed badges (#620): counted lazily for the visible rows and cached
 * per commit id, since a commit's contents never change. A failed batch leaves its commits
 * without numbers rather than retrying in a loop.
 */
export function useCommitLineStats(client: RepoClient, repoPath: string): CommitLineStats {
  const [counted, setCounted] = useState<Counted | null>(null);
  const batcher = useRef<Batcher>({ cache: null, wanted: null, inFlight: false });

  useEffect(() => {
    const current = batcher.current;
    return () => {
      current.wanted = null;
    };
  }, []);

  const request = useCallback(
    (commitIds: readonly string[]) => {
      batcher.current.wanted = { client, repoPath, ids: commitIds };
      pump(batcher.current, setCounted);
    },
    [client, repoPath],
  );

  const stats = counted !== null && counted.client === client && counted.repoPath === repoPath ? counted.map : EMPTY;
  return { stats, request };
}
