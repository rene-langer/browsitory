import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { waitUntilGoneInPage } from "../support/dom";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-keyboard-remote.git");
const REMOTE_NAME = "kbd-origin";

// Same retrying open as `command-palette.spec.ts`'s `openPalette` (Ctrl+K can arrive before the
// workspace's key handler is attached right after `beforeTest`'s reload).
async function runPaletteCommand(query: string): Promise<void> {
  await $('section[aria-label="Branches"] button[aria-expanded]').waitForExist({ timeout: 10000 });
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
    { timeout: 10000, interval: 0, timeoutMsg: "expected Ctrl/Cmd+K to open the command palette" },
  );
  await input.setValue(query);
  await browser.keys(["Enter"]);
  await input.waitForDisplayed({ reverse: true, timeout: 10000 });
}

describe("Browsitory keyboard context menus", () => {
  // Non-destructive: one remote added here and removed in `after`; the tests only open and
  // close menus and dialogs.
  before(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_REMOTE_PATH], { stdio: "inherit" });
    execFileSync("git", ["remote", "add", REMOTE_NAME, BARE_REMOTE_PATH], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  });

  after(() => {
    execFileSync("git", ["remote", "remove", REMOTE_NAME], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
  });

  it("opens a branch menu with Shift+F10 and returns focus to the row on Escape", async () => {
    await expandSidebarSection("Branches");
    const row = await $('//li[@role="button" and contains(., " (current)")]');
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).focus(), row);

    await browser.keys(["Shift", "F10"]);
    const menu = await $("[role='menu']");
    await menu.waitForExist({ timeout: 10000 });
    // Exactly one menu: a native contextmenu that WebKitGTK may fire after the key (on keyup) must
    // not open a second one. Give it a moment to arrive before counting.
    await browser.pause(300);
    expect(await $$("[role='menu']")).toHaveLength(1);
    // Checkout is disabled on the current branch, so Rename is the first focusable item.
    expect(await browser.execute(() => document.activeElement?.textContent)).toBe("Rename");

    await browser.keys(["ArrowDown"]);
    expect(await browser.execute(() => document.activeElement?.getAttribute("role"))).toBe("menuitem");
    expect(await browser.execute(() => document.activeElement?.textContent)).not.toBe("Rename");

    await browser.keys(["Escape"]);
    await waitUntilGoneInPage("[role='menu']");
    expect(await browser.execute((el) => document.activeElement === (el as Element), row)).toBe(true);
  });

  it("opens a remote folder's menu from the keyboard", async () => {
    await expandSidebarSection("Branches");
    const header = await $(`button=${REMOTE_NAME}`);
    await header.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).focus(), header);

    await browser.keys(["Shift", "F10"]);
    const menu = await $("[role='menu']");
    await menu.waitForExist({ timeout: 10000 });
    await browser.pause(300);
    expect(await $$("[role='menu']")).toHaveLength(1);
    expect(await browser.execute(() => document.activeElement?.getAttribute("role"))).toBe("menuitem");

    await browser.keys(["Escape"]);
    await waitUntilGoneInPage("[role='menu']");
    expect(await browser.execute((el) => document.activeElement === (el as Element), header)).toBe(true);
  });

  it("opens Edit remote from the palette", async () => {
    await runPaletteCommand(`Edit remote ${REMOTE_NAME}…`);
    const dialog = await $(`dialog[aria-label='Edit ${REMOTE_NAME}']`);
    await dialog.waitForExist({ timeout: 10000 });
    await browser.keys(["Escape"]);
    await waitUntilGoneInPage(`dialog[aria-label='Edit ${REMOTE_NAME}']`);
  });
});
