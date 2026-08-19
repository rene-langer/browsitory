import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import styles from "./SplitView.module.css";

const ARROW_KEY_STEP = 16;

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

export function SplitView({
  left,
  right,
  storageKey,
  defaultWidth = 300,
  minWidth = 160,
  maxWidth = 480,
  collapsible = false,
  label = "Resize",
}: {
  left: ReactNode;
  right: ReactNode;
  storageKey?: string;
  defaultWidth?: number;
  minWidth?: number;
  maxWidth?: number;
  collapsible?: boolean;
  label?: string;
}) {
  const [width, setWidth] = useState(() =>
    loadWidth(storageKey, defaultWidth, minWidth, maxWidth, collapsible),
  );
  const dragState = useRef<{ startX: number; startWidth: number } | null>(null);

  function persist(next: number) {
    if (storageKey !== undefined) {
      localStorage.setItem(storageKey, String(next));
    }
  }

  function clamp(next: number): number {
    if (collapsible && next < minWidth / 2) return 0;
    return Math.min(maxWidth, Math.max(minWidth, next));
  }

  function handlePointerDown(event: ReactPointerEvent) {
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
  }, [collapsible, minWidth, maxWidth]);

  function handleKeyDown(event: ReactKeyboardEvent) {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    // From a collapsed (0) width, clamp() snaps anything below minWidth / 2
    // straight back to 0, so stepping alone can never re-expand the pane.
    // Treat either arrow key as an explicit expand instead.
    if (collapsible && width === 0) {
      setWidth(defaultWidth);
      persist(defaultWidth);
      return;
    }
    const delta = event.key === "ArrowRight" ? ARROW_KEY_STEP : -ARROW_KEY_STEP;
    const next = clamp(width + delta);
    setWidth(next);
    persist(next);
  }

  function handleDoubleClick() {
    if (!collapsible) return;
    const next = width === 0 ? defaultWidth : 0;
    setWidth(next);
    persist(next);
  }

  return (
    <div className={styles.splitView}>
      {/* `hidden` keeps a collapsed pane mounted (no remount cost when it is
          re-expanded) while removing it from the tab order and a11y tree. */}
      <div className={styles.left} hidden={width === 0} style={{ width: `${width}px` }}>
        {left}
      </div>
      <div
        className={styles.divider}
        role="separator"
        aria-orientation="vertical"
        aria-label={label}
        aria-valuenow={width}
        aria-valuemin={minWidth}
        aria-valuemax={maxWidth}
        tabIndex={0}
        onPointerDown={handlePointerDown}
        onKeyDown={handleKeyDown}
        onDoubleClick={handleDoubleClick}
      />
      <div className={styles.right}>
        <div className={styles.rightInner}>{right}</div>
      </div>
    </div>
  );
}
