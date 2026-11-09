/** A range of file lines, 1-based and inclusive. */
export interface LineSpan {
  start: number;
  end: number;
}

/**
 * The file lines the browser's text selection covers inside `container` (#447): the
 * `data-line-number` of every element it touches, as one span from the lowest to the highest.
 * A line the selection only reaches the very start of (as after a triple-click) or only leaves
 * at its very end doesn't count. `null` without a selection there, or when it covers no numbered
 * line (only removed lines of a diff, say).
 */
export function selectedLineSpan(container: HTMLElement | null): LineSpan | null {
  const selection = typeof window.getSelection === "function" ? window.getSelection() : null;
  if (container === null || selection === null || selection.rangeCount === 0 || selection.isCollapsed) return null;
  let start = Infinity;
  let end = -Infinity;
  for (let i = 0; i < selection.rangeCount; i++) {
    const range = selection.getRangeAt(i);
    for (const element of container.querySelectorAll<HTMLElement>("[data-line-number]")) {
      if (!range.intersectsNode(element) || touchesOnlyAnEdge(range, element)) continue;
      const line = Number(element.dataset.lineNumber);
      if (!Number.isInteger(line) || line < 1) continue;
      start = Math.min(start, line);
      end = Math.max(end, line);
    }
  }
  return start <= end ? { start, end } : null;
}

/** The range ends at the very start of `element`, or starts at its very end. */
function touchesOnlyAnEdge(range: Range, element: HTMLElement): boolean {
  const probe = document.createRange();
  if (element.contains(range.endContainer) && !element.contains(range.startContainer)) {
    probe.setStart(element, 0);
    probe.setEnd(range.endContainer, range.endOffset);
    return probe.toString() === "";
  }
  if (element.contains(range.startContainer) && !element.contains(range.endContainer)) {
    probe.setStart(range.startContainer, range.startOffset);
    probe.setEnd(element, element.childNodes.length);
    return probe.toString() === "";
  }
  return false;
}

/** "lines 3–5" or "line 3". */
export function lineRangeLabel(start: number, end: number): string {
  return start === end ? `line ${start}` : `lines ${start}–${end}`;
}
