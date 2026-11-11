import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { sidebarSectionToggle } from "../support/sidebar";
import { git } from "../support/git";
import { SLOW_UI_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const SOURCE = path.join(os.tmpdir(), "browsitory-e2e-clone-source.git");
const PARENT = path.join(os.tmpdir(), "browsitory-e2e-clones");
const NAME = "e2e-cloned";
// A clone (with its submodule) is a longer job than a UI round trip.
const CLONE_TIMEOUT = 30_000;

async function setInput(label: string, value: string) {
  const input = await $(`aria/${label}`);
  await input.waitForExist();
  await input.setValue(value);
}

describe("Browsitory clone", () => {
  before(() => {
    fs.rmSync(SOURCE, { recursive: true, force: true });
    fs.rmSync(PARENT, { recursive: true, force: true });
    fs.mkdirSync(PARENT, { recursive: true });
    git(os.tmpdir(), "clone", "--bare", E2E_REPO_PATH, SOURCE);
  });

  it("clones a repository from the picker and opens it in a new tab", async () => {
    await $(sidebarSectionToggle("Branches")).waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));
    const cloneButton = await $("button=Clone…");
    await cloneButton.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), cloneButton);

    await setInput("Repository URL", `file://${SOURCE.replace(/\\/g, "/")}`);
    await setInput("Parent folder", PARENT);
    const nameInput = await $("aria/Folder name");
    await nameInput.clearValue();
    await nameInput.setValue(NAME);
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Clone"));

    const tab = await $(`[role="tab"]*=${NAME}`);
    await tab.waitForExist({ timeout: CLONE_TIMEOUT });
    expect(fs.existsSync(path.join(PARENT, NAME, ".git"))).toBe(true);
    // The first tab stays mounted in the background (#642): look in the shown workspace only.
    const seedCommit = await $('[data-active-repo="true"]').$("li*=e2e: add submodule");
    await seedCommit.waitForExist({ timeout: SLOW_UI_TIMEOUT });
  });

  // Leave one tab open, as the spec found it. A tab left open is restored by every later spec's
  // reload and shown first, before the E2E auto-open switches to the test repository; that first
  // tab's workspace then stays mounted in the background (#642), and a page-wide selector could
  // find its elements instead of the shown ones.
  after(async () => {
    const clonedTab = `//button[@role="tab" and @title="${path.join(PARENT, NAME)}"]`;
    if (!(await $(clonedTab).isExisting())) return;
    await browser.execute((el) => (el as HTMLElement).click(), await $(`${clonedTab}/following-sibling::button[1]`));
    await browser.waitUntil(async () => (await (await $$('[role="tab"]')).length) === 1, {
      timeoutMsg: "expected the cloned repository's tab to close",
    });
  });
});
