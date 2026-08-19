import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

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
    branchName = execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], {
      cwd: E2E_REPO_PATH,
    }).toString().trim();

    fs.writeFileSync(path.join(E2E_REPO_PATH, "reflog-first.txt"), "first\n");
    execFileSync("git", ["add", "reflog-first.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", FIRST_SUMMARY], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    firstCommitId = execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: E2E_REPO_PATH,
    }).toString().trim();

    fs.writeFileSync(path.join(E2E_REPO_PATH, "reflog-second.txt"), "second\n");
    execFileSync("git", ["add", "reflog-second.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", SECOND_SUMMARY], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  });

  it("restores a prior local branch entry and refreshes the selected branch history", async () => {
    // "Reflog" holds the reference selector and restore controls; "Branches" holds the switcher
    // whose text is asserted after the restore. Both default closed.
    await expandSidebarSection("Reflog");
    await expandSidebarSection("Branches");

    const branchRef = `refs/heads/${branchName}`;
    const selector = await $("aria/Reflog reference");
    await selector.waitForExist({ timeout: 10000 });
    await selector.selectByAttribute("value", branchRef);

    await browser.waitUntil(async () => (await (await $$(`button=Restore ${branchRef}`)).length) >= 2, {
      timeout: 10000,
      timeoutMsg: "expected reflog entries for both fixture commits",
    });
    const restoreButtons = await $$(`button=Restore ${branchRef}`);
    await restoreButtons[1].click();

    const dialog = await $(`aria/Restore ${branchRef}`);
    await dialog.waitForExist({ timeout: 10000 });
    await expect(dialog).toHaveText(expect.stringContaining(firstCommitId));
    await (await dialog.$("button=Restore reflog entry")).click();

    const branchSwitcher = await $("aria/Branch switcher");
    await expect(branchSwitcher).toHaveText(branchName);
    await browser.waitUntil(
      async () => await graphHasSummary(FIRST_SUMMARY),
      { timeout: 10000, timeoutMsg: "expected the restored commit in the refreshed graph" },
    );
    await browser.waitUntil(
      async () => !(await graphHasSummary(SECOND_SUMMARY)),
      { timeout: 10000, timeoutMsg: "expected the newer commit to leave the refreshed graph" },
    );
  });
});
