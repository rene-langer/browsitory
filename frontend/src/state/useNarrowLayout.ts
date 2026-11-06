import { useEffect, useState } from "react";

const NARROW_BREAKPOINT = 900;

/** True while the window is narrower than the breakpoint (`RepoWorkspace`'s layout). */
export function useNarrowLayout(): boolean {
  // Below this width the sidebar auto-collapses so the graph and diff panes both stay usable.
  const [narrow, setNarrow] = useState(() => window.innerWidth < NARROW_BREAKPOINT);
  useEffect(() => {
    const onResize = () => setNarrow(window.innerWidth < NARROW_BREAKPOINT);
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  return narrow;
}
