import { $, browser, expect } from "@wdio/globals";
import { waitForAppReady } from "../support/app";

// #635: below 900px the sidebar hides, and the repository toolbar's sidebar button opens it as a
// drawer over the graph; Escape closes it. #638: the toolbar's branch chip opens "Switch branch…".
// The window goes back to the default size at the end: every spec file shares this session.

const DEFAULT_SIZE = { width: 1200, height: 800 };

async function resizeWindowTo(width: number): Promise<void> {
  await browser.setWindowSize(width, DEFAULT_SIZE.height);
  // Window decorations (if any) make the viewport a little narrower than the window.
  await browser.waitUntil(
    async () => Math.abs((await browser.execute(() => window.innerWidth)) - width) <= 40,
    { timeoutMsg: `expected the viewport to be about ${width}px wide` },
  );
}

/** The sidebar pane: the element right before its divider. */
function sidebarPane(): Promise<{ hidden: boolean; overlay: boolean; width: number }> {
  return browser.execute(() => {
    const pane = document.querySelector('[role="separator"][aria-label="Sidebar width"]')
      ?.previousElementSibling as HTMLElement | null | undefined;
    if (pane == null) throw new Error("sidebar pane not found");
    return { hidden: pane.hidden === true, overlay: pane.dataset.overlay === "true", width: pane.getBoundingClientRect().width };
  });
}

describe("Browsitory in a narrow window", () => {
  after(async () => {
    await browser.setWindowSize(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
  });

  it("opens the sidebar as a drawer from the toolbar and closes it with Escape", async () => {
    await waitForAppReady();
    await resizeWindowTo(820);
    await browser.waitUntil(async () => (await sidebarPane()).hidden, {
      timeoutMsg: "expected the sidebar to hide in a narrow window",
    });

    const toggle = await $("div[aria-label='Repository'] button[aria-label='Sidebar']");
    await toggle.click();
    await browser.waitUntil(async () => (await sidebarPane()).overlay, {
      timeoutMsg: "expected the sidebar to open as a drawer",
    });
    const open = await sidebarPane();
    expect(open.hidden).toBe(false);
    expect(open.width).toBeGreaterThan(100);
    await expect(toggle).toHaveAttribute("aria-expanded", "true");

    await browser.keys(["Escape"]);
    await browser.waitUntil(async () => (await sidebarPane()).hidden, {
      timeoutMsg: "expected Escape to close the drawer",
    });
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
  });

  it("opens the branch switcher from the toolbar's current branch", async () => {
    await waitForAppReady();
    await (await $("div[aria-label='Repository'] button[aria-label^='Current branch']")).click();
    const input = await $('input[aria-label="Switch branch"]');
    await input.waitForDisplayed();
    await browser.keys(["Escape"]);
    await input.waitForDisplayed({ reverse: true });
  });
});
