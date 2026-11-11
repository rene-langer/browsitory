import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";

/** How far (in CSS pixels) a press has to move before it becomes a drag; less is a click. */
export const DRAG_THRESHOLD = 4;
/** Within this distance of a scroll container's top or bottom edge, a drag scrolls it. */
const EDGE = 32;
/** Pixels scrolled per pointer move or animation frame near an edge. */
const SCROLL_STEP = 12;

/** The drop target under the pointer, and which half of it (for "before"/"after" a row). */
export interface PointerDragOver {
  key: string;
  element: HTMLElement;
  position: "before" | "after";
}

export interface PointerDragState<P> {
  payload: P;
  x: number;
  y: number;
  over: PointerDragOver | null;
}

export interface PointerDragOptions<P> {
  /**
   * Which drop targets this drag sees: elements carrying `dropTargetProps(scope, key)`. Targets
   * of another scope (another list's drag) are ignored, as if the pointer were over nothing.
   */
  scope: string;
  /** Released over `over` (`null`: over no target of this scope). */
  onDrop: (payload: P, over: PointerDragOver | null) => void;
  /** The press moved past the threshold: the drag has begun. */
  onStart?: (payload: P) => void;
  /** Escape, a cancelled pointer, or the window losing focus ended the drag without a drop. */
  onCancel?: (payload: P) => void;
  /** The target under the pointer changed (`null`: none). */
  onOver?: (payload: P, over: PointerDragOver | null) => void;
  /** The element to scroll while the pointer is near its top or bottom edge. */
  autoScroll?: () => HTMLElement | null;
}

/** The attributes that make an element a drop target of `scope`, named `key`. */
export function dropTargetProps(scope: string, key: string): { "data-drop-scope": string; "data-drop-key": string } {
  return { "data-drop-scope": scope, "data-drop-key": key };
}

function targetAt(scope: string, x: number, y: number): PointerDragOver | null {
  // jsdom has no `elementFromPoint`; tests stub it.
  if (typeof document.elementFromPoint !== "function") return null;
  const hit = document.elementFromPoint(x, y);
  const element = hit?.closest<HTMLElement>(`[data-drop-scope="${scope}"]`) ?? null;
  const key = element?.getAttribute("data-drop-key");
  if (element === null || key === null || key === undefined) return null;
  const rect = element.getBoundingClientRect();
  return { key, element, position: y < rect.top + rect.height / 2 ? "before" : "after" };
}

/** -1, 0 or 1: which way `container` should scroll for a pointer at `y`. */
function edgeDirection(container: HTMLElement, y: number): -1 | 0 | 1 {
  const rect = container.getBoundingClientRect();
  if (y < rect.top + EDGE) return -1;
  if (y > rect.bottom - EDGE) return 1;
  return 0;
}

/**
 * Dragging with pointer events (#744), not the HTML5 drag API: the desktop app keeps Tauri's
 * native file-drop handler on (#649), which stops HTML5 drags inside the page on Windows, and
 * pointer events behave the same in WebKitGTK, WebView2 and the VSCode webview.
 *
 * `startDrag` goes on the drag source's `onPointerDown`. A press becomes a drag once it moves
 * `DRAG_THRESHOLD` pixels; the listeners sit on `window`, so a source row that unmounts while
 * dragging (a windowed list scrolled away) doesn't end the drag. The target under the pointer is
 * the nearest element with `dropTargetProps(scope, …)`. Escape or `pointercancel` cancels; the
 * click that ends a drag is swallowed, so the source doesn't also act on it.
 */
export function usePointerDrag<P>(options: PointerDragOptions<P>): {
  drag: PointerDragState<P> | null;
  startDrag: (event: ReactPointerEvent, payload: P) => void;
} {
  const [drag, setDrag] = useState<PointerDragState<P> | null>(null);
  const optionsRef = useRef(options);
  useEffect(() => {
    optionsRef.current = options;
  });
  // Ends the press in progress (drag or not); `null` while there is none.
  const stopRef = useRef<(() => void) | null>(null);

  useEffect(() => () => stopRef.current?.(), []);

  const startDrag = useCallback((event: ReactPointerEvent, payload: P) => {
    if (event.button !== 0) return;
    stopRef.current?.();
    // Keeps the press from selecting text on its way.
    event.preventDefault();
    const source = event.currentTarget;
    const startX = event.clientX;
    const startY = event.clientY;
    let active = false;
    let over: PointerDragOver | null = null;
    let lastY = startY;
    let frame: number | null = null;

    const scrollStep = (): boolean => {
      const container = optionsRef.current.autoScroll?.() ?? null;
      if (container === null) return false;
      const direction = edgeDirection(container, lastY);
      if (direction !== 0) container.scrollTop += direction * SCROLL_STEP;
      return direction !== 0;
    };
    const keepScrolling = () => {
      frame = null;
      if (active && scrollStep()) frame = requestAnimationFrame(keepScrolling);
    };

    const update = (x: number, y: number) => {
      const next = targetAt(optionsRef.current.scope, x, y);
      if (next?.key !== over?.key) optionsRef.current.onOver?.(payload, next);
      over = next;
      setDrag({ payload, x, y, over });
    };

    const onMove = (move: PointerEvent) => {
      lastY = move.clientY;
      if (!active) {
        if (Math.hypot(move.clientX - startX, move.clientY - startY) < DRAG_THRESHOLD) return;
        active = true;
        optionsRef.current.onStart?.(payload);
      }
      update(move.clientX, move.clientY);
      if (scrollStep() && frame === null && typeof requestAnimationFrame === "function") {
        frame = requestAnimationFrame(keepScrolling);
      }
    };
    // A release after a drag clicks the nearest element holding both the press and the release:
    // the source or one of its ancestors. That click is swallowed; one elsewhere is a new click.
    // Only the first click after the release is looked at.
    const swallowClick = (click: MouseEvent) => {
      window.removeEventListener("click", swallowClick, true);
      if (!(click.target instanceof Node) || !click.target.contains(source)) return;
      click.stopPropagation();
      click.preventDefault();
    };
    const stop = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onCancel);
      if (frame !== null) cancelAnimationFrame(frame);
      frame = null;
      stopRef.current = null;
      if (active) setDrag(null);
    };
    const onUp = () => {
      const wasActive = active;
      stop();
      if (!wasActive) return;
      // The click this release produces belongs to the drag, not to whatever is under it.
      window.addEventListener("click", swallowClick, { capture: true });
      setTimeout(() => window.removeEventListener("click", swallowClick, true), 0);
      optionsRef.current.onDrop(payload, over);
    };
    const onCancel = () => {
      const wasActive = active;
      stop();
      if (wasActive) optionsRef.current.onCancel?.(payload);
    };
    const onKey = (key: KeyboardEvent) => {
      if (key.key !== "Escape") return;
      if (active) {
        key.preventDefault();
        key.stopPropagation();
      }
      onCancel();
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onCancel);
    stopRef.current = stop;
  }, []);

  return { drag, startDrag };
}
