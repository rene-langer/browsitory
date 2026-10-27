import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "search-e2e.txt";
const SUMMARY = "e2e: search target";
const AUTHOR = "Searchy McAuthor <searchy@example.com>";

function git(...args: string[]) {
  execFileSync("git", args, { cwd: E2E_REPO_PATH, stdio: "inherit" });
}

describe("Browsitory commit search", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "find me\n");
    git("add", FILE);
    git("commit", "-m", SUMMARY, `--author=${AUTHOR}`);
  });

  it("finds a commit by author, opens it, and closes back to the graph", async () => {
    const input = await $(`input[aria-label="Search commits"]`);
    await input.waitForExist({ timeout: 10000 });
    // Same native-setter workaround the other specs use for controlled inputs.
    await browser.execute((el, value) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(el, value);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, input, "author:searchy");

    // WebdriverIO's partial-text selector can't follow a CSS combinator, so chain the lookup.
    const row = await $(`ul[aria-label="Search results"]`).$(`li*=${SUMMARY}`);
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), row);
    await (await $(`dd*=Searchy McAuthor`)).waitForExist({ timeout: 10000 });

    const close = await $(`button[aria-label="Close search"]`);
    await browser.execute((el) => (el as HTMLElement).click(), close);
    await (await $(`ul[aria-label="Commit history"]`)).waitForExist({ timeout: 10000 });
    await expect(input).toHaveValue("");
  });
});
