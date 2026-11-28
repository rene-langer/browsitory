import { useSyncExternalStore } from "react";

/**
 * How the working tree shows its diffs (#793): the current file's diff only (the default) or
 * every shown file's diff stacked below the file list. A per-machine UI preference, stored like
 * the file list layout (`fileListLayoutPreference.ts`); every open repository tab follows one
 * choice. The List | Stacked toggle above the file list and Preferences > Diff set it.
 */
export type DiffLayout = "list" | "stacked";

const STORAGE_KEY = "working-tree-diff-layout";

const listeners = new Set<() => void>();

// What a blocked storage can't keep.
let fallback: DiffLayout | null = null;

export function readDiffLayout(): DiffLayout {
  try {
    return localStorage.getItem(STORAGE_KEY) === "stacked" ? "stacked" : "list";
  } catch {
    return "list";
  }
}

export function writeDiffLayout(layout: DiffLayout): void {
  try {
    localStorage.setItem(STORAGE_KEY, layout);
  } catch {
    // Blocked storage: the choice still applies until the app restarts.
    fallback = layout;
  }
  for (const listener of listeners) listener();
}

function snapshot(): DiffLayout {
  return fallback ?? readDiffLayout();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The current choice, re-rendering every user when it changes. */
export function useDiffLayout(): DiffLayout {
  return useSyncExternalStore(subscribe, snapshot);
}
