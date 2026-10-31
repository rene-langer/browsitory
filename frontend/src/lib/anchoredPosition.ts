/** Where a popover anchored to an element goes, in viewport (`position: fixed`) coordinates. */
export interface Placement {
  top: number;
  left: number;
  /** The room on the chosen side; the popover's height is capped to it. */
  maxHeight: number;
  side: "below" | "above";
}

/**
 * Places a popover of `size` next to `anchor` inside `viewport`: below it when it fits, otherwise
 * above when it fits there, otherwise on the side with more room (capping the height to that
 * room). Horizontally it starts at the anchor's left edge and shifts to stay `margin` inside the
 * viewport; a popover wider than the viewport is pinned to the left margin. Pure: no DOM access.
 */
export function placeAnchored(
  anchor: { top: number; bottom: number; left: number },
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  { gap = 4, margin = 4 }: { gap?: number; margin?: number } = {},
): Placement {
  const roomBelow = viewport.height - anchor.bottom - gap - margin;
  const roomAbove = anchor.top - gap - margin;
  const side =
    size.height <= roomBelow ? "below" : size.height <= roomAbove ? "above" : roomAbove > roomBelow ? "above" : "below";
  const maxHeight = Math.max(0, side === "below" ? roomBelow : roomAbove);
  const top = side === "below" ? anchor.bottom + gap : anchor.top - gap - Math.min(size.height, maxHeight);
  const left = Math.max(margin, Math.min(anchor.left, viewport.width - size.width - margin));
  return { top, left, maxHeight, side };
}
