import { fireEvent, screen } from "@testing-library/react";

/**
 * The commit button's menu (#1003): Commit and push, Amend last commit and Add co-author… live
 * there. Opens it and returns its items' names (none without a menu), then closes it again.
 */
export function commitMenuItems(): string[] {
  const chevron = screen.queryByRole("button", { name: "More commit options" });
  if (chevron === null) return [];
  fireEvent.click(chevron);
  const items = [...screen.queryAllByRole("menuitem"), ...screen.queryAllByRole("menuitemcheckbox")];
  const names = items.map((item) => item.getAttribute("aria-label") ?? (item.textContent ?? "").trim());
  fireEvent.click(chevron);
  return names;
}

/** Opens the commit button's menu and picks `name`. */
export function chooseCommitMenuItem(name: string): void {
  fireEvent.click(screen.getByRole("button", { name: "More commit options" }));
  fireEvent.click(screen.getByRole("menuitem", { name }));
}
