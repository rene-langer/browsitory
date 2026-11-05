import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const git = (...args: string[]) =>
  execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();

/**
 * Picks `label` from the context menu of `commitId`'s row in the commit graph.
 *
 * A command run from the app lands before the graph refreshes, and a refresh closes an open
 * menu. A click on an item of a menu that a refresh already closed does nothing (#182). Seeing
 * the new commit's row doesn't prove the graph has settled either. So each poll, inside the
 * page: if the menu shows `label`, click it (found and clicked in one step, so nothing can close
 * the menu in between); otherwise open the menu on the row (again, if a refresh closed it).
 */
async function chooseFromCommitMenu(commitId: string, label: string) {
  await browser.waitUntil(
    () =>
      browser.execute(
        (id: string, wanted: string) => {
          const item = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="menu"] button')).find(
            (button) => !button.disabled && (button.textContent ?? "").trim() === wanted,
          );
          if (item !== undefined) {
            item.click();
            return true;
          }
          document
            .querySelector(`li[data-commit-id="${id}"]`)
            ?.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
          return false;
        },
        commitId,
        label,
      ),
    { timeout: 10000, timeoutMsg: `could not pick "${label}" from the menu of commit ${commitId}` },
  );
}

async function clickButton(label: string) {
  const item = await $(`button*=${label}`);
  await item.waitForExist({ timeout: 10000 });
  await browser.execute((el) => (el as HTMLElement).click(), item);
}

describe("Browsitory history rewrite", () => {
  let baseBranch = "";
  let startTip = "";
  let sourceCommit = "";

  before(() => {
    baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    git("checkout", "-b", "e2e-pick-source");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "picked.txt"), "picked\n");
    git("add", "picked.txt");
    git("commit", "-m", "e2e: commit to cherry-pick");
    sourceCommit = git("rev-parse", "HEAD");
    git("checkout", baseBranch);
    startTip = git("rev-parse", "HEAD");
  });

  it("cherry-picks a commit, reverts it, then resets hard back", async () => {
    await chooseFromCommitMenu(sourceCommit, "Cherry-pick");
    // Wait on the commit, not the file: checkout writes picked.txt before the commit lands.
    await browser.waitUntil(() => git("log", "-1", "--format=%s") === "e2e: commit to cherry-pick", {
      timeout: 10000,
    });
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt"))).toBe(true);

    // The picked copy is HEAD now. (Picked in the same second as the original, it has the same
    // id, as nothing in the commit differs.)
    await chooseFromCommitMenu(git("rev-parse", "HEAD"), "Revert");
    await browser.waitUntil(
      () => git("log", "-1", "--format=%s") === 'Revert "e2e: commit to cherry-pick"',
      { timeout: 10000 },
    );
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "picked.txt"))).toBe(false);

    await chooseFromCommitMenu(startTip, `Reset ${baseBranch} to here…`);
    const hard = await $("input[type=radio][value=hard]");
    await hard.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), hard);
    await clickButton("Reset");
    await clickButton("Reset hard");
    await browser.waitUntil(() => git("rev-parse", "HEAD") === startTip, { timeout: 10000 });
  });
});
