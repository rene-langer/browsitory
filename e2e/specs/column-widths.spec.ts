import { browser, expect } from "@wdio/globals";
import { waitForAppReady } from "../support/app";

// #605: the sidebar and history columns keep their width when the window resizes; only the diff
// column grows or shrinks. The widths are the defaults from RepoWorkspace.tsx (sidebar, 260px)
// and WorkspaceMain.tsx (history, 420px); this spec file starts with empty storage
// (wdio.conf.ts's `beforeSession`). Window widths stay within xvfb-run's default 1280px screen.

type ColumnWidths = { sidebar: number | null; history: number; diff: number };

// Each SplitView's left pane is the element right before its divider; the diff column is the
// one right after the history divider. A hidden (collapsed) pane reports `null`.
function columnWidths(): Promise<ColumnWidths> {
  return browser.execute(() => {
    const leftOf = (label: string) =>
      document.querySelector(`[role="separator"][aria-label="${label}"]`)?.previousElementSibling as
        | HTMLElement
        | null
        | undefined;
    const sidebar = leftOf("Sidebar width");
    const history = leftOf("History and diff width");
    const diff = document.querySelector('[role="separator"][aria-label="History and diff width"]')
      ?.nextElementSibling as HTMLElement | null | undefined;
    if (sidebar == null || history == null || diff == null) throw new Error("columns not found");
    return {
      sidebar: sidebar.hidden ? null : sidebar.getBoundingClientRect().width,
      history: history.getBoundingClientRect().width,
      diff: diff.getBoundingClientRect().width,
    };
  });
}

async function resizeWindowTo(width: number): Promise<void> {
  await browser.setWindowSize(width, 800);
  // Window decorations (if any) make the viewport a little narrower than the window.
  await browser.waitUntil(
    async () => Math.abs((await browser.execute(() => window.innerWidth)) - width) <= 40,
    { timeoutMsg: `expected the viewport to be about ${width}px wide` },
  );
}

describe("Browsitory column widths", () => {
  it("keeps the sidebar and history widths when the window resizes; only the diff changes", async () => {
    await waitForAppReady();

    await resizeWindowTo(1250);
    await browser.waitUntil(async () => (await columnWidths()).sidebar === 260, {
      timeoutMsg: "expected the sidebar at its default 260px",
    });
    const wide = await columnWidths();
    expect(wide.history).toBe(420);

    await resizeWindowTo(1080);
    await browser.waitUntil(async () => (await columnWidths()).diff < wide.diff, {
      timeoutMsg: "expected the diff column to shrink with the window",
    });
    const narrower = await columnWidths();
    expect(narrower.sidebar).toBe(260);
    expect(narrower.history).toBe(420);

    // Too narrow for 260 + 420 + the diff's 320px minimum: the sidebar collapses instead of
    // squeezing the history column, and comes back at its width once there is room.
    await resizeWindowTo(960);
    await browser.waitUntil(async () => (await columnWidths()).sidebar === null, {
      timeoutMsg: "expected the sidebar to collapse in a narrow window",
    });
    expect((await columnWidths()).history).toBe(420);

    await resizeWindowTo(1250);
    await browser.waitUntil(async () => (await columnWidths()).sidebar === 260, {
      timeoutMsg: "expected the sidebar back at 260px once the window is wide again",
    });
    expect((await columnWidths()).history).toBe(420);
  });
});
