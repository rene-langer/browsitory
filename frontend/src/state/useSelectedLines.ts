import { useEffect, useState, type RefObject } from "react";
import { selectedLineSpan, type LineSpan } from "../lib/selectedLines";

/** The file lines currently selected inside `container` (`selectedLineSpan`), kept up to date
 *  as the selection changes. Off (always `null`) while `enabled` is false. */
export function useSelectedLines(container: RefObject<HTMLElement | null>, enabled: boolean): LineSpan | null {
  const [span, setSpan] = useState<LineSpan | null>(null);
  useEffect(() => {
    if (!enabled) return;
    const update = () => {
      const next = selectedLineSpan(container.current);
      setSpan((prev) => (prev?.start === next?.start && prev?.end === next?.end ? prev : next));
    };
    update();
    document.addEventListener("selectionchange", update);
    return () => document.removeEventListener("selectionchange", update);
  }, [container, enabled]);
  return enabled ? span : null;
}
