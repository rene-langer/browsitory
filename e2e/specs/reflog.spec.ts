import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage } from "../support/dom";
import { expandSidebarSection } from "../support/sidebar";
import { git } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FIRST_SUMMARY = "e2e: reflog first recovery point";
const SECOND_SUMMARY = "e2e: reflog second recovery point";

let branchName: string;
let firstCommitId: string;

async function graphHasSummary(summary: string): Promise<boolean> {
  const rows = await browser.$$(`//li[contains(@class, "commit-row") and contains(., "${summary}")]`);
  return (await rows.length) > 0;
}

describe("Browsitory reflog recovery", () => {
  before(() => {
    branchName = git(E2E_REPO_PATH, "rev-parse", "--abbrev-ref", "HEAD");

    fs.writeFileSync(path.join(E2E_REPO_PATH, "reflog-first.txt"), "first\n");
    git(E2E_REPO_PATH, "add", "reflog-first.txt");
    git(E2E_REPO_PATH, "commit", "-m", FIRST_SUMMARY);
    firstCommitId = git(E2E_REPO_PATH, "rev-parse", "HEAD");

    fs.writeFileSync(path.join(E2E_REPO_PATH, "reflog-second.txt"), "second\n");
    git(E2E_REPO_PATH, "add", "reflog-second.txt");
    git(E2E_REPO_PATH, "commit", "-m", SECOND_SUMMARY);
  });

  it("restores a prior local branch entry and refreshes the selected branch history", async () => {
    // "Reflog" holds the reference selector and restore controls; "Branches" holds the
    // current-branch row whose text is asserted after the restore. Both default closed.
    await expandSidebarSection("Reflog");
    await expandSidebarSection("Branches");

    const branchRef = `refs/heads/${branchName}`;
    const selector = await $("aria/Reflog reference");
    await selector.waitForExist();
    await selector.selectByAttribute("value", branchRef);

    await browser.waitUntil(async () => (await (await $$(`button=Restore ${branchRef}`)).length) >= 2, {
      timeoutMsg: "expected reflog entries for both fixture commits",
    });
    const restoreButtons = await $$(`button=Restore ${branchRef}`);
    await restoreButtons[1].click();

    const dialog = await $(`aria/Restore ${branchRef}`);
    await dialog.waitForExist();
    await expect(dialog).toHaveText(expect.stringContaining(firstCommitId));
    // In the page (`clickInPage`): the button sits at the window's bottom edge, where
    // WebKitWebDriver's own click stopped arriving (#751).
    await clickInPage("dialog[open] button", "Restore reflog entry");

    // BranchTree has no separate switcher control any more — the current branch is just the row
    // in the Local folder carrying the " (current)" suffix. The row itself is the interactive
    // element (a `role="button"` `<li>`, not a nested `<button>` — see `ListRow.tsx`).
    // `starts-with`: the row may also carry ahead/behind counts (↑n ↓m) after the name.
    const currentBranchRow = await $(
      `//li[@role="button" and starts-with(normalize-space(.), "${branchName} (current)")]`,
    );
    await currentBranchRow.waitForExist();
    await browser.waitUntil(
      async () => await graphHasSummary(FIRST_SUMMARY),
      { timeoutMsg: "expected the restored commit in the refreshed graph" },
    );
    await browser.waitUntil(
      async () => !(await graphHasSummary(SECOND_SUMMARY)),
      { timeoutMsg: "expected the newer commit to leave the refreshed graph" },
    );
  });
});
