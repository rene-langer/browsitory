/**
 * F6 / Shift+F6 between the workspace's panes (#670): the toolbar, the sidebar, the history and
 * the diff, each marked with `data-pane` (and, but for the toolbar, a labelled `region`). A pane
 * gets back the element that last had the focus in it; the first time, its list (the commit
 * graph, the file list) or else its first focusable element.
 */

const PANE_SELECTOR = "[data-pane]";
const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]';
const LIST_ROLES = '[role="listbox"], [role="tree"], [role="grid"], [role="tablist"]';

/** Not inside a `hidden` element (a collapsed sidebar) and not taken out of the tab order. */
function usable(element: HTMLElement, root: Element): boolean {
  if (element.tabIndex < 0 || element.matches(":disabled") || element.closest("[hidden], [inert]") !== null) return false;
  return root.contains(element);
}

/** The panes shown now, in document order. */
export function visiblePanes(root: ParentNode): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(PANE_SELECTOR)).filter(
    (pane) => pane.closest("[hidden], [inert]") === null,
  );
}

/** The pane holding `element`, if any. */
export function paneOf(element: Element | null): HTMLElement | null {
  return element?.closest<HTMLElement>(PANE_SELECTOR) ?? null;
}

/** Where the focus goes in `pane`: the last focused element in it, its list, or its first stop. */
export function paneFocusTarget(pane: HTMLElement, lastFocused: Element | undefined): HTMLElement | null {
  if (lastFocused instanceof HTMLElement && lastFocused.isConnected && usable(lastFocused, pane)) return lastFocused;
  const list = Array.from(pane.querySelectorAll<HTMLElement>(LIST_ROLES)).find((element) => usable(element, pane));
  if (list !== undefined) return list;
  return Array.from(pane.querySelectorAll<HTMLElement>(FOCUSABLE)).find((element) => usable(element, pane)) ?? null;
}

/**
 * Moves the focus to the next (`1`) or previous (`-1`) pane after the one holding
 * `document.activeElement`, wrapping around, skipping panes with nothing to focus. Returns
 * whether it moved.
 */
export function focusAdjacentPane(root: ParentNode, direction: 1 | -1, lastFocused: WeakMap<Element, Element>): boolean {
  const panes = visiblePanes(root);
  if (panes.length === 0) return false;
  const current = paneOf(document.activeElement);
  const index = current === null ? -1 : panes.indexOf(current);
  for (let step = 1; step <= panes.length; step++) {
    // From outside every pane, F6 starts at the first and Shift+F6 at the last.
    const next =
      index === -1
        ? panes[direction === 1 ? step - 1 : panes.length - step]
        : panes[(index + direction * step + panes.length * step) % panes.length];
    if (next === current) continue;
    const target = paneFocusTarget(next, lastFocused.get(next));
    if (target !== null) {
      target.focus();
      return true;
    }
  }
  return false;
}
