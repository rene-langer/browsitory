import type { RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";

// Per repo, per machine: a UI preference, not repository state (see the 4b design). Same
// mechanism as the branch tree's folder state (`persistedOpenState.ts`), so it works in both
// frontends.
const storageKey = (repoPath: string) => `fetch-prune:${repoPath}`;

/** The user's explicit Prune choice for this repo, or `null` when git config should decide. */
export function readFetchPrune(repoPath: string): boolean | null {
  try {
    const stored = localStorage.getItem(storageKey(repoPath));
    return stored === "on" ? true : stored === "off" ? false : null;
  } catch {
    return null;
  }
}

export function writeFetchPrune(repoPath: string, prune: boolean): void {
  try {
    localStorage.setItem(storageKey(repoPath), prune ? "on" : "off");
  } catch {
    // Blocked storage: the in-memory choice still applies for this session.
  }
}

/**
 * What the Prune checkbox shows: the explicit choice, else the git-config default of the sync
 * bar's fetch target (the upstream's remote, else the first remote).
 */
export function effectiveFetchPrune(
  override: boolean | null,
  remotes: RemoteInfo[],
  upstream: UpstreamInfo | null,
): boolean {
  if (override !== null) return override;
  const target = upstream?.remoteName ?? remotes[0]?.name;
  return remotes.find((remote) => remote.name === target)?.prune ?? false;
}
