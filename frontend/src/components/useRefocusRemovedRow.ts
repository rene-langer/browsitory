import { useEffect, useRef, type RefObject } from "react";

/** The rows' "…" buttons, which carry the row's key as `data-menu-key`. */
const MENU_BUTTONS = "button[data-menu-key]";

/** Where a row's own menu or confirmation keeps the focus until it closes. */
const OVERLAY = 'dialog, [role="dialog"], [role="alertdialog"], [role="menu"]';

/**
 * Keeps the keyboard focus in a sidebar list when a row it acts on leaves it (#1141): its menu
 * or confirmation is open, or its delete runs, and the row goes (deleted, or a refresh removed
 * it). The focus would drop to the page; instead it goes to the "…" of the row now in that
 * place, else of the last row, else to the section's header. A focus that is somewhere else by
 * then stays there.
 *
 * `watching` holds the keys (the "…"'s `data-menu-key`) of the rows acted on now: an open menu
 * or confirmation, a running delete. `rows` selects the "…" buttons to move between, in their
 * order on screen. The section is the `<section>` around `list`, remembered for when the list
 * itself goes with its last row.
 */
export function useRefocusRemovedRow({
  list,
  watching,
  rows = MENU_BUTTONS,
}: {
  list: HTMLElement | null | RefObject<HTMLElement | null>;
  watching: readonly (string | null | undefined)[];
  rows?: string;
}): void {
  const section = useRef<Element | null>(null);
  // Each watched row's last place among the rows.
  const places = useRef(new Map<string, number>());
  // The place of a watched row that went, until the focus is moved there.
  const pending = useRef<number | null>(null);

  useEffect(() => {
    const element = list === null || list instanceof HTMLElement ? list : list.current;
    section.current = element?.closest("section") ?? section.current;
    if (section.current === null) return;
    const buttons = [...section.current.querySelectorAll<HTMLElement>(rows)];
    const keys = buttons.map((button) => button.getAttribute("data-menu-key"));
    // A row watched until now that went: the same render may have closed its menu.
    for (const [key, place] of places.current) if (!keys.includes(key)) pending.current = place;
    places.current = new Map();
    for (const key of watching) {
      const place = key == null ? -1 : keys.indexOf(key);
      if (key != null && place >= 0) places.current.set(key, place);
    }
    if (pending.current === null) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body && active.isConnected) {
      // The row's confirmation still has the focus: wait until it closes.
      if (active.closest(OVERLAY) === null) pending.current = null;
      return;
    }
    const place = Math.min(pending.current, buttons.length - 1);
    pending.current = null;
    (buttons[place] ?? section.current.querySelector<HTMLElement>("button"))?.focus({ preventScroll: true });
  });
}
