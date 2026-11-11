import { useSyncExternalStore } from "react";

/**
 * How a commit search shows its matches (#687): highlighted in the graph (the default) or as a
 * flat list of the matching commits. A per-machine UI preference, stored like the lines-changed
 * badges' (`lineStatsPreference.ts`); every open repository tab follows one choice.
 */
export type SearchView = "graph" | "list";

const STORAGE_KEY = "commit-search-view";

const listeners = new Set<() => void>();

// What a blocked storage can't keep.
let fallback: SearchView | null = null;

export function readSearchView(): SearchView {
  try {
    return localStorage.getItem(STORAGE_KEY) === "list" ? "list" : "graph";
  } catch {
    return "graph";
  }
}

export function writeSearchView(view: SearchView): void {
  try {
    localStorage.setItem(STORAGE_KEY, view);
  } catch {
    // Blocked storage: the choice still applies until the app restarts.
    fallback = view;
  }
  for (const listener of listeners) listener();
}

function snapshot(): SearchView {
  return fallback ?? readSearchView();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The current choice, re-rendering every user when it changes. */
export function useSearchView(): SearchView {
  return useSyncExternalStore(subscribe, snapshot);
}
