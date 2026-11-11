import { useEffect, useLayoutEffect, useRef } from "react";

/**
 * Keeps the scroll positions inside `root` across hiding it (#642). A tab in the background keeps
 * its workspace mounted but hidden with `display: none`, and a browser may put every scrolled
 * list inside back at the top when it is shown again. So the positions are recorded as they
 * change while `active`, and put back as soon as `active` turns true again.
 */
export function useScrollMemory(root: HTMLElement | null, active: boolean): void {
  const positions = useRef(new Map<Element, { top: number; left: number }>());
  const recording = useRef(active);
  useEffect(() => {
    recording.current = active;
  }, [active]);

  useEffect(() => {
    if (root === null) return;
    // Scroll events don't bubble; listening in the capture phase sees every list inside.
    const onScroll = (event: Event) => {
      const target = event.target;
      if (!recording.current || !(target instanceof Element)) return;
      positions.current.set(target, { top: target.scrollTop, left: target.scrollLeft });
    };
    root.addEventListener("scroll", onScroll, true);
    return () => root.removeEventListener("scroll", onScroll, true);
  }, [root]);

  useLayoutEffect(() => {
    if (!active || root === null) return;
    for (const [element, { top, left }] of positions.current) {
      if (!element.isConnected || !root.contains(element)) {
        positions.current.delete(element);
        continue;
      }
      if (element.scrollTop !== top || element.scrollLeft !== left) element.scrollTo?.({ top, left });
    }
  }, [active, root]);
}
