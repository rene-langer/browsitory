import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage } from "../support/dom";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "search-e2e.txt";
const SUMMARY = "e2e: search target";
const AUTHOR = "Searchy McAuthor <searchy@example.com>";
const QUERY = "author:searchy";

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
    // Type the query and wait for the matching row, checking both in one in-page step per
    // poll. A chained `$(list).$(row)` lookup gives up when the list is not there after the
    // implicit wait, and in CI (#479) the list never appeared at all: the list renders as soon as
    // the debounced search starts, so the query typed right after the `beforeTest` reload did
    // not stick. Each poll therefore puts the query back whenever the field does not hold it
    // (the same native-setter workaround the other specs use for controlled inputs).
    await browser.waitUntil(
      () =>
        browser.execute(
          (query: string, summary: string) => {
            const field = document.querySelector<HTMLInputElement>('input[aria-label="Search commits"]');
            if (field === null) return false;
            if (field.value !== query) {
              const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
              setter?.call(field, query);
              field.dispatchEvent(new Event("input", { bubbles: true }));
              return false;
            }
            return Array.from(document.querySelectorAll('ul[aria-label="Search results"] li')).some(
              (row) => (row.textContent ?? "").includes(summary),
            );
          },
          QUERY,
          SUMMARY,
        ),
      { timeout: 15000, timeoutMsg: `no search result for "${QUERY}" containing "${SUMMARY}"` },
    );
    await clickInPage(`ul[aria-label="Search results"] li`, SUMMARY, { contains: true });
    await (await $(`dd*=Searchy McAuthor`)).waitForExist({ timeout: 10000 });

    const close = await $(`button[aria-label="Close search"]`);
    await browser.execute((el) => (el as HTMLElement).click(), close);
    await (await $(`ul[aria-label="Commit history"]`)).waitForExist({ timeout: 10000 });
    await expect($(`input[aria-label="Search commits"]`)).toHaveValue("");
  });
});
