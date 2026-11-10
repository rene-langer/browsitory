// Whether the New branch form checks the new branch out (#656). A per-machine UI preference like
// the commit list's line stats (`lineStatsPreference.ts`), so it works in both frontends. On by
// default, as `git switch -c`; the form remembers the last choice.
const STORAGE_KEY = "new-branch-checkout";

// What a blocked storage can't keep.
let fallback: boolean | null = null;

export function readCheckoutNewBranch(): boolean {
  if (fallback !== null) return fallback;
  try {
    return localStorage.getItem(STORAGE_KEY) !== "off";
  } catch {
    return true;
  }
}

export function writeCheckoutNewBranch(checkout: boolean): void {
  fallback = null;
  try {
    localStorage.setItem(STORAGE_KEY, checkout ? "on" : "off");
  } catch {
    // Blocked storage: the choice still applies until the app restarts.
    fallback = checkout;
  }
}
