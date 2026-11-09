import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { waitUntilInPage } from "../support/dom";
import { expandSidebarSection, sidebarBranchRow, sidebarRemoteFolder, sidebarSectionToggle } from "../support/sidebar";
import { SLOW_UI_TIMEOUT } from "../support/timeouts";
import { DEMO_REPO_PATH, startConflict } from "./demoRepo";

// Captures the documentation screenshots into docs/guide/assets/ (see ./README.md). Run it with
// `pnpm screenshots`, never as part of the E2E suite: it asserts nothing beyond "the screen it
// shows has loaded".
//
// The tests run in order and build on each other (sidebar sections opened by one stay open), so
// the config stops at the first failure (`bail`). Clicks go through `browser.execute`, like the
// rest of the E2E suite.

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ASSETS = path.resolve(__dirname, "../../docs/guide/assets");
const WINDOW = { width: 1280, height: 800 };

async function click(selector: string): Promise<void> {
  const element = await $(selector);
  await element.waitForExist();
  await browser.execute((el) => (el as HTMLElement).click(), element);
}

async function collapseSidebarSection(title: string): Promise<void> {
  const trigger = await $(sidebarSectionToggle(title));
  await trigger.waitForExist();
  if ((await trigger.getAttribute("aria-expanded")) === "true") {
    await browser.execute((el) => (el as HTMLElement).click(), trigger);
  }
}

/** Waits for rendering to settle (fonts, layout, a pending fetch), then saves the shot. */
async function shoot(name: string): Promise<void> {
  await browser.execute(() => (document.activeElement as HTMLElement | null)?.blur());
  await browser.pause(600);
  await browser.saveScreenshot(path.join(ASSETS, `${name}.png`));
}

async function setTheme(theme: "light" | "dark"): Promise<void> {
  const toggle = await $(`button[aria-label="Switch to ${theme} theme"]`);
  if (await toggle.isExisting()) await browser.execute((el) => (el as HTMLElement).click(), toggle);
  await browser.waitUntil(async () => (await browser.execute(() => document.documentElement.dataset.theme)) === theme, {
    timeout: 5000,
    timeoutMsg: `the app did not switch to the ${theme} theme`,
  });
}

describe("Documentation screenshots", () => {
  before(async () => {
    fs.mkdirSync(ASSETS, { recursive: true });
    await browser.setWindowSize(WINDOW.width, WINDOW.height);
    // Start from default sidebar state; the E2E build has just opened its own fixture repo.
    await browser.execute(() => localStorage.clear());
    await browser.refresh();
    await $(sidebarSectionToggle("Branches")).waitForExist({ timeout: SLOW_UI_TIMEOUT });
    await setTheme("light");

    // Open the demo from the picker's recent list, then close the E2E fixture's tab.
    await click('button[aria-label="Open another repository"]');
    await click(`li*=${DEMO_REPO_PATH}`);
    await browser.waitUntil(async () => (await (await $$('[role="tab"]')).length) === 2, {
      timeoutMsg: "the demo repository did not open in a second tab",
    });
    const fixtureTabClose = await $(
      `//button[@role="tab" and not(@title="${DEMO_REPO_PATH}")]/following-sibling::button[1]`,
    );
    await browser.execute((el) => (el as HTMLElement).click(), fixtureTabClose);
    await browser.waitUntil(async () => (await (await $$('[role="tab"]')).length) === 1, {
      timeoutMsg: "the E2E fixture tab did not close",
    });
    await $("li*=Add setup steps to the README").waitForExist();
  });

  it("overview", async () => {
    await $("li*=Uncommitted changes").waitForExist();
    await shoot("overview");
  });

  it("staging", async () => {
    await click("span*=src/app.js (Modified)");
    await waitUntilInPage(".diff-line-add");
    const message = await $("textarea[placeholder='Commit message']");
    await browser.execute(
      (el) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
        setter?.call(el, "Add zoom controls to the toolbar");
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      message,
    );
    await shoot("staging");
  });

  it("commit-diff", async () => {
    await click("li*=Limit the tile cache to 200 entries");
    await waitUntilInPage(".diff-line-add");
    await shoot("commit-diff");
  });

  it("branches", async () => {
    await expandSidebarSection("Branches");
    await $(sidebarBranchRow("feature/offline-tiles")).waitForExist();
    const origin = await $(sidebarRemoteFolder("origin"));
    await origin.waitForExist();
    if ((await origin.getAttribute("aria-expanded")) !== "true") {
      await browser.execute((el) => (el as HTMLElement).click(), origin);
      await browser.waitUntil(async () => (await origin.getAttribute("aria-expanded")) === "true", {
        timeout: 5000,
        timeoutMsg: "the origin folder did not expand",
      });
    }
    await shoot("branches");
  });

  it("stashes", async () => {
    await collapseSidebarSection("Branches");
    await expandSidebarSection("Stashes");
    await $("span*=Try a closer default zoom").waitForExist();
    await shoot("stashes");
  });

  it("tags", async () => {
    await collapseSidebarSection("Stashes");
    await expandSidebarSection("Tags");
    await $('button[aria-label="Actions for v0.2.0"]').waitForExist();
    await shoot("tags");
    await collapseSidebarSection("Tags");
    await expandSidebarSection("Branches");
  });

  it("command-palette", async () => {
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("branch");
    // Any matching command will do: the palette lists several.
    await waitUntilInPage("li", "Branch", { contains: true });
    await browser.saveScreenshot(path.join(ASSETS, "command-palette.png"));
    await browser.keys(["Escape"]);
    await input.waitForDisplayed({ reverse: true });
  });

  it("conflict", async () => {
    startConflict();
    // Reload status through the palette's "Refresh" command.
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("Refresh");
    await browser.keys(["Enter"]);
    // The status strip and the commit box both offer it.
    await waitUntilInPage("button", "Abort merge", { contains: true });
    await click("li*=Uncommitted changes");
    // Bring the conflicted file's resolve controls into view.
    const conflicted = await $("span*=src/search.js (Conflicted)");
    await conflicted.waitForExist();
    await browser.execute((el) => {
      (el as HTMLElement).scrollIntoView({ block: "start" });
      // scrollIntoView also scrolls ancestors that only clip (overflow: hidden), shifting the whole
      // window; put those back so only the diff pane's own scroll area moved.
      for (let node = (el as HTMLElement).parentElement; node !== null; node = node.parentElement) {
        const overflowY = getComputedStyle(node).overflowY;
        if (overflowY !== "auto" && overflowY !== "scroll") node.scrollTop = 0;
      }
    }, conflicted);
    await shoot("conflict");
  });

  it("dark-theme", async () => {
    await setTheme("dark");
    await shoot("dark-theme");
    await setTheme("light");
  });
});
