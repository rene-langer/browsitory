import { $, browser } from "@wdio/globals";
import { withFailureDiagnostics } from "./diagnostics";

// Selectors for the sidebar that match exactly one element, as WebdriverIO's strict `$` requires
// (#394).

/**
 * The header button that opens and closes the sidebar section `title`. `AccordionSection` labels
 * it with the section's title; matching that label keeps the selector unique, since a section's
 * body has toggles of its own (branch folders, remotes).
 */
export function sidebarSectionToggle(title: string): string {
  // The shown workspace only: the last few tabs stay mounted, hidden, in the background (#642).
  return `[data-active-repo='true'] section[aria-label='${title}'] button[aria-expanded][aria-label='${title}']`;
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
//
// It first waits for the workspace's first load (#824, #825). Until then the Branches section is
// a short "Loading branches…" placeholder (#660) while the sections under it are already there;
// when the load lands, the branch tree replaces the placeholder and pushes every section under
// it down the sidebar. A header clicked in that moment could miss: WebKitWebDriver works out
// where to click, the header moves, and the click lands on whatever moved under it (the
// Worktrees section then stayed closed, #824). A header clicked just before it moved far below
// the visible part of the sidebar, taking its body with it, and windowed lists (the reflog,
// #775) render no rows that far out of view (#825). After the click it checks that the section
// opened, and clicks again if it didn't.
export async function expandSidebarSection(title: string): Promise<void> {
  await waitForSidebarLoaded();
  const toggle = sidebarSectionToggle(title);
  await withFailureDiagnostics(
    `expand-${title.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`,
    () =>
      browser.waitUntil(
        async () => {
          const trigger = await $(toggle);
          if (!(await trigger.isExisting())) return false;
          if ((await trigger.getAttribute("aria-expanded")) === "true") return true;
          await trigger.click();
          return (await trigger.getAttribute("aria-expanded")) === "true";
        },
        { interval: 250, timeoutMsg: `expected the ${title} section to open` },
      ),
    sidebarState,
  );
}

/**
 * Waits until the shown workspace's first load has landed: its Branches section replaces the
 * loading placeholder only then (`WorkspaceSidebar.tsx`, #660).
 */
export async function waitForSidebarLoaded(): Promise<void> {
  await withFailureDiagnostics(
    "sidebar-loaded",
    () => $(sidebarSectionToggle("Branches")).waitForExist({ timeoutMsg: "expected the workspace's first load to land" }),
    sidebarState,
  );
}

/**
 * The shown workspace's sidebar as the page has it, for failure messages: each section's title
 * and whether it is open, any loading placeholder, how far the sidebar is scrolled, and where the
 * Reflog section's list sits in it (rows render only near the visible part, #775).
 */
export async function sidebarState(): Promise<unknown> {
  return browser.execute(() => {
    const workspace = document.querySelector("[data-active-repo='true']");
    if (workspace === null) {
      return { activeWorkspace: false, workspaces: document.querySelectorAll("[data-active-repo]").length };
    }
    const sidebar = workspace.querySelector("aside[aria-label='Repository sections']");
    // The part of the sidebar that scrolls (`Sidebar.module.css`'s `.sections`).
    const scroller = Array.from(sidebar?.querySelectorAll<HTMLElement>("div") ?? []).find(
      (element) => getComputedStyle(element).overflowY === "auto",
    );
    const reflog = workspace.querySelector("section[aria-label='Reflog']");
    const reflogList = reflog?.querySelector("ul") ?? null;
    const select = reflog?.querySelector("select") ?? null;
    return {
      sections: Array.from(sidebar?.querySelectorAll("section[aria-label]") ?? []).map((section) => ({
        title: section.getAttribute("aria-label"),
        expanded: section.querySelector("button[aria-expanded]")?.getAttribute("aria-expanded") ?? null,
      })),
      placeholders: Array.from(sidebar?.querySelectorAll("[aria-busy='true']") ?? []).map((element) =>
        element.getAttribute("aria-label"),
      ),
      sidebarScroll:
        scroller === undefined
          ? null
          : { top: scroller.scrollTop, height: scroller.clientHeight, scrollHeight: scroller.scrollHeight },
      reflog:
        reflog === null
          ? null
          : {
              reference: select === null ? null : { value: select.value, disabled: select.disabled },
              header: reflog.querySelector("button[aria-expanded]")?.textContent ?? null,
              text: (reflog.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 300),
              rows: reflogList === null ? null : reflogList.querySelectorAll(":scope > li").length,
              listTop:
                reflogList === null || scroller === undefined
                  ? null
                  : reflogList.getBoundingClientRect().top - scroller.getBoundingClientRect().top,
            },
    };
  });
}
