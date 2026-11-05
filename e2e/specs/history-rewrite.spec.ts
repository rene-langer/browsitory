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

/** Waits until the commit graph has a row for `commitId`. Checked inside the page. */
async function waitForGraphRow(commitId: string) {
  await browser.waitUntil(
    () =>
      browser.execute(
        (id: string) => document.querySelector(`li[data-commit-id="${id}"]`) !== null,
        commitId,
      ),
    { timeout: 10000, timeoutMsg: `the commit graph never showed commit ${commitId}` },
  );
}

/**
 * Opens the context menu of `commitId`'s row once the graph shows it, in one in-page step, so
 * no handle to a row can go stale while the graph refreshes.
 */
async function openCommitMenuById(commitId: string) {
  await waitForGraphRow(commitId);
  await browser.execute((id: string) => {
    document
      .querySelector(`li[data-commit-id="${id}"]`)
      ?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
  }, commitId);
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
    // the refresh, and the click on its Revert item then does nothing (#182). So open the
    // picked copy's own row (it is HEAD now), once the graph shows it.
    await openCommitMenuById(git("rev-parse", "HEAD"));
    await clickMenuItem("Revert");
    await browser.waitUntil(
      () => git("log", "-1", "--format=%s") === 'Revert "e2e: commit to cherry-pick"',
      { timeout: 10000 },
    );
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt"))).toBe(false);
    // Same as above: let the graph show the revert before opening the next menu.
    await waitForGraphRow(git("rev-parse", "HEAD"));
    await openCommitMenuById(startTip);
    await clickMenuItem(`Reset ${baseBranch} to here…`);
    const hard = await $("input[type=radio][value=hard]");
    await hard.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), hard);
    await clickMenuItem("Reset");
    await clickMenuItem("Reset hard");
    await browser.waitUntil(() => git("rev-parse", "HEAD") === startTip, { timeout: 10000 });
  });
});
