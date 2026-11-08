// "Fetch automatically" (#440): the per-repository override of the app-wide interval, and when
// each repository was last fetched in this session.

/** The intervals the UI offers, in minutes. */
export const AUTO_FETCH_CHOICES = [5, 10, 30, 60] as const;

/** What a repository uses: `null` follows the app setting, 0 is off, else minutes. */
export type AutoFetchOverride = number | null;

// Per repo, per machine: a UI preference, like the Prune choice (`fetchPrune.ts`), so it works
// in both frontends without a config.toml list.
const storageKey = (repoPath: string) => `auto-fetch:${repoPath}`;

type Listener = () => void;
const overrideListeners = new Set<Listener>();

export function readAutoFetchOverride(repoPath: string): AutoFetchOverride {
  try {
    const stored = localStorage.getItem(storageKey(repoPath));
    if (stored === null) return null;
    if (stored === "off") return 0;
    const minutes = Number(stored);
    return Number.isInteger(minutes) && minutes > 0 ? minutes : null;
  } catch {
    return null;
  }
}

export function writeAutoFetchOverride(repoPath: string, value: AutoFetchOverride): void {
  try {
    if (value === null) localStorage.removeItem(storageKey(repoPath));
    else localStorage.setItem(storageKey(repoPath), value <= 0 ? "off" : String(Math.round(value)));
  } catch {
    // Blocked storage: nothing to remember; the app setting keeps applying.
  }
  for (const listener of overrideListeners) listener();
}

/** Calls `listener` after any repository's override changed in this window. */
export function subscribeAutoFetchOverride(listener: Listener): () => void {
  overrideListeners.add(listener);
  return () => {
    overrideListeners.delete(listener);
  };
}

/** The interval in minutes a repository fetches at; 0 is off. */
export function effectiveAutoFetchMinutes(override: AutoFetchOverride, appMinutes: number): number {
  const minutes = override ?? appMinutes;
  return Number.isFinite(minutes) && minutes > 0 ? minutes : 0;
}

/** "Every 10 minutes" / "Off". */
export function autoFetchLabel(minutes: number): string {
  if (minutes <= 0) return "Off";
  if (minutes === 60) return "Every hour";
  return minutes === 1 ? "Every minute" : `Every ${minutes} minutes`;
}

// When each repository was last fetched (manually or in the background), and when the
// background fetch last tried, in this session. Module state, not React state: only the active
// tab's workspace is mounted, and switching tabs remounts it.
const lastFetched = new Map<string, number>();
const lastAttempt = new Map<string, number>();
const fetchListeners = new Set<Listener>();

export function recordFetch(repoPath: string, at: number = Date.now()): void {
  lastFetched.set(repoPath, at);
  for (const listener of fetchListeners) listener();
}

/** Milliseconds since the epoch, or `null` when the repository wasn't fetched this session. */
export function lastFetchAt(repoPath: string): number | null {
  return lastFetched.get(repoPath) ?? null;
}

export function subscribeLastFetch(listener: Listener): () => void {
  fetchListeners.add(listener);
  return () => {
    fetchListeners.delete(listener);
  };
}

/** When the background schedule last started counting for this repository (an attempt). */
export function lastAutoFetchAttempt(repoPath: string): number | null {
  return lastAttempt.get(repoPath) ?? null;
}

export function recordAutoFetchAttempt(repoPath: string, at: number = Date.now()): void {
  lastAttempt.set(repoPath, at);
}

const noticed = new Set<string>();

/** True the first time `remoteName` of `repoPath` is reported this session, false after. */
export function firstNotice(repoPath: string, remoteName: string): boolean {
  const key = `${repoPath}\u0000${remoteName}`;
  if (noticed.has(key)) return false;
  noticed.add(key);
  return true;
}

/** Test-only: forget the session state. */
export function __resetAutoFetchForTests(): void {
  lastFetched.clear();
  lastAttempt.clear();
  noticed.clear();
  fetchListeners.clear();
  overrideListeners.clear();
}
