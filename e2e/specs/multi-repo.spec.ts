import path from "node:path";
import os from "node:os";
import { expect } from "@wdio/globals";
import { waitForAppReady } from "../support/app";

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
  });
});
