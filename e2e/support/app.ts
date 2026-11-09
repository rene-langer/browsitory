import { $, browser } from "@wdio/globals";
import { sidebarSectionToggle } from "./sidebar";

/**
 * Waits until the active repository has loaded. App.tsx's Ctrl/Cmd+K handler no-ops until
 * `state.repoPath` is set (see its early return), and `wdio.conf.ts`'s `beforeTest` reloads the
 * page before every `it`, so the repo's async auto-open (and this guard) can still be in flight
 * right when a test starts — sending Ctrl+K too early is silently swallowed. Sidebar sections
 * only render once the repo has loaded, so waiting for one to exist is this suite's established
 * way to gate on "app finished loading" (see how `pull-requests.spec.ts`/
 * `remote-management.spec.ts`/etc. all wait on a section's trigger or form before their first
 * interaction).
 */
export async function waitForAppReady(): Promise<void> {
  await $(sidebarSectionToggle("Branches")).waitForExist();
}

/**
 * Sends Ctrl/Cmd+K until the command palette is showing and returns its input (whose label is
 * the picker's name once a picker is open). Doesn't wait for the app itself: call
 * `waitForAppReady` first.
 *
 * Even after `waitForAppReady`, a keypress sent right after `beforeTest`'s reload was
 * occasionally lost (1 failure in 3 full-suite runs; the palette never opened, no error) — most
 * likely because the Branches section can already be in the DOM a beat before the workspace's
 * window keydown listener (attached in a passive effect) is live, or before the startup tab
 * reconciliation has settled. Re-pressing only while the input still isn't displayed keeps the
 * toggle from closing a palette that opened late.
 */
export async function openPalette(): Promise<WebdriverIO.Element> {
  const input = await $('input[aria-label="Command palette"]');
  await browser.waitUntil(
    async () => {
      if (await input.isDisplayed()) return true;
      await browser.keys(["Control", "k"]);
      return input.waitForDisplayed({ timeout: 1000 }).then(
        () => true,
        () => false,
      );
    },
    { interval: 0, timeoutMsg: "expected Ctrl/Cmd+K to open the command palette" },
  );
  return input;
}
