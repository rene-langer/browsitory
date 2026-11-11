import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import styles from "./SplitView.module.css";

const ARROW_KEY_STEP = 16;
/** Matches `.right`'s `min-width` in SplitView.module.css. */
const RIGHT_MIN_WIDTH = 320;
/** Matches `.divider`'s `flex-basis`. */
const DIVIDER_WIDTH = 1;

/**
 * Lets a SplitView nested in another one's right pane report the width it needs
 * (its fixed left pane plus its own right pane's minimum), so the outer one
 * collapses its collapsible left pane instead of squeezing the nested one (#605).
 */
const ReportRequiredWidth = createContext<((width: number) => void) | null>(null);

/** How a collapsible left pane shows: at its width, hidden, or over the right pane (#635). */
export type SplitViewLeftState = "docked" | "collapsed" | "overlay";

/** A request from outside (a toolbar button, a shortcut) to toggle or show the left pane. */
export interface SplitViewRequest {
  kind: "toggle" | "show";
  /** A new value is a new request; the one the view mounts with is not. */
  nonce: number;
  /** Where focus returns when a drawer this opens closes; else where focus was. */
  returnFocusTo?: HTMLElement | null;
}

function loadWidth(
  storageKey: string | undefined,
  defaultWidth: number,
  min: number,
  max: number,
  collapsible: boolean,
): number {
  if (storageKey === undefined) return defaultWidth;
  const stored = localStorage.getItem(storageKey);
  if (stored === null) return defaultWidth;
  const parsed = Number.parseInt(stored, 10);
  if (Number.isNaN(parsed)) return defaultWidth;
  // A collapsed pane persists as 0, which is outside [min, max]; clamping it
  // would silently re-expand the pane on the next mount.
  if (collapsible && parsed === 0) return 0;
  return Math.min(max, Math.max(min, parsed));
}

/** The width a pane persisted as collapsed (0) re-expands to: the last width it had open. */
function loadExpandedWidth(
  storageKey: string | undefined,
  defaultWidth: number,
  min: number,
  max: number,
): number {
  if (storageKey === undefined) return defaultWidth;
  const parsed = Number.parseInt(localStorage.getItem(`${storageKey}-expanded`) ?? "", 10);
  if (Number.isNaN(parsed) || parsed <= 0) return defaultWidth;
  return Math.min(max, Math.max(min, parsed));
}

/**
 * A fixed-width left pane, a draggable divider and a right pane that takes the
 * rest. Resizing the container never changes the left pane's width: only the
 * right pane grows and shrinks (#605). When the container is too narrow for the
 * left pane plus the right pane's minimum (including a nested SplitView's fixed
 * pane), a collapsible left pane hides; its width is kept and comes back once
 * there is room again.
 *
 * While a collapsible pane is hidden for lack of room, an explicit expand (double-click or an
 * arrow key on the divider, or a `request`) shows it as an overlay drawer over the right pane
 * instead of squeezing it; Escape or a click beside it closes the drawer (#635).
 */
export function SplitView({
  left,
  right,
  storageKey,
  defaultWidth = 300,
  minWidth = 160,
  maxWidth = 480,
  collapsible = false,
  forceCollapsed = false,
  label = "Resize",
  leftId,
  request,
  onLeftStateChange,
  leftRegion,
  rightRegion,
}: {
  left: ReactNode;
  right: ReactNode;
  storageKey?: string;
  defaultWidth?: number;
  minWidth?: number;
  maxWidth?: number;
  collapsible?: boolean;
  /** Hides the left pane regardless of the stored width (adaptive narrow layouts); stored width is untouched. */
  forceCollapsed?: boolean;
  label?: string;
  /** The left pane's id, for a toggle's `aria-controls`. */
  leftId?: string;
  request?: SplitViewRequest;
  /** Reports how the left pane shows, for a toggle's `aria-expanded`. */
  onLeftStateChange?: (state: SplitViewLeftState) => void;
  /**
   * Names the left or right pane as a landmark region, which F6 moves between (`data-pane`,
   * #670). A nested SplitView names its own panes instead of the right one that holds it.
   */
  leftRegion?: string;
  rightRegion?: string;
}) {
  const [width, setWidth] = useState(() =>
    loadWidth(storageKey, defaultWidth, minWidth, maxWidth, collapsible),
  );
  const expandedWidth = useRef<number | null>(null);
  if (expandedWidth.current === null) {
    expandedWidth.current =
      width > 0 ? width : loadExpandedWidth(storageKey, defaultWidth, minWidth, maxWidth);
  }
  const dragState = useRef<{ startX: number; startWidth: number } | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const [containerWidth, setContainerWidth] = useState<number | null>(null);
  const [nestedRequiredWidth, setNestedRequiredWidth] = useState(0);
  const reportRequiredWidth = useContext(ReportRequiredWidth);
  const leftRef = useRef<HTMLDivElement>(null);
  // The drawer's width while it is open, else `null`: the pane's width, or the last one it had open.
  const [overlayWidth, setOverlayWidth] = useState<number | null>(null);
  const overlayOpen = overlayWidth !== null;
  // Where focus was when the drawer opened, to return it there when the drawer closes.
  const overlayOpener = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const root = rootRef.current;
    if (root === null || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1];
      if (entry !== undefined) setContainerWidth(entry.contentRect.width);
    });
    observer.observe(root);
    return () => observer.disconnect();
  }, []);

  function persist(next: number) {
    if (next > 0) expandedWidth.current = next;
    if (storageKey !== undefined) {
      localStorage.setItem(storageKey, String(next));
      if (next > 0) localStorage.setItem(`${storageKey}-expanded`, String(next));
    }
  }

  function clamp(next: number): number {
    if (collapsible && next < minWidth / 2) return 0;
    return Math.min(maxWidth, Math.max(minWidth, next));
  }

  function handlePointerDown(event: ReactPointerEvent) {
    // A pane hidden for lack of room can't be dragged open; double-click opens the drawer.
    if (hiddenForSpace) return;
    dragState.current = { startX: event.clientX, startWidth: width };
  }

  useEffect(() => {
    function handlePointerMove(event: PointerEvent) {
      if (dragState.current === null) return;
      const delta = event.clientX - dragState.current.startX;
      setWidth(clamp(dragState.current.startWidth + delta));
    }
    function handlePointerUp() {
      if (dragState.current === null) return;
      dragState.current = null;
      setWidth((current) => {
        persist(current);
        return current;
      });
    }
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collapsible, minWidth, maxWidth, storageKey]);

  function handleKeyDown(event: ReactKeyboardEvent) {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    if (hiddenForSpace) {
      if (event.key === "ArrowRight") openOverlay();
      else closeOverlay();
      return;
    }
    // From a collapsed (0) width, clamp() snaps anything below minWidth / 2
    // straight back to 0, so stepping alone can never re-expand the pane.
    // Treat either arrow key as an explicit expand instead.
    if (collapsible && width === 0) {
      expand();
      return;
    }
    const delta = event.key === "ArrowRight" ? ARROW_KEY_STEP : -ARROW_KEY_STEP;
    const next = clamp(width + delta);
    setWidth(next);
    persist(next);
  }

  function expand() {
    const next = expandedWidth.current ?? defaultWidth;
    setWidth(next);
    persist(next);
  }

  function collapse() {
    setWidth(0);
    persist(0);
  }

  function handleDoubleClick() {
    if (!collapsible) return;
    if (hiddenForSpace) {
      if (overlayOpen) closeOverlay();
      else openOverlay();
      return;
    }
    if (width === 0) {
      expand();
      return;
    }
    collapse();
  }

  const rightRequiredWidth = Math.max(RIGHT_MIN_WIDTH, nestedRequiredWidth);
  const tooNarrow =
    collapsible &&
    containerWidth !== null &&
    containerWidth < width + DIVIDER_WIDTH + rightRequiredWidth;
  const hiddenForSpace = forceCollapsed || tooNarrow;
  // Room came back: the pane docks again, and the drawer is not reopened by itself later.
  if (!hiddenForSpace && overlayOpen) setOverlayWidth(null);
  const overlay = hiddenForSpace && overlayOpen;
  const shownWidth = overlay ? overlayWidth : hiddenForSpace ? 0 : width;
  const leftState: SplitViewLeftState = overlay ? "overlay" : shownWidth === 0 ? "collapsed" : "docked";

  function openOverlay(returnFocusTo?: HTMLElement | null) {
    const active = document.activeElement;
    overlayOpener.current = returnFocusTo ?? (active instanceof HTMLElement && active !== document.body ? active : null);
    setOverlayWidth(width > 0 ? width : (expandedWidth.current ?? defaultWidth));
  }

  function closeOverlay() {
    setOverlayWidth(null);
    // Only when focus was in the drawer: a click elsewhere keeps its own focus.
    if (leftRef.current?.contains(document.activeElement) === true || document.activeElement === document.body) {
      overlayOpener.current?.focus({ preventScroll: true });
    }
  }

  // A toolbar toggle or a shortcut. Only a new nonce is a request. An effect, not render-time
  // state: answering it writes the stored width and moves focus.
  const handledRequest = useRef(request?.nonce);
  /* eslint-disable react-hooks/set-state-in-effect -- the request is an event from outside */
  useEffect(() => {
    if (request === undefined || request.nonce === handledRequest.current) return;
    handledRequest.current = request.nonce;
    if (!collapsible) return;
    if (hiddenForSpace) {
      if (request.kind === "show" || !overlayOpen) openOverlay(request.returnFocusTo);
      else closeOverlay();
    } else if (width === 0) expand();
    else if (request.kind === "toggle") collapse();
    // Only a new request triggers this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request]);
  /* eslint-enable react-hooks/set-state-in-effect */

  // Move focus into the drawer when it opens, so Tab and Escape work from there.
  useEffect(() => {
    if (overlay) leftRef.current?.focus({ preventScroll: true });
  }, [overlay]);

  useEffect(() => {
    onLeftStateChange?.(leftState);
  }, [onLeftStateChange, leftState]);
  // A collapsible pane gives way when space runs out, so only a fixed one counts.
  const requiredWidth = (collapsible ? 0 : shownWidth) + DIVIDER_WIDTH + rightRequiredWidth;

  useLayoutEffect(() => {
    reportRequiredWidth?.(requiredWidth);
  }, [reportRequiredWidth, requiredWidth]);
  useLayoutEffect(() => {
    if (reportRequiredWidth === null) return;
    return () => reportRequiredWidth(0);
  }, [reportRequiredWidth]);

  return (
    <div className={styles.splitView} ref={rootRef}>
      {/* `hidden` keeps a collapsed pane mounted (no remount cost when it is
          re-expanded) while removing it from the tab order and a11y tree. */}
      <div
        ref={leftRef}
        id={leftId}
        className={overlay ? `${styles.left} ${styles.overlay}` : styles.left}
        hidden={shownWidth === 0}
        role={leftRegion === undefined ? undefined : "region"}
        aria-label={leftRegion}
        data-pane={leftRegion === undefined ? undefined : ""}
        data-overlay={overlay ? "true" : undefined}
        tabIndex={overlay ? -1 : undefined}
        onKeyDown={
          overlay
            ? (event) => {
                if (event.key !== "Escape" || event.defaultPrevented) return;
                // A dialog opened from the drawer closes itself first.
                if (event.target instanceof Element && event.target.closest("dialog") !== null) return;
                event.stopPropagation();
                closeOverlay();
              }
            : undefined
        }
        style={{ width: `${shownWidth}px`, minWidth: `${Math.min(shownWidth, minWidth)}px` }}
      >
        {left}
      </div>
      <div
        className={styles.divider}
        role="separator"
        aria-orientation="vertical"
        aria-label={label}
        aria-valuenow={shownWidth}
        aria-valuemin={minWidth}
        aria-valuemax={maxWidth}
        tabIndex={0}
        onPointerDown={handlePointerDown}
        onKeyDown={handleKeyDown}
        onDoubleClick={handleDoubleClick}
      />
      <div
        className={styles.right}
        role={rightRegion === undefined ? undefined : "region"}
        aria-label={rightRegion}
        data-pane={rightRegion === undefined ? undefined : ""}
      >
        <div className={styles.rightInner}>
          <ReportRequiredWidth.Provider value={setNestedRequiredWidth}>{right}</ReportRequiredWidth.Provider>
        </div>
      </div>
      {/* A click beside the drawer closes it, as Escape does. Last, so the divider stays the
          left pane's next sibling. */}
      {overlay && <div className={styles.backdrop} data-split-backdrop aria-hidden="true" onClick={closeOverlay} />}
    </div>
  );
}
