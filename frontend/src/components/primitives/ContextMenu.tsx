import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { Check, ChevronRight } from "lucide-react";
import { isMenuKey } from "../../lib/menuKeys";
import styles from "./ContextMenu.module.css";

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  destructive?: boolean;
  /**
   * Rendered as the item button's `title`. On a disabled item it is why (issue #31/UX-003), and
   * shows as visible text under the label, the item's description (#664).
   */
  title?: string;
  /** When set, the item is a checkbox (`menuitemcheckbox` + `aria-checked`) with a check mark. */
  checked?: boolean;
  /**
   * The item's group (#645): a separator is drawn between neighbours whose groups differ, so a
   * long menu reads as read-only, history-changing and destructive sections. Items without a
   * group form one group of their own.
   */
  group?: string;
  /**
   * A key that runs the same action outside the menu (#645), shown dimmed after the label, such
   * as `"s"` or `"Ctrl/Cmd+Z"`. Only a hint: it is not bound while the menu is open.
   */
  shortcut?: string;
  /** The `shortcut` in `aria-keyshortcuts` syntax (`"Control+Z Meta+Z"`), for assistive tech. */
  ariaKeyShortcuts?: string;
  /**
   * Items of a submenu this item opens (#992), by click, Enter, Space or ArrowRight; ArrowLeft or
   * Escape goes back, choosing one of them closes the whole menu. `onSelect` is not called.
   */
  submenu?: ContextMenuItem[];
}

/** Space kept between the menu and the window's edges. */
const MARGIN = 4;

/**
 * For each item, the reason it shares with every other item of its group (#992): all of them
 * disabled with the same `title`. The menu shows such a reason once, under the group's last item.
 */
function sharedReasons(items: readonly ContextMenuItem[]): (string | undefined)[] {
  const shared: (string | undefined)[] = items.map(() => undefined);
  let start = 0;
  for (let end = 1; end <= items.length; end++) {
    if (end < items.length && items[end].group === items[start].group) continue;
    const run = items.slice(start, end);
    const reason = run[0].title;
    const allShare = run.every((item) => item.disabled === true && item.title === reason);
    if (run.length > 1 && reason !== undefined && allShare) {
      for (let index = start; index < end; index++) shared[index] = reason;
    }
    start = end;
  }
  return shared;
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
  returnFocusTo,
  isTrigger,
  submenuOf,
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
  /**
   * Names this menu's own menu button when it may be neither `returnFocusTo` nor the element
   * focused at open, e.g. a row's "…" when the menu was opened from the row by a key or a
   * right-click. Called at press time with the pressed `aria-haspopup="menu"` element.
   */
  isTrigger?: (button: Element) => boolean;
  /**
   * Set on a submenu (#992): its item's label (the submenu's name) and closing the whole menu,
   * which Tab does. Keys stay in the submenu; Escape and ArrowLeft go back to the item.
   */
  submenuOf?: { label: string; closeAll: () => void };
}) {
  const isSubmenu = submenuOf !== undefined;
  const menuRef = useRef<HTMLUListElement>(null);
  const reasonIdPrefix = useId();
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);

  // The trigger: the element focused when the menu mounts, captured by the mount effect below
  // just before it moves focus into the menu — after the opening commit's unmount cleanups, so
  // a palette that closed in that commit has already put focus back where it was. Escape, Tab
  // and running an item focus it again *before* `onSelect`/`onClose`, so a dialog an item opens
  // records it (`useRestoreFocus`) and returns there too. An outside click leaves focus alone —
  // the user clicked somewhere else. `preventScroll`: a trigger scrolled out of view stays put.
  const capturedTrigger = useRef<HTMLElement | null | undefined>(undefined);
  const restoreFocus = () => {
    const trigger = returnFocusTo !== undefined ? returnFocusTo : capturedTrigger.current;
    if (trigger?.isConnected === true) trigger.focus({ preventScroll: true });
  };

  // Clamp to the viewport once the menu's real size is known, so a menu opened near the right or
  // bottom edge is never partly off-screen. Outside click and Escape close it; leaving with the
  // pointer does not.
  const [position, setPosition] = useState({ x, y });
  useLayoutEffect(() => {
    const menu = menuRef.current;
    if (menu === null) return;
    const rect = menu.getBoundingClientRect();
    const nextX = Math.max(MARGIN, Math.min(x, window.innerWidth - rect.width - MARGIN));
    const nextY = Math.max(MARGIN, Math.min(y, window.innerHeight - rect.height - MARGIN));
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
  // The open submenu (#992): its item and where it opens, beside that item.
  const [submenu, setSubmenu] = useState<{ index: number; x: number; y: number; button: HTMLButtonElement } | null>(
    null,
  );

  // The document listeners below read the latest `restoreFocus` and trigger without re-subscribing.
  const restoreFocusRef = useRef(restoreFocus);
  const returnFocusToRef = useRef(returnFocusTo);
  const isTriggerRef = useRef(isTrigger);
  useEffect(() => {
    restoreFocusRef.current = restoreFocus;
    returnFocusToRef.current = returnFocusTo;
    isTriggerRef.current = isTrigger;
  });

  useEffect(() => {
    // Escape with focus outside the menu (e.g. a menu opened by the pointer while focus stayed
    // elsewhere). Escape inside the menu is handled — and stopped — by `handleMenuKeyDown`.
    function handleKeyDown(event: KeyboardEvent) {
      // A submenu's Escape outside it is its parent menu's to handle.
      if (event.key !== "Escape" || isSubmenu) return;
      if (menuRef.current !== null && menuRef.current.contains(event.target as Node)) return;
      restoreFocusRef.current();
      onClose();
    }
    function handleClickOutside(event: MouseEvent) {
      const target = event.target as Node;
      if (menuRef.current === null || menuRef.current.contains(target)) return;
      if (event.button === 0 && isOwnMenuButton(target)) return;
      onClose();
    }
    // A primary press on this menu's own menu button is left to that button's click, which
    // toggles the menu closed; closing here would let the click reopen it at once. Its own button
    // is a menu button (`aria-haspopup="menu"`, so never a right-clicked row) that is the trigger
    // it returns focus to, the element focused when it opened, or the one `isTrigger` names.
    // Another menu's open button is not: a press there closes this menu.
    function isOwnMenuButton(target: Node): boolean {
      const element = target instanceof Element ? target : target.parentElement;
      const button = element?.closest('[aria-haspopup="menu"]');
      if (button === null || button === undefined) return false;
      return (
        button === returnFocusToRef.current ||
        button === capturedTrigger.current ||
        isTriggerRef.current?.(button) === true
      );
    }
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [onClose, isSubmenu]);

  // Move actual DOM focus onto the initial item once, when the menu first mounts — a menu opened
  // via keyboard (Enter/Space on a "…" affordance, or Shift+F10/Menu-key) should drop focus
  // straight into it rather than leaving it stranded on the trigger. A menu with no enabled
  // item focuses the menu itself, so Escape/Tab still work and assistive tech enters it.
  useEffect(() => {
    // Only on the first run: StrictMode's re-run would otherwise capture the menu's own item.
    if (capturedTrigger.current === undefined) {
      const active = document.activeElement;
      capturedTrigger.current = active instanceof HTMLElement && active !== document.body ? active : null;
    }
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
    if (item.submenu !== undefined) {
      openSubmenu(index);
      return;
    }
    restoreFocus();
    item.onSelect();
    onClose();
  }

  function openSubmenu(index: number) {
    const button = itemRefs.current[index];
    if (button == null) return;
    const rect = button.getBoundingClientRect();
    setSubmenu({ index, x: rect.right, y: rect.top, button });
  }

  // A submenu's choice closes this menu too, focus going back to its trigger first (#992).
  const closeAll = () => {
    restoreFocus();
    onClose();
  };

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
    // A submenu's keys never reach the menu it opened from.
    if (isSubmenu) event.stopPropagation();
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
        if (submenuOf !== undefined) {
          submenuOf.closeAll();
          return;
        }
        restoreFocus();
        onClose();
        return;
      case "ArrowRight": {
        // The item the key was pressed on (the roving index may not have caught up yet).
        const pressed = itemRefs.current.findIndex((button) => button === event.target);
        const index = pressed === -1 ? focusedIndex : pressed;
        if (items[index]?.submenu !== undefined) {
          event.preventDefault();
          activate(index);
        }
        return;
      }
      case "ArrowLeft":
        if (isSubmenu) {
          event.preventDefault();
          restoreFocus();
          onClose();
        }
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

  const shared = sharedReasons(items);
  const groupReasonId = (index: number) => {
    let last = index;
    while (last + 1 < items.length && items[last + 1].group === items[index].group) last++;
    return `${reasonIdPrefix}-reason-${last}`;
  };

  return (
    <ul
      ref={menuRef}
      role="menu"
      aria-label={submenuOf?.label}
      className={styles.menu}
      // Never taller than the window (#992): a long menu scrolls.
      style={{
        position: "fixed",
        top: position.y,
        left: position.x,
        maxHeight: `calc(100vh - ${2 * MARGIN}px)`,
        overflowY: "auto",
      }}
      tabIndex={enabledIndexes.length === 0 ? -1 : undefined}
      onKeyDown={handleMenuKeyDown}
      // A keyboard-generated contextmenu (the Menu key fires it on keyup, after focus moved in
      // here) must open neither the webview's native menu nor a row's menu underneath.
      onContextMenu={(event) => {
        event.preventDefault();
        event.stopPropagation();
      }}
    >
      {items.map((item, index) => {
        // A disabled item's `title` is why; it also shows under the label, as its description. A
        // reason the whole group shares shows once, under its last item (#992).
        const reason = item.disabled === true && item.title !== undefined ? item.title : undefined;
        const ownReasonId = `${reasonIdPrefix}-reason-${index}`;
        const reasonId = shared[index] === undefined ? ownReasonId : groupReasonId(index);
        const showsReason = reason !== undefined && reasonId === ownReasonId;
        const opensSubmenu = item.submenu !== undefined;
        const openSubmenuHere = submenu !== null && submenu.index === index ? submenu : null;
        const submenuOpen = openSubmenuHere !== null;
        return [
          index > 0 && items[index - 1].group !== item.group && (
            <li key={`separator:${item.label}`} role="separator" className={styles.separator} />
          ),
          <li key={item.label} role="none">
            <button
              ref={(element) => {
                itemRefs.current[index] = element;
              }}
              type="button"
              role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
              aria-checked={item.checked}
              aria-haspopup={opensSubmenu ? "menu" : undefined}
              aria-expanded={opensSubmenu ? submenuOpen : undefined}
              aria-keyshortcuts={item.ariaKeyShortcuts}
              className={styles.item}
              disabled={item.disabled}
              title={item.title}
              aria-describedby={reason === undefined ? undefined : reasonId}
              tabIndex={index === focusedIndex ? 0 : -1}
              data-destructive={item.destructive === true ? "true" : undefined}
              onFocus={() => setFocusedIndex(index)}
              onClick={() => activate(index)}
            >
              {item.checked !== undefined && (
                <Check size={12} aria-hidden="true" className={item.checked ? styles.check : styles.checkHidden} />
              )}
              {item.label}
              {item.shortcut !== undefined && (
                <kbd className={styles.shortcut} aria-hidden="true">
                  {item.shortcut}
                </kbd>
              )}
              {opensSubmenu && <ChevronRight size={12} aria-hidden="true" className={styles.submenuMark} />}
            </button>
            {showsReason && (
              <span id={reasonId} className={styles.reason}>
                {reason}
              </span>
            )}
            {openSubmenuHere !== null && item.submenu !== undefined && (
              <ContextMenu
                x={openSubmenuHere.x}
                y={openSubmenuHere.y}
                returnFocusTo={openSubmenuHere.button}
                submenuOf={{ label: item.label, closeAll }}
                onClose={() => setSubmenu(null)}
                items={item.submenu.map((sub) => ({
                  ...sub,
                  onSelect: () => {
                    restoreFocus();
                    sub.onSelect();
                    onClose();
                  },
                }))}
              />
            )}
          </li>,
        ];
      })}
    </ul>
  );
}
