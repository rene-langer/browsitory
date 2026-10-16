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
    await browser.waitUntil(() => fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt")), {
      timeout: 10000,
    });
    expect(git("log", "-1", "--format=%s")).toBe("e2e: commit to cherry-pick");

    // The picked copy is now HEAD on baseBranch; it is the first matching row.
    await openCommitMenu("e2e: commit to cherry-pick");
    await clickMenuItem("Revert");
    await browser.waitUntil(() => !fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt")), {
      timeout: 10000,
    });
    expect(git("log", "-1", "--format=%s")).toBe('Revert "e2e: commit to cherry-pick"');

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
