// What the app keeps per repository in web storage, by key prefix (the repository path
// follows): recent searches (`recentSearches.ts`), recent commit messages
// (`commitMessageHistory.ts`), the last branches switched to (`recentBranches.ts`), the Prune
// choice (`fetchPrune.ts`) and Fetch automatically (`autoFetch.ts`). It goes when the
// repository leaves Recent repositories (#1096).
const REPO_KEY_PREFIXES = ["commit-searches:", "commit-messages:", "recent-branches:", "fetch-prune:", "auto-fetch:"];

/** Removes everything web storage keeps for `repoPath`. */
export function forgetRepoWebStorage(repoPath: string): void {
  try {
    for (const prefix of REPO_KEY_PREFIXES) localStorage.removeItem(prefix + repoPath);
  } catch {
    // Blocked storage holds nothing to remove.
  }
}

/** Removes what web storage keeps for every repository but those in `keep`. */
export function forgetRepoWebStorageExcept(keep: Iterable<string>): void {
  const kept = new Set(keep);
  try {
    const gone: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i);
      const prefix = key === null ? undefined : REPO_KEY_PREFIXES.find((p) => key.startsWith(p));
      if (key !== null && prefix !== undefined && !kept.has(key.slice(prefix.length))) gone.push(key);
    }
    for (const key of gone) localStorage.removeItem(key);
  } catch {
    // Blocked storage holds nothing to remove.
  }
}
