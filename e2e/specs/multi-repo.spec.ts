import path from "node:path";
import os from "node:os";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const E2E_SECOND_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-second-repo");

async function waitForAppReady(): Promise<void> {
  await $('section[aria-label="Branches"] button[aria-expanded]').waitForExist({ timeout: 10000 });
}

describe("Browsitory multi-repo tabs", () => {
  it("opens a second, independent repo as a new tab, switches, and isolates per-tab state", async () => {
    await waitForAppReady();

    await browser.execute((el) => (el as HTMLElement).click(), await $('button[aria-label="Open another repository"]'));

    const secondRepoRow = await $(`li*=${E2E_SECOND_REPO_PATH}`);
    await secondRepoRow.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), secondRepoRow);

    await browser.waitUntil(
      async () => (await $$('[role="tab"]')).length === 2,
      { timeout: 10000, timeoutMsg: "expected a second tab after opening the second repo" },
    );

    const secondTab = await $(`button[title="${E2E_SECOND_REPO_PATH}"]`);
    await secondTab.waitForExist({ timeout: 10000 });
    expect(await secondTab.getAttribute("aria-selected")).toBe("true");

    const secondRepoCommit = await $("li*=e2e: second repo base commit");
    await secondRepoCommit.waitForExist({ timeout: 10000 });

    // Switch back to the first tab and confirm the second repo's commit isn't visible there —
    // proves per-tab state isolation, not just that two tabs exist.
    const firstTab = await $(`button[title="${E2E_REPO_PATH}"]`);
    await browser.execute((el) => (el as HTMLElement).click(), firstTab);
    await browser.waitUntil(
      async () => (await firstTab.getAttribute("aria-selected")) === "true",
      { timeout: 10000, timeoutMsg: "expected switching back to focus the first tab" },
    );
    // `isDisplayed()`, not `isExisting()`: `RepoWorkspace` keeps every open tab's DOM mounted at
    // all times (toggling `display: none`/`display: contents` for snappy tab switching, see
    // `App.tsx`), so the second repo's own (now-hidden) workspace still legitimately contains
    // this `<li>` in the DOM even with correct per-tab isolation — `isExisting()` would find it
    // regardless of whether the first tab's data ever leaked. `isDisplayed()` resolves through
    // the `display: none` ancestor and would also correctly catch a real leak: if this commit
    // ever rendered inside the *first* (active) tab's own workspace, that `<li>` comes first in
    // document order (workspaces render in `openRepos.openRepos` order) and `$` would match it.
    const secondRepoCommitFromFirstTab = await $("li*=e2e: second repo base commit");
    expect(await secondRepoCommitFromFirstTab.isDisplayed()).toBe(false);

    await browser.execute(
      (el) => (el as HTMLElement).click(),
      await $(`button[aria-label="Close ${path.basename(E2E_SECOND_REPO_PATH)}"]`),
    );
    await browser.waitUntil(
      async () => (await $$('[role="tab"]')).length === 1,
      { timeout: 10000, timeoutMsg: "expected the second tab to close" },
    );
  });
});
