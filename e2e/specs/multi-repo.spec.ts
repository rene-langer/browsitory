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

    const secondRepoCommit = await $("li*=e2e: second repo base commit");
    await secondRepoCommit.waitForExist();

    // Switch back to the first tab and confirm the second repo's commit isn't visible there —
    // proves per-tab state isolation, not just that two tabs exist.
    const firstTab = await $(`button[title="${E2E_REPO_PATH}"]`);
    await browser.execute((el) => (el as HTMLElement).click(), firstTab);
    await browser.waitUntil(
      async () => (await firstTab.getAttribute("aria-selected")) === "true",
      { timeoutMsg: "expected switching back to focus the first tab" },
    );
    // `isExisting()`, the strict check: `App.tsx` mounts only the active tab's `RepoWorkspace`
    // (inactive tabs are unmounted, not hidden with `display: none` — PERF-001), so once the
    // first tab is active again the second repo's workspace is gone from the DOM entirely. Any
    // match here can only mean the second repo's commit leaked into the first tab's workspace.
    const secondRepoCommitFromFirstTab = await $("li*=e2e: second repo base commit");
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
});
