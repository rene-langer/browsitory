import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const git = (...args: string[]) =>
  execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();

async function openCommitMenu(summary: string) {
  const row = await $(`li*=${summary}`);
  await row.waitForExist({ timeout: 10000 });
  await browser.execute((el) => {
    el.dispatchEvent(
      new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
    );
  }, row);
}

/**
 * Waits until the commit graph shows `count` rows (innermost `<li>`s) containing `summary`.
 * Counted inside the page, so no handle to a row can go stale while the graph refreshes.
 */
async function waitForGraphRows(summary: string, count: number) {
  await browser.waitUntil(
    () =>
      browser.execute(
        (text: string) =>
          Array.from(document.querySelectorAll("li")).filter(
            (li) => li.querySelector("li") === null && (li.textContent ?? "").includes(text),
          ).length,
        summary,
      ).then((found) => found === count),
    { timeout: 10000, timeoutMsg: `the commit graph never showed ${count} row(s) with "${summary}"` },
  );
}

async function clickMenuItem(label: string) {
  const item = await $(`button*=${label}`);
  await item.waitForExist({ timeout: 10000 });
  await browser.execute((el) => (el as HTMLElement).click(), item);
}

describe("Browsitory history rewrite", () => {
  let baseBranch = "";
  let startTip = "";

  before(() => {
    baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    git("checkout", "-b", "e2e-pick-source");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "picked.txt"), "picked\n");
    git("add", "picked.txt");
    git("commit", "-m", "e2e: commit to cherry-pick");
    git("checkout", baseBranch);
    startTip = git("rev-parse", "HEAD");
  });

  it("cherry-picks a commit, reverts it, then resets hard back", async () => {
    await openCommitMenu("e2e: commit to cherry-pick");
    await clickMenuItem("Cherry-pick");
    // Wait on the commit, not the file: checkout writes picked.txt before the commit lands.
    await browser.waitUntil(() => git("log", "-1", "--format=%s") === "e2e: commit to cherry-pick", {
      timeout: 10000,
    });
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt"))).toBe(true);
    // The commit lands before the graph refreshes. A menu opened on the old graph is closed by
    // the refresh, and the click on its Revert item then does nothing (#182). Wait for the
    // picked copy's row next to the original's.
    await waitForGraphRows("e2e: commit to cherry-pick", 2);

    // The picked copy is now HEAD on baseBranch; it is the first matching row.
    await openCommitMenu("e2e: commit to cherry-pick");
    await clickMenuItem("Revert");
    await browser.waitUntil(
      () => git("log", "-1", "--format=%s") === 'Revert "e2e: commit to cherry-pick"',
      { timeout: 10000 },
    );
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt"))).toBe(false);
    // Same as above: let the graph show the revert before opening the next menu.
    await waitForGraphRows('Revert "e2e: commit to cherry-pick"', 1);

    const startSummary = git("log", "-1", "--format=%s", startTip);
    await openCommitMenu(startSummary);
    await clickMenuItem(`Reset ${baseBranch} to here…`);
    const hard = await $("input[type=radio][value=hard]");
    await hard.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), hard);
    await clickMenuItem("Reset");
    await clickMenuItem("Reset hard");
    await browser.waitUntil(() => git("rev-parse", "HEAD") === startTip, { timeout: 10000 });
  });
});
