// Per repo, per machine, like the recent commit searches (`recentSearches.ts`): the branches the
// toolbar's switcher lists first (#638).
const storageKey = (repoPath: string) => `recent-branches:${repoPath}`;

/** How many branches are kept per repository. */
export const RECENT_BRANCH_LIMIT = 5;

/** The repository's recently switched-to branches, newest first. */
export function readRecentBranches(repoPath: string): string[] {
  try {
    const stored = localStorage.getItem(storageKey(repoPath));
    if (stored === null) return [];
    const parsed: unknown = JSON.parse(stored);
    return Array.isArray(parsed)
      ? parsed.filter((name): name is string => typeof name === "string" && name !== "").slice(0, RECENT_BRANCH_LIMIT)
      : [];
  } catch {
    return [];
  }
}

/** Puts `name` in front of the repository's recent branches. */
export function recordRecentBranch(repoPath: string, name: string): void {
  const next = [name, ...readRecentBranches(repoPath).filter((other) => other !== name)].slice(0, RECENT_BRANCH_LIMIT);
  try {
    localStorage.setItem(storageKey(repoPath), JSON.stringify(next));
  } catch {
    // Blocked or full storage: the list just doesn't change.
  }
}
