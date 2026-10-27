import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "compare-e2e.txt";

function git(...args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim();
}

function commit(contents: string, message: string): string {
  fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), contents);
  git("add", FILE);
  git("commit", "-m", message);
  return git("rev-parse", "HEAD").slice(0, 7);
}

describe("Browsitory compare", () => {
  let base = "";
  let tip = "";

  before(() => {
    base = commit("one\n", "e2e: compare base");
    commit("one\ntwo\n", "e2e: compare middle");
    tip = commit("one\ntwo\nthree\n", "e2e: compare tip");
  });

  it("compares two commits, swaps them, and closes back to the commit view", async () => {
    // Commits made with plain git after the app loaded; reload so the graph has them.
    await browser.refresh();

    const baseRow = await $("li*=e2e: compare base");
    await baseRow.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), baseRow);
    // Synthetic ctrl-click, as in rebase.spec.ts: a real held modifier sticks in WebKitGTK.
    const tipRow = await $("li*=e2e: compare tip");
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
    }, tipRow);

    const compareButton = await $("button=Compare these commits");
    await compareButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), compareButton);

    const heading = await $(`section[aria-label="Comparison"] h2`);
    await heading.waitForExist({ timeout: 10000 });
    await expect(heading).toHaveText(`Comparing ${base}..${tip}`);
    // The From/To values are `dd`s with `text-overflow: ellipsis`; WebKitGTK's visible-text
    // getter reads them as empty, so match on their content instead of the section's text.
    await $(`section[aria-label="Comparison"]`).$("dd*=e2e: compare tip").waitForExist({ timeout: 10000 });
    await (await $(`span=${FILE}`)).waitForExist({ timeout: 10000 });

    await browser.execute((el) => (el as HTMLElement).click(), await $(`button[aria-label="Swap sides"]`));
    await expect(heading).toHaveText(`Comparing ${tip}..${base}`);

    await browser.execute((el) => (el as HTMLElement).click(), await $(`button[aria-label="Close compare"]`));
    await browser.waitUntil(async () => !(await $(`section[aria-label="Comparison"]`).isExisting()), { timeout: 10000 });
  });
});
