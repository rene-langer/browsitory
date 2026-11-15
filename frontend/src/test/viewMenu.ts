import { fireEvent, screen } from "@testing-library/react";

/**
 * The View menus (#1007): the diff toolbar's "Diff view" and the graph search bar's "Graph view"
 * hold the rarely changed toggles as checkable items.
 */
export type ViewMenu = "Diff view" | "Graph view";

const toggleMenu = (menu: ViewMenu) => fireEvent.click(screen.getByRole("button", { name: menu }));

/** Opens `menu` and picks the toggle `name`. */
export function chooseViewOption(name: string, menu: ViewMenu = "Diff view"): void {
  toggleMenu(menu);
  fireEvent.click(screen.getByRole("menuitemcheckbox", { name }));
}

/** The toggle's `aria-checked` ("true", "false" or "mixed"), read with the menu opened and closed again. */
export function viewOptionState(name: string, menu: ViewMenu = "Diff view"): string | null {
  toggleMenu(menu);
  const state = screen.getByRole("menuitemcheckbox", { name }).getAttribute("aria-checked");
  toggleMenu(menu);
  return state;
}

/** The menu's toggles, in order. */
export function viewOptionNames(menu: ViewMenu = "Diff view"): string[] {
  toggleMenu(menu);
  const names = screen.queryAllByRole("menuitemcheckbox").map(visibleName);
  toggleMenu(menu);
  return names;
}

/** An item's text without what is hidden from assistive tech (the check mark, the Auto badge). */
function visibleName(item: HTMLElement): string {
  const copy = item.cloneNode(true) as HTMLElement;
  copy.querySelectorAll("[aria-hidden='true']").forEach((hidden) => hidden.remove());
  return (copy.textContent ?? "").trim();
}
