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

function loadWidth(storageKey: string | undefined, defaultWidth: number, min: number, max: number): number {
  if (storageKey === undefined) return defaultWidth;
  const stored = localStorage.getItem(storageKey);
  if (stored === null) return defaultWidth;
  const parsed = Number.parseInt(stored, 10);
  if (Number.isNaN(parsed)) return defaultWidth;
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
}: {
  left: ReactNode;
  right: ReactNode;
  storageKey?: string;
  defaultWidth?: number;
  minWidth?: number;
  maxWidth?: number;
  collapsible?: boolean;
}) {
  const [width, setWidth] = useState(() => loadWidth(storageKey, defaultWidth, minWidth, maxWidth));
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
    if (event.key === "ArrowRight") {
      const next = clamp(width + ARROW_KEY_STEP);
      setWidth(next);
      persist(next);
    } else if (event.key === "ArrowLeft") {
      const next = clamp(width - ARROW_KEY_STEP);
      setWidth(next);
      persist(next);
    }
  }

  function handleDoubleClick() {
    if (!collapsible) return;
    const next = width === 0 ? defaultWidth : 0;
    setWidth(next);
    persist(next);
  }

  return (
    <div className={styles.splitView}>
      <div className={styles.left} style={{ width: `${width}px` }}>
        {left}
      </div>
      <div
        className={styles.divider}
        role="separator"
        aria-orientation="vertical"
        tabIndex={0}
        onPointerDown={handlePointerDown}
        onKeyDown={handleKeyDown}
        onDoubleClick={handleDoubleClick}
      />
      <div className={styles.right}>{right}</div>
    </div>
  );
}
