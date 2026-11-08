import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const E2E_SUBMODULE_PATH = "deps/e2e-child";
const OPEN_IN_NEW_TAB = "//div[@role='menu']//button[@role='menuitem' and normalize-space(.)='Open in new tab']";

// Opens the submodule row's menu from the keyboard: focus its "…" (shown only on hover or focus,
// so focused by script rather than clicked) and press Shift+F10.
async function openSubmoduleMenu(): Promise<void> {
  const actions = await $(`button[aria-label='Actions for ${E2E_SUBMODULE_PATH}']`);
  await actions.waitForExist({ timeout: 10000 });
  await browser.execute((el) => (el as HTMLElement).focus(), actions);
  await browser.keys(["Shift", "F10"]);
  await $("[role='menu']").waitForExist({ timeout: 10000 });
}

async function closeMenu(): Promise<void> {
  await browser.keys(["Escape"]);
  await $("[role='menu']").waitForExist({ reverse: true, timeout: 10000 });
}

describe("Browsitory submodules", () => {
  it("initializes and updates a local submodule, then shows its changed gitlink in parent status", async () => {
    // "Submodules" defaults closed; expand it before its Initialize/Update buttons exist.
    await expandSidebarSection("Submodules");

    const initializeButton = await $("button=Initialize " + E2E_SUBMODULE_PATH);
    await initializeButton.waitForExist({ timeout: 10000 });

    // Not checked out yet: nothing to open in a tab (#445), and the menu offers Initialize.
    await openSubmoduleMenu();
    const openItem = await $(OPEN_IN_NEW_TAB);
    expect(await openItem.isEnabled()).toBe(false);
    expect(await openItem.getAttribute("title")).toContain("Not initialized");
    await expect($("//div[@role='menu']//button[@role='menuitem' and normalize-space(.)='Initialize']")).toBeExisting();
    await closeMenu();

    await initializeButton.click();
    const initializedIndicator = await $("span=Initialized");
    await initializedIndicator.waitForExist({ timeout: 10000 });

    const updateButton = await $("button=Update " + E2E_SUBMODULE_PATH);
    await updateButton.click();
    const childPath = path.join(E2E_REPO_PATH, E2E_SUBMODULE_PATH);
    await updateButton.waitForEnabled({ timeout: 10000 });
    await browser.waitUntil(
      () => {
        try {
          const topLevel = execFileSync("git", ["rev-parse", "--show-toplevel"], {
            cwd: childPath,
            encoding: "utf8",
          }).trim();
          if (path.resolve(topLevel) !== path.resolve(childPath)) {
            return false;
          }
          execFileSync("git", ["rev-parse", "--verify", "HEAD^{commit}"], {
            cwd: childPath,
            stdio: "ignore",
          });
          return true;
        } catch {
          return false;
        }
      },
      { timeout: 10000, timeoutMsg: "expected the child checkout to have its own valid HEAD" },
    );

    execFileSync("git", ["config", "user.name", "Test User"], { cwd: childPath });
    fs.writeFileSync(path.join(childPath, "advanced.txt"), "advanced child commit\n");
    execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: childPath });
    execFileSync("git", ["add", "."], { cwd: childPath });
    execFileSync("git", ["commit", "-m", "e2e: advance child"], { cwd: childPath });

    await initializeButton.click();
    // The uncommitted file list renders each row's `{path} ({kind})` text in a `<span>` inside
    // an `<li role="option">` (see `DiffPane.tsx`'s `FileListRow`), not in a `<button>`.
    const gitlinkStatus = await $("span=" + E2E_SUBMODULE_PATH + " (Modified)");
    await gitlinkStatus.waitForExist({ timeout: 10000 });
    await expect(gitlinkStatus).toBeExisting();
  });

  // Uses the checked-out submodule the test above left behind.
  it("opens the checked-out submodule in its own repository tab from the row menu (#445)", async () => {
    await expandSidebarSection("Submodules");
    await openSubmoduleMenu();
    const openItem = await $(OPEN_IN_NEW_TAB);
    await browser.waitUntil(() => openItem.isEnabled(), {
      timeout: 10000,
      timeoutMsg: "expected Open in new tab to be enabled for the checked-out submodule",
    });
    // The menu focuses its first enabled item, Open in new tab.
    expect(await browser.execute(() => document.activeElement?.textContent)).toBe("Open in new tab");
    await browser.keys(["Enter"]);

    await browser.waitUntil(async () => (await $$('[role="tab"]')).length === 2, {
      timeout: 10000,
      timeoutMsg: "expected a second tab for the submodule",
    });
    const submoduleTab = await $(`//button[@role="tab" and contains(@title, "e2e-child")]`);
    await submoduleTab.waitForExist({ timeout: 10000 });
    expect(await submoduleTab.getAttribute("aria-selected")).toBe("true");
    expect(path.resolve(await submoduleTab.getAttribute("title"))).toBe(
      path.resolve(E2E_REPO_PATH, E2E_SUBMODULE_PATH),
    );
    // The tab shows the submodule's own history, not the superproject's.
    await $("li*=e2e: advance child").waitForExist({ timeout: 10000 });

    // Close it again (the close button sits right after the tab, see `multi-repo.spec.ts`).
    await browser.execute(
      (el) => (el as HTMLElement).click(),
      await $(`//button[@role="tab" and contains(@title, "e2e-child")]/following-sibling::button[1]`),
    );
    await browser.waitUntil(async () => (await $$('[role="tab"]')).length === 1, {
      timeout: 10000,
      timeoutMsg: "expected the submodule tab to close",
    });
  });
});
