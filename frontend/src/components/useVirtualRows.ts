import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

/** Rows rendered before the first measurement, and above and below the visible ones. */
export const INITIAL_ROWS = 60;
export const OVERSCAN_ROWS = 20;

export interface VirtualRows {
  /** The rows to render: `[start, end)`. */
  start: number;
  end: number;
  /** Space for the rows above `start` and below `end`, as padding on the list. */
  padTop: number;
  padBottom: number;
  /** True once a row's height is known and only the visible rows render. */
  windowed: boolean;
  /** Scrolls the least distance that shows row `index` (`block: "nearest"`). */
  scrollToIndex(index: number): void;
}

/** The nearest ancestor that scrolls vertically, else `null`. */
function scrollParentOf(element: HTMLElement): HTMLElement | null {
  for (let node = element.parentElement; node !== null; node = node.parentElement) {
    const overflow = getComputedStyle(node).overflowY;
    if (overflow === "auto" || overflow === "scroll") return node;
  }
  return null;
}

/**
 * Windowing for a long list of rows of one height (#685): only the rows in view, plus
 * `OVERSCAN_ROWS` either side, are rendered, and padding on the list stands in for the others,
 * so the scroll bar and positions are those of the whole list. The list scrolls inside its
 * nearest scrolling ancestor (the history pane), with other content above it.
 *
 * The row height is measured from the list's first `li` after the first render (before paint)
 * and again when that row resizes (a zoom or UI size change). Until a height can be measured
 * (a hidden tab, or a test environment without layout) every row renders, as before.
 */
export function useVirtualRows(list: HTMLElement | null, count: number): VirtualRows {
  const [rowHeight, setRowHeight] = useState(0);
  // Whether a measurement has run; until then only the first rows render.
  const [measuredOnce, setMeasuredOnce] = useState(false);
  const [range, setRange] = useState({ start: 0, end: INITIAL_ROWS });
  const parentRef = useRef<HTMLElement | null>(null);

  const measureRow = useCallback(() => {
    const row = list?.querySelector<HTMLElement>(":scope > li");
    // Fractional: a whole-pixel height would drift by a row every few hundred rows.
    const height = row?.getBoundingClientRect().height ?? 0;
    // Hidden (no layout): keep what was measured while shown.
    if (height > 0) setRowHeight(height);
    setMeasuredOnce(true);
  }, [list]);

  const updateRange = useCallback(() => {
    const parent = parentRef.current;
    if (list === null || parent === null || rowHeight === 0 || parent.clientHeight === 0) return;
    const listTop = list.getBoundingClientRect().top - parent.getBoundingClientRect().top + parent.scrollTop;
    const top = parent.scrollTop - listTop;
    const first = Math.max(0, Math.floor(top / rowHeight) - OVERSCAN_ROWS);
    const last = Math.min(count, Math.ceil((top + parent.clientHeight) / rowHeight) + OVERSCAN_ROWS);
    setRange((prev) => (prev.start === first && prev.end === last ? prev : { start: first, end: Math.max(first, last) }));
  }, [count, list, rowHeight]);

  useLayoutEffect(() => {
    if (list === null) return;
    parentRef.current = scrollParentOf(list);
    // Layout is only readable after rendering: measuring before paint is what this effect is for.
    // eslint-disable-next-line react-hooks/set-state-in-effect -- a DOM measurement, once per list
    measureRow();
  }, [list, measureRow]);

  useLayoutEffect(() => {
    updateRange();
  }, [updateRange]);

  useEffect(() => {
    const parent = parentRef.current;
    if (list === null || parent === null) return;
    parent.addEventListener("scroll", updateRange, { passive: true });
    let observer: ResizeObserver | null = null;
    let mutations: MutationObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      const resizes = new ResizeObserver(() => {
        measureRow();
        updateRange();
      });
      observer = resizes;
      resizes.observe(parent);
      const row = list.querySelector(":scope > li");
      if (row !== null) resizes.observe(row);

      // Content above the list in the same scroller (a sidebar section opening, a placeholder
      // replaced, #828) moves the list without a scroll or a resize of the scroller. It changes
      // the height of an element beside the list or beside one of its ancestors up to the
      // scroller, so those are observed; elements added there later too. A removed one changes
      // no observed height when it was the scroller's child, hence the range update on mutation.
      const containers: HTMLElement[] = [];
      for (let node = list.parentElement; node !== null; node = node.parentElement) {
        containers.push(node);
        if (node === parent) break;
      }
      for (const container of containers) {
        for (const child of container.children) if (child !== list) resizes.observe(child);
      }
      if (typeof MutationObserver !== "undefined") {
        mutations = new MutationObserver((records) => {
          for (const record of records) {
            for (const node of record.addedNodes) {
              if (node instanceof Element && node !== list) resizes.observe(node);
            }
          }
          updateRange();
        });
        for (const container of containers) mutations.observe(container, { childList: true });
      }
    }
    return () => {
      parent.removeEventListener("scroll", updateRange);
      observer?.disconnect();
      mutations?.disconnect();
    };
  }, [list, measureRow, updateRange]);

  const windowed = rowHeight > 0;
  const start = windowed ? Math.min(range.start, count) : 0;
  const end = windowed ? Math.min(Math.max(range.end, start), count) : measuredOnce ? count : Math.min(count, INITIAL_ROWS);

  const scrollToIndex = useCallback(
    (index: number) => {
      const parent = parentRef.current;
      if (list === null) return;
      if (parent === null || rowHeight === 0) {
        const row = list.querySelector<HTMLElement>(`:scope > li[data-row-index="${index}"]`);
        if (typeof row?.scrollIntoView === "function") row.scrollIntoView({ block: "nearest" });
        return;
      }
      const listTop = list.getBoundingClientRect().top - parent.getBoundingClientRect().top + parent.scrollTop;
      const rowTop = listTop + index * rowHeight;
      if (rowTop < parent.scrollTop) parent.scrollTop = rowTop;
      else if (rowTop + rowHeight > parent.scrollTop + parent.clientHeight)
        parent.scrollTop = rowTop + rowHeight - parent.clientHeight;
      updateRange();
    },
    [list, rowHeight, updateRange],
  );

  return {
    start,
    end,
    padTop: windowed ? start * rowHeight : 0,
    padBottom: windowed ? (count - end) * rowHeight : 0,
    windowed,
    scrollToIndex,
  };
}
