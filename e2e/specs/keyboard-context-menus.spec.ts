import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { openPalette, waitForAppReady } from "../support/app";
import { waitUntilGoneInPage } from "../support/dom";
import { expandSidebarSection, sidebarRemoteFolder } from "../support/sidebar";
import { git } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-keyboard-remote.git");
const REMOTE_NAME = "kbd-origin";
const TAG_NAME = "e2e-kbd-tag";

// Opens the palette with the shared retrying open (`support/app.ts`) and runs a command. `label`
// is the full label of the command to run, typed as the query.
async function runPaletteCommand(label: string): Promise<void> {
  await waitForAppReady();
  const input = await openPalette();
  await input.setValue(label);
  // The Branches section renders as soon as the repo is open, before the first `refresh()` after
  // `beforeTest`'s reload has listed the remotes, and the palette has one entry per listed
  // remote. Enter pressed before that entry exists runs nothing and leaves the palette open
  // (#434), so wait until the highlighted option, the one Enter runs, is this command.
  // `textContent` via `browser.execute`: `getText()` returns "" inside the palette's native
  // <dialog> (see `command-palette.spec.ts`).
  await browser.waitUntil(
    async () =>
      (await browser.execute((el) => {
        const id = (el as HTMLElement).getAttribute("aria-activedescendant");
        return id === null ? null : (document.getElementById(id)?.textContent ?? null);
      }, input)) === label,
    { timeoutMsg: `expected the command palette to highlight "${label}"` },
  );
  await browser.keys(["Enter"]);
  await input.waitForDisplayed({ reverse: true });
}

describe("Browsitory keyboard context menus", () => {
  // Non-destructive: one remote and one tag added here and removed in `after`; the tests only
  // open and close menus and dialogs.
  before(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    git(os.tmpdir(), "init", "--bare", BARE_REMOTE_PATH);
    git(E2E_REPO_PATH, "remote", "add", REMOTE_NAME, BARE_REMOTE_PATH);
    git(E2E_REPO_PATH, "tag", "-f", TAG_NAME);
  });

  after(() => {
    git(E2E_REPO_PATH, "tag", "-d", TAG_NAME);
    git(E2E_REPO_PATH, "remote", "remove", REMOTE_NAME);
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
  });

  it("opens a branch menu with Shift+F10 and returns focus to the row on Escape", async () => {
    await expandSidebarSection("Branches");
    const row = await $('//li[@role="button" and contains(., " (current)")]');
    await row.waitForExist();
    await browser.execute((el) => (el as HTMLElement).focus(), row);

    await browser.keys(["Shift", "F10"]);
    const menu = await $("[role='menu']");
    await menu.waitForExist();
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
    const header = await $(sidebarRemoteFolder(REMOTE_NAME));
    await header.waitForExist();
    await browser.execute((el) => (el as HTMLElement).focus(), header);

    await browser.keys(["Shift", "F10"]);
    const menu = await $("[role='menu']");
    await menu.waitForExist();
    await browser.pause(300);
    expect(await $$("[role='menu']")).toHaveLength(1);
    expect(await browser.execute(() => document.activeElement?.getAttribute("role"))).toBe("menuitem");

    await browser.keys(["Escape"]);
    await waitUntilGoneInPage("[role='menu']");
    expect(await browser.execute((el) => document.activeElement === (el as Element), header)).toBe(true);
  });

  it("opens a tag row's menu from the keyboard and returns focus on Escape", async () => {
    await expandSidebarSection("Tags");
    const actions = await $(`button[aria-label="Actions for ${TAG_NAME}"]`);
    await actions.waitForExist();
    await browser.execute((el) => (el as HTMLElement).focus(), actions);

    await browser.keys(["Shift", "F10"]);
    const menu = await $("[role='menu']");
    await menu.waitForExist();
    await browser.pause(300);
    expect(await $$("[role='menu']")).toHaveLength(1);
    expect(await browser.execute(() => document.activeElement?.getAttribute("role"))).toBe("menuitem");
    const labels = await browser.execute(() =>
      Array.from(document.querySelectorAll("[role='menu'] [role='menuitem']")).map((item) => item.textContent),
    );
    expect(labels).toEqual(expect.arrayContaining(["Branch from tag", "Delete from remote…", "Delete tag…"]));

    await browser.keys(["Escape"]);
    await waitUntilGoneInPage("[role='menu']");
    expect(await browser.execute((el) => document.activeElement === (el as Element), actions)).toBe(true);
  });

  it("opens Edit remote from the palette", async () => {
    await runPaletteCommand(`Edit remote ${REMOTE_NAME}…`);
    const dialog = await $(`dialog[aria-label='Edit ${REMOTE_NAME}']`);
    await dialog.waitForExist();
    await browser.keys(["Escape"]);
    await waitUntilGoneInPage(`dialog[aria-label='Edit ${REMOTE_NAME}']`);
  });
});
