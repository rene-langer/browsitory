import { $ } from "@wdio/globals";

// Every sidebar section (Branches, Worktrees, Submodules, Reflog, Remotes, Tags, Pull Requests)
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
  const trigger = await $(`section[aria-label='${title}'] button[aria-expanded]`);
  await trigger.waitForExist({ timeout: 10000 });
  if ((await trigger.getAttribute("aria-expanded")) !== "true") {
    await trigger.click();
  }
}
