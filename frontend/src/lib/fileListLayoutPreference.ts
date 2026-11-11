import { useSyncExternalStore } from "react";

/**
 * How the working tree's file list shows its files (#729): one flat list of paths (the default)
 * or a tree of folders. A per-machine UI preference, stored like the commit search view's
 * (`searchViewPreference.ts`); every open repository tab follows one choice. The list's Flat |
 * Tree toggle and Preferences > Diff set it.
 */
export type FileListLayout = "flat" | "tree";

const STORAGE_KEY = "working-tree-file-layout";

const listeners = new Set<() => void>();

// What a blocked storage can't keep.
let fallback: FileListLayout | null = null;

export function readFileListLayout(): FileListLayout {
  try {
    return localStorage.getItem(STORAGE_KEY) === "tree" ? "tree" : "flat";
  } catch {
    return "flat";
  }
}

export function writeFileListLayout(layout: FileListLayout): void {
  try {
    localStorage.setItem(STORAGE_KEY, layout);
  } catch {
    // Blocked storage: the choice still applies until the app restarts.
    fallback = layout;
  }
  for (const listener of listeners) listener();
}

function snapshot(): FileListLayout {
  return fallback ?? readFileListLayout();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** The current choice, re-rendering every user when it changes. */
export function useFileListLayout(): FileListLayout {
  return useSyncExternalStore(subscribe, snapshot);
}
