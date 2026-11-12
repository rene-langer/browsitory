import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { INITIAL_ROWS, OVERSCAN_ROWS } from "./useVirtualRows";

export interface WindowedListbox {
  /** The options to render: `[start, end)`. */
  start: number;
  end: number;
  /** Space for what is above `start` and below `end`, as padding on the list. */
  padTop: number;
  padBottom: number;
  /** True once an option's height is known and only the visible options render. */
  windowed: boolean;
  /**
   * Scrolls the least distance that shows option `index` (`block: "nearest"`), rendering it
   * first when it is outside the window.
   */
  scrollToIndex(index: number): void;
}

/** How many of the ascending `sorted` are below `value`. */
function countBelow(sorted: readonly number[], value: number): number {
  let low = 0;
  let high = sorted.length;
  while (low < high) {
    const mid = (low + high) >> 1;
    if (sorted[mid] < value) low = mid + 1;
    else high = mid;
  }
  return low;
}

/**
 * Windowing for a listbox that is its own scroller, with group headings between its options:
 * the command palette (#864). Only the options in view, plus `OVERSCAN_ROWS` either side, and
 * the headings among them are rendered; padding on the list stands in for the others, so the
 * scroll bar and positions are those of the whole list. `useVirtualRows` does the same for a
 * list inside a scrolling pane whose rows are all one height.
 *
 * Every option is one height, every heading another. `headings` lists, ascending, the indexes
 * of the options a heading sits right above. The rendered options are `li[role="option"]`
 * children of `list` carrying `data-row-index`, the headings `li[role="presentation"]`
 * children. Both heights are measured after rendering (before paint) and again when the list
 * resizes; a heading not yet rendered counts as an option's height until one is. Until an
 * option's height can be measured (a test environment without layout) every option renders.
 */
export function useWindowedListbox(
  list: HTMLElement | null,
  count: number,
  headings: readonly number[],
): WindowedListbox {
  const [sizes, setSizes] = useState({ option: 0, heading: 0 });
  // Whether a measurement has run; until then only the first options render.
  const [measuredOnce, setMeasuredOnce] = useState(false);
  const [range, setRange] = useState({ start: 0, end: INITIAL_ROWS });
  // An option to scroll to exactly once it is rendered (`scrollToIndex`).
  const pendingScroll = useRef<number | null>(null);

  const headingHeight = sizes.heading > 0 ? sizes.heading : sizes.option;
  /** The top of option `index`'s slot (its heading, if any, then the option) in the list. */
  const slotTop = useCallback(
    (index: number) => index * sizes.option + countBelow(headings, index) * headingHeight,
    [headings, headingHeight, sizes.option],
  );
  /** The option whose slot holds the list's position `y`, clamped to the options. */
  const indexAt = useCallback(
    (y: number) => {
      let low = 0;
      let high = Math.max(0, count - 1);
      while (low < high) {
        const mid = (low + high + 1) >> 1;
        if (slotTop(mid) <= y) low = mid;
        else high = mid - 1;
      }
      return low;
    },
    [count, slotTop],
  );

  const measure = useCallback(() => {
    if (list === null) return;
    const option = list.querySelector<HTMLElement>(':scope > li[role="option"]');
    const heading = list.querySelector<HTMLElement>(':scope > li[role="presentation"]');
    // Fractional: a whole-pixel height would drift by a row every few hundred rows.
    const optionHeight = option?.getBoundingClientRect().height ?? 0;
    const headingHeightNow = heading?.getBoundingClientRect().height ?? 0;
    setSizes((prev) => {
      // Hidden (no layout): keep what was measured while shown.
      const next = {
        option: optionHeight > 0 ? optionHeight : prev.option,
        heading: headingHeightNow > 0 ? headingHeightNow : prev.heading,
      };
      return next.option === prev.option && next.heading === prev.heading ? prev : next;
    });
    setMeasuredOnce(true);
  }, [list]);

  const updateRange = useCallback(() => {
    if (list === null || sizes.option === 0 || list.clientHeight === 0) return;
    const top = list.scrollTop;
    const first = Math.max(0, indexAt(top) - OVERSCAN_ROWS);
    const last = Math.min(count, indexAt(top + list.clientHeight) + 1 + OVERSCAN_ROWS);
    setRange((prev) => (prev.start === first && prev.end === last ? prev : { start: first, end: Math.max(first, last) }));
  }, [count, indexAt, list, sizes.option]);

  const windowed = sizes.option > 0;
  const start = windowed ? Math.min(range.start, count) : 0;
  const end = windowed ? Math.min(Math.max(range.end, start), count) : measuredOnce ? count : Math.min(count, INITIAL_ROWS);

  // Layout is only readable after rendering: measuring before paint is what this effect is for.
  // Again for each new window, which may hold the first heading seen.
  useLayoutEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- a DOM measurement
    measure();
  }, [measure, start, end, count]);

  useLayoutEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect -- reads the measured layout
    updateRange();
  }, [updateRange]);

  // `scrollToIndex` for an option outside the window: now rendered, scroll to it exactly.
  useLayoutEffect(() => {
    const index = pendingScroll.current;
    if (index === null || list === null || index < start || index >= end) return;
    pendingScroll.current = null;
    const option = list.querySelector<HTMLElement>(`:scope > li[data-row-index="${index}"]`);
    if (typeof option?.scrollIntoView === "function") option.scrollIntoView({ block: "nearest" });
  });

  useEffect(() => {
    if (list === null) return;
    list.addEventListener("scroll", updateRange, { passive: true });
    let observer: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      observer = new ResizeObserver(() => {
        measure();
        updateRange();
      });
      observer.observe(list);
    }
    return () => {
      list.removeEventListener("scroll", updateRange);
      observer?.disconnect();
    };
  }, [list, measure, updateRange]);

  // The latest geometry, for a `scrollToIndex` that keeps one identity.
  const latest = useRef({ list, start, end, windowed, slotTop, updateRange, headings, headingHeight, option: sizes.option });
  useLayoutEffect(() => {
    latest.current = { list, start, end, windowed, slotTop, updateRange, headings, headingHeight, option: sizes.option };
  });

  const scrollToIndex = useCallback((index: number) => {
    const { list: element, start: first, end: last, windowed: isWindowed, slotTop: top, updateRange: update } =
      latest.current;
    if (element === null) return;
    const rendered = element.querySelector<HTMLElement>(`:scope > li[data-row-index="${index}"]`);
    if (rendered !== null || !isWindowed || (index >= first && index < last)) {
      if (typeof rendered?.scrollIntoView === "function") rendered.scrollIntoView({ block: "nearest" });
      return;
    }
    // Outside the window: scroll near it by the measured heights, render it, then scroll exactly.
    const { headings: headingIndexes, headingHeight: headingTall, option } = latest.current;
    const optionTop = top(index) + (headingIndexes.includes(index) ? headingTall : 0);
    if (optionTop < element.scrollTop) element.scrollTop = optionTop;
    else element.scrollTop = optionTop + option - element.clientHeight;
    pendingScroll.current = index;
    update();
  }, []);

  return useMemo(
    () => ({
      start,
      end,
      padTop: windowed ? slotTop(start) : 0,
      padBottom: windowed ? slotTop(count) - slotTop(end) : 0,
      windowed,
      scrollToIndex,
    }),
    [count, end, scrollToIndex, slotTop, start, windowed],
  );
}
