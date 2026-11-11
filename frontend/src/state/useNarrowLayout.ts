import { useEffect, useState } from "react";

export const NARROW_BREAKPOINT = 900;

/**
 * True while `element` (the workspace body) is narrower than the breakpoint: the sidebar then
 * opens as a drawer instead of docking (`RepoWorkspace`, #635). The container, not the window,
 * because a VS Code webview or a split editor group can be far narrower than the window. Without
 * `ResizeObserver` (or before the element mounts) the window's width stands in.
 */
export function useNarrowLayout(element: HTMLElement | null = null): boolean {
  const [narrow, setNarrow] = useState(() => window.innerWidth < NARROW_BREAKPOINT);
  useEffect(() => {
    if (element !== null && typeof ResizeObserver !== "undefined") {
      const observer = new ResizeObserver((entries) => {
        const entry = entries[entries.length - 1];
        // No width at all: the workspace of a tab in the background (#642), hidden, not narrow.
        if (entry !== undefined && entry.contentRect.width > 0) setNarrow(entry.contentRect.width < NARROW_BREAKPOINT);
      });
      observer.observe(element);
      return () => observer.disconnect();
    }
    const onResize = () => setNarrow(window.innerWidth < NARROW_BREAKPOINT);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, [element]);
  return narrow;
}
