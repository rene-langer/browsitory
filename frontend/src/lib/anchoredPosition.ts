/** Where a popover anchored to an element goes, in viewport (`position: fixed`) coordinates. */
export interface Placement {
  top: number;
  left: number;
  /** The room on the chosen side; the popover's height is capped to it. */
  maxHeight: number;
  side: "below" | "above";
}

/**
 * Places a popover of `size` next to `anchor` inside `viewport`: on the `prefer`red side while it
 * still fits there (so it doesn't jump when its content shrinks), otherwise below it when it
 * fits, otherwise above when it fits there, otherwise on the side with more room (capping the
 * height to that room). When neither side has room for even `minHeight`, it takes the whole
 * window height and may cover the anchor. `top` always stays inside the viewport. Horizontally it
 * starts at the anchor's left edge and shifts to stay `margin` inside the viewport; a popover
 * wider than the viewport is pinned to the left margin. Pure: no DOM access.
 */
export function placeAnchored(
  anchor: { top: number; bottom: number; left: number },
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  {
    gap = 4,
    margin = 4,
    minHeight = 48,
    prefer,
  }: { gap?: number; margin?: number; minHeight?: number; prefer?: Placement["side"] } = {},
): Placement {
  const roomBelow = viewport.height - anchor.bottom - gap - margin;
  const roomAbove = anchor.top - gap - margin;
  const fits = (side: Placement["side"]) => size.height <= (side === "below" ? roomBelow : roomAbove);
  const side =
    prefer !== undefined && fits(prefer)
      ? prefer
      : fits("below")
        ? "below"
        : fits("above")
          ? "above"
          : roomAbove > roomBelow
            ? "above"
            : "below";
  const left = Math.max(margin, Math.min(anchor.left, viewport.width - size.width - margin));
  const room = side === "below" ? roomBelow : roomAbove;
  const whole = Math.max(0, viewport.height - 2 * margin);
  if (room < Math.min(size.height, minHeight)) {
    return { top: margin, left, maxHeight: whole, side };
  }
  const maxHeight = Math.max(0, room);
  const height = Math.min(size.height, maxHeight);
  const wanted = side === "below" ? anchor.bottom + gap : anchor.top - gap - height;
  const top = Math.max(margin, Math.min(wanted, viewport.height - margin - height));
  return { top, left, maxHeight, side };
}
