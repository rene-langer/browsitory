// Per repo, per machine: a UI convenience, not repository state. Same mechanism as the commit
// box's "Recent messages" (`commitMessageHistory.ts`), so it works in both frontends.
const storageKey = (repoPath: string) => `commit-searches:${repoPath}`;

/** How many searches are kept per repository. */
export const RECENT_SEARCH_LIMIT = 10;

/** The repository's recent searches (trimmed queries), newest first. */
export function readRecentSearches(repoPath: string): string[] {
  try {
    const stored = localStorage.getItem(storageKey(repoPath));
    if (stored === null) return [];
    const parsed: unknown = JSON.parse(stored);
    return Array.isArray(parsed)
      ? parsed.filter((q): q is string => typeof q === "string" && q.trim() !== "").slice(0, RECENT_SEARCH_LIMIT)
      : [];
  } catch {
    return [];
  }
}

export function writeRecentSearches(repoPath: string, searches: string[]): void {
  try {
    localStorage.setItem(storageKey(repoPath), JSON.stringify(searches.slice(0, RECENT_SEARCH_LIMIT)));
  } catch {
    // Blocked or full storage: the list just doesn't change.
  }
}

/** `query` in front of `searches`, without an earlier copy of it, at most `RECENT_SEARCH_LIMIT`. */
export function withRecentSearch(searches: string[], query: string): string[] {
  const q = query.trim();
  if (q === "") return searches;
  return [q, ...searches.filter((s) => s !== q)].slice(0, RECENT_SEARCH_LIMIT);
}
