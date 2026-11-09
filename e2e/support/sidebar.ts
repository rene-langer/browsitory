import { $ } from "@wdio/globals";

// Selectors for the sidebar that match exactly one element, as WebdriverIO's strict `$` requires
// (#394).

/**
 * The header button that opens and closes the sidebar section `title`. `AccordionSection` labels
 * it with the section's title; matching that label keeps the selector unique, since a section's
 * body has toggles of its own (branch folders, remotes).
 */
export function sidebarSectionToggle(title: string): string {
  return `section[aria-label='${title}'] button[aria-expanded][aria-label='${title}']`;
}

/**
 * The row (a `role="button"` `<li>`, see `ListRow.tsx`) of the local branch `name` in the
 * Branches section, found by its "Actions for <name>" button. A `li*=name` text selector also
 * matches the folder `<li>`s around the row, and a row in a folder shows only the last part of
 * the name.
 */
export function sidebarBranchRow(name: string): string {
  return `//section[@aria-label='Branches']//li[@role='button'][.//button[@aria-label='Actions for ${name}']]`;
}

/**
 * The folder header of the remote `name` in the Branches section: the button beside its
 * "Actions for remote <name>" button. A `button=name` text selector can match other buttons
 * with the same text (for example a local branch folder of that name).
 */
export function sidebarRemoteFolder(name: string): string {
  return `//div[button[@aria-label='Actions for remote ${name}']]/button[normalize-space(.)='${name}']`;
}

/** The Remote name field of the Branches section's Add remote form, found by its label. */
export const ADD_REMOTE_NAME_INPUT = "//form[@aria-label='Add remote']//label[contains(., 'Remote name')]//input";

// Every sidebar section (Branches, Worktrees, Submodules, Reflog, Remotes, Tags, Pull requests)
// is wrapped in `AccordionSection` (see `frontend/src/components/primitives/AccordionSection.tsx`)
// and defaults CLOSED — its body isn't even rendered into the DOM until the header button is
// clicked open (conditional render, not CSS hiding), so any spec that reaches into a section's
// contents has to expand it first.
//
// `AccordionSection` also persists open/closed state to `localStorage` per `storageKey`, and
// this suite runs every spec file against the same long-lived WebDriver session (`wdio.conf.ts`
// only reloads the page between tests, it never clears storage or restarts the app) — so a
// section opened by an earlier spec stays open for every later one. This helper is therefore
// idempotent: it only clicks the header when the section is actually closed, so it's safe to
// call unconditionally regardless of what earlier specs left behind.
export async function expandSidebarSection(title: string): Promise<void> {
  const trigger = await $(sidebarSectionToggle(title));
  await trigger.waitForExist();
  if ((await trigger.getAttribute("aria-expanded")) !== "true") {
    await trigger.click();
  }
}
