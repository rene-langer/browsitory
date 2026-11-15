import { fireEvent, screen } from "@testing-library/react";

/**
 * The View menus (#1007): the diff toolbar's "View diff options" and the graph search bar's
 * "View graph options" hold the rarely changed toggles as checkable items. A toggle keeps its
 * menu open, so each helper closes it again.
 */
export type ViewMenu = "View diff options" | "View graph options";

const toggleMenu = (menu: ViewMenu) => fireEvent.click(screen.getByRole("button", { name: menu }));

/** Opens `menu`, picks the toggle `name`, and closes the menu again. */
export function chooseViewOption(name: string, menu: ViewMenu = "View diff options"): void {
  toggleMenu(menu);
  fireEvent.click(screen.getByRole("menuitemcheckbox", { name }));
  if (screen.queryByRole("menu") !== null) toggleMenu(menu);
}

/** The toggle's `aria-checked` ("true", "false" or "mixed"), read with the menu opened and closed again. */
export function viewOptionState(name: string, menu: ViewMenu = "View diff options"): string | null {
  toggleMenu(menu);
  const state = screen.getByRole("menuitemcheckbox", { name }).getAttribute("aria-checked");
  toggleMenu(menu);
  return state;
}

/** The menu's toggles, in order. */
export function viewOptionNames(menu: ViewMenu = "View diff options"): string[] {
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
