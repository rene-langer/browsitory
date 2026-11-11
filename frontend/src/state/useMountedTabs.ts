import { useState } from "react";

/**
 * How many tabs keep their workspace mounted (#642): the shown one and the ones shown last. A tab
 * switched back to within that many keeps its selection, scroll position, open panels, search and
 * file history, and isn't read again unless the repository changed meanwhile; an older one is
 * unmounted, so many open tabs don't each hold their history and views.
 */
export const MOUNTED_TABS = 4;

/**
 * The open tabs whose workspace stays mounted, most recently shown first: the active tab, then
 * the tabs shown before it, at most `limit`. A tab never shown since it opened isn't mounted.
 */
export function useMountedTabs(openPaths: readonly string[], activePath: string | null, limit = MOUNTED_TABS): string[] {
  const [recent, setRecent] = useState<string[]>([]);
  const open = new Set(openPaths);
  let next = recent.filter((path) => open.has(path));
  if (activePath !== null && open.has(activePath)) next = [activePath, ...next.filter((path) => path !== activePath)];
  next = next.slice(0, limit);
  const changed = next.length !== recent.length || next.some((path, i) => path !== recent[i]);
  // Adjusting state while rendering (React's "storing information from previous renders"), so the
  // newly shown tab mounts in this very render.
  if (changed) setRecent(next);
  return next;
}
