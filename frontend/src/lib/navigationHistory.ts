import type { SelectedRow } from "../state/useAppState";

/**
 * The rows a tab visited, for Back and Forward (#618), like a browser's history: selecting a
 * row drops the entries after the current one and appends it.
 */
export interface NavigationHistory {
  entries: readonly SelectedRow[];
  /** The current entry; `-1` only while `entries` is empty. */
  index: number;
  /**
   * The current entry came from a keyboard move through a list, so the next such move replaces
   * it instead of adding one: arrowing through a list is one step back, not one per row.
   */
  merging: boolean;
}

/**
 * How a row was selected: `"step"` moves through a list by row (Up/Down, j/k, Page Up/Down,
 * Home/End); `"jump"` is everything else (a click, a link, Go to, parent/child, a search result).
 */
export type VisitKind = "jump" | "step";

/** Entries kept per tab; the oldest go first. */
export const NAVIGATION_HISTORY_LIMIT = 100;

export function initialNavigationHistory(row: SelectedRow): NavigationHistory {
  return { entries: [row], index: 0, merging: false };
}

function sameRow(a: SelectedRow, b: SelectedRow): boolean {
  if (a === "uncommitted" || b === "uncommitted") return a === b;
  return a.commitId === b.commitId;
}

/** `history` after selecting `row`; the same object when nothing changes. */
export function recordVisit(history: NavigationHistory, row: SelectedRow, kind: VisitKind): NavigationHistory {
  const { entries, index, merging } = history;
  const current = index >= 0 ? entries[index] : undefined;
  if (current !== undefined && sameRow(current, row)) {
    const nextMerging = kind === "step" && merging;
    return nextMerging === merging ? history : { ...history, merging: nextMerging };
  }
  if (kind === "step" && merging && index >= 0) {
    // Back on the row the keyboard moves started from: they cancel out.
    const previous = index > 0 ? entries[index - 1] : undefined;
    if (previous !== undefined && sameRow(previous, row)) {
      return { entries: entries.slice(0, index), index: index - 1, merging: false };
    }
    return { entries: [...entries.slice(0, index), row], index, merging: true };
  }
  const next = [...entries.slice(0, index + 1), row].slice(-NAVIGATION_HISTORY_LIMIT);
  return { entries: next, index: next.length - 1, merging: kind === "step" };
}

/**
 * The index Back (`-1`) or Forward (`1`) goes to: the nearest entry that way whose row
 * `isAvailable` (a commit no longer shown is skipped), or `null` at the end.
 */
export function navigationTarget(
  history: NavigationHistory,
  direction: -1 | 1,
  isAvailable: (row: SelectedRow) => boolean,
): number | null {
  for (let i = history.index + direction; i >= 0 && i < history.entries.length; i += direction) {
    if (isAvailable(history.entries[i])) return i;
  }
  return null;
}

/** `history` with `index` as the current entry. */
export function moveToEntry(history: NavigationHistory, index: number): NavigationHistory {
  return { ...history, index, merging: false };
}
