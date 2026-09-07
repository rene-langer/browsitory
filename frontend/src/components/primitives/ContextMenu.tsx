import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import styles from "./ContextMenu.module.css";

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  destructive?: boolean;
  /** Rendered as the item button's `title` — e.g. explaining why it's disabled (issue #31/UX-003). */
  title?: string;
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  const menuRef = useRef<HTMLUListElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  // Indexes of every non-disabled item, in order — the only stops arrow-key navigation and
  // initial focus-on-open should land on (the WAI-ARIA APG menu pattern skips disabled items
  // rather than stranding focus on a control that can't activate).
  const enabledIndexes = items.reduce<number[]>((acc, item, index) => {
    if (item.disabled !== true) acc.push(index);
    return acc;
  }, []);

  // Roving tabindex: exactly one item is a tab stop at a time. Starts on the first enabled item
  // so the menu is immediately usable from the keyboard the moment it opens, matching the APG
  // menu pattern's initial-focus expectation.
  const [focusedIndex, setFocusedIndex] = useState<number>(() => enabledIndexes[0] ?? -1);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current !== null && !menuRef.current.contains(event.target as Node)) onClose();
    }
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [onClose]);

  // Move actual DOM focus onto the initial item once, when the menu first mounts — a menu opened
  // via keyboard (Enter/Space on a "…" affordance, or Shift+F10/Menu-key) should drop focus
  // straight into it rather than leaving it stranded on the trigger.
  useEffect(() => {
    if (focusedIndex >= 0) itemRefs.current[focusedIndex]?.focus();
    // Intentionally run once on mount only — subsequent focus moves are driven by arrow-key
    // navigation and hover below, not by re-running this effect.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function moveFocus(direction: 1 | -1) {
    if (enabledIndexes.length === 0) return;
    const currentPosition = enabledIndexes.indexOf(focusedIndex);
    const nextPosition =
      currentPosition === -1
        ? direction === 1
          ? 0
          : enabledIndexes.length - 1
        : (currentPosition + direction + enabledIndexes.length) % enabledIndexes.length;
    const nextIndex = enabledIndexes[nextPosition];
    setFocusedIndex(nextIndex);
    itemRefs.current[nextIndex]?.focus();
  }

  function focusEdge(position: "first" | "last") {
    if (enabledIndexes.length === 0) return;
    const nextIndex = position === "first" ? enabledIndexes[0] : enabledIndexes[enabledIndexes.length - 1];
    setFocusedIndex(nextIndex);
    itemRefs.current[nextIndex]?.focus();
  }

  function handleMenuKeyDown(event: ReactKeyboardEvent<HTMLUListElement>) {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveFocus(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveFocus(-1);
        break;
      case "Home":
        event.preventDefault();
        focusEdge("first");
        break;
      case "End":
        event.preventDefault();
        focusEdge("last");
        break;
      default:
        break;
    }
  }

  return (
    <ul
      ref={menuRef}
      role="menu"
      className={styles.menu}
      style={{ position: "fixed", top: y, left: x }}
      onMouseLeave={onClose}
      onKeyDown={handleMenuKeyDown}
    >
      {items.map((item, index) => (
        <li key={item.label} role="none">
          <button
            ref={(element) => {
              itemRefs.current[index] = element;
            }}
            type="button"
            role="menuitem"
            className={styles.item}
            disabled={item.disabled}
            title={item.title}
            tabIndex={index === focusedIndex ? 0 : -1}
            data-destructive={item.destructive === true ? "true" : undefined}
            onFocus={() => setFocusedIndex(index)}
            onClick={() => {
              if (item.disabled === true) return;
              item.onSelect();
              onClose();
            }}
          >
            {item.label}
          </button>
        </li>
      ))}
    </ul>
  );
}
