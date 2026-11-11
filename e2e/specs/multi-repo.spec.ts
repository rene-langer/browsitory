import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { expect } from "@wdio/globals";
import { openPalette, waitForAppReady } from "../support/app";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const E2E_SECOND_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-second-repo");

describe("Browsitory multi-repo tabs", () => {
  it("opens a second, independent repo as a new tab, switches, and isolates per-tab state", async () => {
    await waitForAppReady();

    await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));

    const secondRepoRow = await $(`li*=${E2E_SECOND_REPO_PATH}`);
    await secondRepoRow.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), secondRepoRow);

    await browser.waitUntil(
      async () => (await (await $$('[role="tab"]')).length) === 2,
      { timeoutMsg: "expected a second tab after opening the second repo" },
    );

    const secondTab = await $(`button[title="${E2E_SECOND_REPO_PATH}"]`);
    await secondTab.waitForExist();
    expect(await secondTab.getAttribute("aria-selected")).toBe("true");

    const secondRepoCommit = await $('[data-active-repo="true"]').$("li*=e2e: second repo base commit");
    await secondRepoCommit.waitForExist();

    // Switch back to the first tab and confirm the second repo's commit isn't visible there —
    // proves per-tab state isolation, not just that two tabs exist.
    const firstTab = await $(`button[title="${E2E_REPO_PATH}"]`);
    await browser.execute((el) => (el as HTMLElement).click(), firstTab);
    await browser.waitUntil(
      async () => (await firstTab.getAttribute("aria-selected")) === "true",
      { timeoutMsg: "expected switching back to focus the first tab" },
    );
    // The second tab's workspace stays mounted, hidden (#642), so look in the shown workspace:
    // any match there can only mean the second repo's commit leaked into the first tab's view.
    const firstPanelId = await firstTab.getAttribute("aria-controls");
    await browser.waitUntil(
      async () => (await $('[data-active-repo="true"]').getAttribute("id")) === firstPanelId,
      { timeoutMsg: "expected the first tab's workspace to be the shown one" },
    );
    const secondRepoCommitFromFirstTab = await $('[data-active-repo="true"]').$("li*=e2e: second repo base commit");
    expect(await secondRepoCommitFromFirstTab.isExisting()).toBe(false);

    // #613: a change made in the inactive second repo shows its tab's "uncommitted changes"
    // marker once the user has been there and left again (the check on leaving a tab).
    const untracked = path.join(E2E_SECOND_REPO_PATH, "e2e-tab-marker.txt");
    const marked = async () => (await secondTab.getAttribute("aria-describedby")) !== null;
    try {
      fs.writeFileSync(untracked, "uncommitted\n");
      await browser.execute((el) => (el as HTMLElement).click(), secondTab);
      await browser.waitUntil(async () => (await secondTab.getAttribute("aria-selected")) === "true", {
        timeoutMsg: "expected switching to the second tab",
      });
      await browser.execute((el) => (el as HTMLElement).click(), firstTab);
      await browser.waitUntil(marked, { timeoutMsg: "expected the inactive second tab to show the changes marker" });
      const description = await browser.execute(
        (id) => document.getElementById(id)?.textContent ?? null,
        (await secondTab.getAttribute("aria-describedby")) ?? "",
      );
      expect(description).toBe("Uncommitted changes");
      expect(await firstTab.getAttribute("aria-describedby")).toBeNull();
    } finally {
      fs.unlinkSync(untracked);
    }

    // The per-tab close button has no accessible name (it's `aria-hidden`/`tabIndex={-1}`,
    // mouse-only — see `RepoTabs.tsx`), so it's found structurally: the `<button>` right after
    // the second repo's `role="tab"` button, inside the same `role="presentation"` wrapper.
    await browser.execute(
      (el) => (el as HTMLElement).click(),
      await $(`//button[@role="tab" and @title="${E2E_SECOND_REPO_PATH}"]/following-sibling::button[1]`),
    );
    await browser.waitUntil(
      async () => (await (await $$('[role="tab"]')).length) === 1,
      { timeoutMsg: "expected the second tab to close" },
    );

    // #613: "Reopen closed tab" brings it back after the first tab, active again.
    const input = await openPalette();
    await input.setValue("reopen closed tab");
    await browser.waitUntil(
      async () => (await browser.execute(() => document.querySelector("dialog li")?.textContent ?? "")).includes("Reopen closed tab"),
      { timeoutMsg: "expected the palette to offer Reopen closed tab" },
    );
    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true });
    await browser.waitUntil(
      async () =>
        (
          await browser.execute(() =>
            Array.from(document.querySelectorAll('[role="tab"]'))
              .map((tab) => `${tab.getAttribute("title")}:${tab.getAttribute("aria-selected")}`)
              .join("|"),
          )
        ) === `${E2E_REPO_PATH}:false|${E2E_SECOND_REPO_PATH}:true`,
      { timeoutMsg: "expected the second tab back in its old place, active" },
    );

    // Leave one tab open, as the spec found it.
    await browser.execute(
      (el) => (el as HTMLElement).click(),
      await $(`//button[@role="tab" and @title="${E2E_SECOND_REPO_PATH}"]/following-sibling::button[1]`),
    );
    await browser.waitUntil(
      async () => (await (await $$('[role="tab"]')).length) === 1,
      { timeoutMsg: "expected the reopened second tab to close" },
    );
  });

  // #625: a tab's own name survives a reload (the tabs are restored from the config), and the
  // tab list switches between tabs by name.
  it("renames a tab, keeps the name across a reload, and picks a tab from the tab list", async () => {
    await waitForAppReady();
    const tabNames = () =>
      browser.execute(() =>
        Array.from(document.querySelectorAll('[role="tab"]'))
          .map((tab) => `${tab.getAttribute("title")}=${(tab.textContent ?? "").trim()}:${tab.getAttribute("aria-selected")}`)
          .join("|"),
      );
    const rename = async (name: string) => {
      const field = await $('dialog[aria-label="Rename tab"] input');
      await field.waitForDisplayed();
      // Typed, not `clearValue()`: WebKitGTK's clear fires no input event, so React keeps the old
      // name. The field selects its text on focus, so Backspace empties it.
      await browser.execute((el) => (el as HTMLInputElement).select(), field);
      await browser.keys(["Backspace"]);
      if (name !== "") await field.addValue(name);
      await browser.execute((el) => (el as HTMLElement).click(), await $('dialog[aria-label="Rename tab"] button[type="submit"]'));
      await field.waitForExist({ reverse: true });
    };

    // A synthetic contextmenu event, as rebase.spec.ts does (a real right-click leaves WebKitGTK's
    // modifier state stuck).
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true }));
    }, await $(`button[title="${E2E_REPO_PATH}"]`));
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Rename tab…"));
    await rename("e2e main");
    await browser.waitUntil(async () => (await tabNames()) === `${E2E_REPO_PATH}=e2e main:true`, {
      timeoutMsg: "expected the tab to show its new name",
    });

    await browser.refresh();
    await waitForAppReady();
    await browser.waitUntil(async () => (await tabNames()) === `${E2E_REPO_PATH}=e2e main:true`, {
      timeoutMsg: "expected the tab name to come back after a reload",
    });

    await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));
    const secondRepoRow = await $(`li*=${E2E_SECOND_REPO_PATH}`);
    await secondRepoRow.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), secondRepoRow);
    await browser.waitUntil(async () => (await (await $$('[role="tab"]')).length) === 2, {
      timeoutMsg: "expected a second tab",
    });

    const pickFromList = async (filter: string) => {
      await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="List open tabs"]'));
      const input = await $('input[aria-label="Filter open tabs"]');
      await input.waitForDisplayed();
      await input.setValue(filter);
      await browser.keys(["Enter"]);
      await input.waitForExist({ reverse: true });
    };
    await pickFromList("e2e main");
    await browser.waitUntil(
      async () => (await tabNames()) === `${E2E_REPO_PATH}=e2e main:true|${E2E_SECOND_REPO_PATH}=browsitory-e2e-second-repo:false`,
      { timeoutMsg: "expected the tab list to switch to the renamed tab" },
    );
    await pickFromList("second-repo");
    await browser.waitUntil(
      async () => (await tabNames()) === `${E2E_REPO_PATH}=e2e main:false|${E2E_SECOND_REPO_PATH}=browsitory-e2e-second-repo:true`,
      { timeoutMsg: "expected the tab list to switch to the second tab" },
    );

    // Leave the spec as it found it: one tab, under its folder name. F2 opens the rename dialog.
    await browser.execute(
      (el) => (el as HTMLElement).click(),
      await $(`//button[@role="tab" and @title="${E2E_SECOND_REPO_PATH}"]/following-sibling::button[1]`),
    );
    const firstTab = await $(`button[title="${E2E_REPO_PATH}"]`);
    await browser.waitUntil(async () => (await firstTab.getAttribute("aria-selected")) === "true", {
      timeoutMsg: "expected the first tab active again",
    });
    await browser.execute((el) => (el as HTMLElement).focus(), firstTab);
    await browser.keys(["F2"]);
    await rename("");
    await browser.waitUntil(async () => (await tabNames()) === `${E2E_REPO_PATH}=browsitory-e2e-repo:true`, {
      timeoutMsg: "expected an empty name to restore the folder name",
    });
  });
});
