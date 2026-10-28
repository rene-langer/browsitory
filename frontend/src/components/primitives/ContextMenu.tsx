import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { Check } from "lucide-react";
import { isMenuKey } from "../../lib/menuKeys";
import styles from "./ContextMenu.module.css";

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  destructive?: boolean;
  /** Rendered as the item button's `title` — e.g. explaining why it's disabled (issue #31/UX-003). */
  title?: string;
  /** When set, the item is a checkbox (`menuitemcheckbox` + `aria-checked`) with a check mark. */
  checked?: boolean;
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
  returnFocusTo,
}: {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
  /**
   * Where focus goes back to when the menu closes from the keyboard or by running an item.
   * Defaults to whatever was focused when the menu first rendered (its trigger).
   */
  returnFocusTo?: HTMLElement | null;
}) {
  const menuRef = useRef<HTMLUListElement>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  // The trigger: the element focused when the menu first renders, captured before the mount
  // effect below moves focus into the menu. Escape, Tab and running an item focus it again
  // *before* `onSelect`/`onClose`, so a dialog an item opens records it (`useRestoreFocus`) and
  // returns there too. An outside click leaves focus alone — the user clicked somewhere else.
  const [capturedTrigger] = useState<HTMLElement | null>(() =>
    document.activeElement instanceof HTMLElement && document.activeElement !== document.body
      ? document.activeElement
      : null,
  );
  const trigger = returnFocusTo !== undefined ? returnFocusTo : capturedTrigger;
  const restoreFocus = () => {
    if (trigger?.isConnected === true) trigger.focus();
  };

  // Clamp to the viewport once the menu's real size is known, so a menu opened near the right or
  // bottom edge is never partly off-screen. Outside click and Escape close it; leaving with the
  // pointer does not.
  const [position, setPosition] = useState({ x, y });
  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (menu === null) return;
    const rect = menu.getBoundingClientRect();
    const margin = 4;
    const nextX = Math.max(margin, Math.min(x, window.innerWidth - rect.width - margin));
    const nextY = Math.max(margin, Math.min(y, window.innerHeight - rect.height - margin));
    setPosition((current) => (current.x === nextX && current.y === nextY ? current : { x: nextX, y: nextY }));
  }, [x, y, items.length]);

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
    // Escape with focus outside the menu (e.g. a menu opened by the pointer while focus stayed
    // elsewhere). Escape inside the menu is handled — and stopped — by `handleMenuKeyDown`.
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      if (menuRef.current !== null && menuRef.current.contains(event.target as Node)) return;
      if (trigger?.isConnected === true) trigger.focus();
      onClose();
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
  }, [onClose, trigger]);

  // Move actual DOM focus onto the initial item once, when the menu first mounts — a menu opened
  // via keyboard (Enter/Space on a "…" affordance, or Shift+F10/Menu-key) should drop focus
  // straight into it rather than leaving it stranded on the trigger. A menu with no enabled
  // item focuses the menu itself, so Escape/Tab still work and assistive tech enters it.
  useEffect(() => {
    if (focusedIndex >= 0) itemRefs.current[focusedIndex]?.focus();
    else menuRef.current?.focus();
    // Intentionally run once on mount only — subsequent focus moves are driven by arrow keys
    // and typeahead, not by re-running this effect.
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

  function focusItem(index: number) {
    setFocusedIndex(index);
    itemRefs.current[index]?.focus();
  }

  function activate(index: number) {
    const item = items[index];
    if (item === undefined || item.disabled === true) return;
    restoreFocus();
    item.onSelect();
    onClose();
  }

  // Single-character typeahead: the next enabled item (after the focused one, wrapping) whose
  // label starts with `key`, case-insensitively. No multi-character buffer.
  function typeahead(key: string): boolean {
    if (enabledIndexes.length === 0) return false;
    const needle = key.toLowerCase();
    const start = enabledIndexes.indexOf(focusedIndex);
    for (let step = 1; step <= enabledIndexes.length; step += 1) {
      const index = enabledIndexes[(start + step + enabledIndexes.length) % enabledIndexes.length];
      if (items[index].label.toLowerCase().startsWith(needle)) {
        focusItem(index);
        return true;
      }
    }
    return false;
  }

  function handleMenuKeyDown(event: ReactKeyboardEvent<HTMLUListElement>) {
    if (isMenuKey(event)) {
      // Already open: the key must not reach a row or list underneath and open another menu.
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    switch (event.key) {
      case "Escape":
        // Stopped so ancestor handlers (the graph's "Escape collapses a multi-selection") don't
        // also act on it.
        event.preventDefault();
        event.stopPropagation();
        restoreFocus();
        onClose();
        return;
      case "Tab":
        // APG: Tab closes the menu and moves on. Focus goes back to the trigger first, and the
        // browser's default Tab action then moves on from there (not prevented).
        restoreFocus();
        onClose();
        return;
      case "Enter":
      case " ":
        // Handled here rather than by the native button click, which would otherwise fire as well.
        event.preventDefault();
        activate(focusedIndex);
        return;
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
        if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey && typeahead(event.key)) {
          event.preventDefault();
        }
        break;
    }
  }

  return (
    <ul
      ref={menuRef}
      role="menu"
      className={styles.menu}
      style={{ position: "fixed", top: position.y, left: position.x }}
      tabIndex={enabledIndexes.length === 0 ? -1 : undefined}
      onKeyDown={handleMenuKeyDown}
      // A keyboard-generated contextmenu (the Menu key fires it on keyup, after focus moved in
      // here) must open neither the webview's native menu nor a row's menu underneath.
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      {items.map((item, index) => (
        <li key={item.label} role="none">
          <button
            ref={(element) => {
              itemRefs.current[index] = element;
            }}
            type="button"
            role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-checked={item.checked}
            className={styles.item}
            disabled={item.disabled}
            title={item.title}
            tabIndex={index === focusedIndex ? 0 : -1}
            data-destructive={item.destructive === true ? "true" : undefined}
            onFocus={() => setFocusedIndex(index)}
            onClick={() => activate(index)}
          >
            {item.checked !== undefined && (
              <Check size={12} aria-hidden="true" className={item.checked ? styles.check : styles.checkHidden} />
            )}
            {item.label}
          </button>
        </li>
      ))}
    </ul>
  );
}
