import { useEffect, useState } from "react";

/** How far outside the scrolling stack a diff starts loading (#793). */
const NEAR_MARGIN = "400px 0px";

/**
 * Whether `element` is within `NEAR_MARGIN` of what `root` (a scrolling container) shows, kept
 * up to date while `enabled`. The root is passed explicitly: with the viewport as root, an inner
 * scroller clips the element first and the margin never applies. Without a root yet, nothing is
 * near.
 */
export function useNearView(element: Element | null, root: Element | null, enabled: boolean): boolean {
  const [near, setNear] = useState(false);
  useEffect(() => {
    if (!enabled || element === null || root === null) return;
    const observer = new IntersectionObserver(
      (entries) => {
        const last = entries[entries.length - 1];
        if (last !== undefined) setNear(last.isIntersecting);
      },
      { root, rootMargin: NEAR_MARGIN },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [element, root, enabled]);
  return enabled && near;
}
