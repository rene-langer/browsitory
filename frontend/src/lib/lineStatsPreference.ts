import { useSyncExternalStore } from "react";

// Whether the commit list shows its lines-changed badges (#620). A per-machine UI preference,
// stored like the fetch Prune choice (`fetchPrune.ts`), so it works in both frontends. On by
// default. Every open repository tab follows one choice.
const STORAGE_KEY = "commit-list-line-stats";

const listeners = new Set<() => void>();

// What a blocked storage can't keep.
let fallback: boolean | null = null;

export function readShowLineStats(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) !== "off";
  } catch {
    return true;
  }
}

export function writeShowLineStats(show: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, show ? "on" : "off");
  } catch {
    // Blocked storage: the choice still applies until the app restarts.
    fallback = show;
  }
  for (const listener of listeners) listener();
}

function snapshot(): boolean {
  return fallback ?? readShowLineStats();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The current choice, re-rendering every user when it changes. */
export function useShowLineStats(): boolean {
  return useSyncExternalStore(subscribe, snapshot);
}
