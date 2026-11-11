import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage } from "../support/dom";
import { gitIn } from "../support/git";
import { SLOW_UI_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "search-e2e.txt";
const SUMMARY = "e2e: search target";
const AUTHOR = "Searchy McAuthor <searchy@example.com>";
const QUERY = "author:searchy";

const git = gitIn(E2E_REPO_PATH);

describe("Browsitory commit search", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "find me\n");
    git("add", FILE);
    git("commit", "-m", SUMMARY, `--author=${AUTHOR}`);
  });

  it("highlights a match in the graph, lists it with List, and closes back to the graph", async () => {
    const input = await $(`input[aria-label="Search commits"]`);
    await input.waitForExist();
    // Type the query and wait for the match to be selected in the graph (#687), checking both in
    // one in-page step per poll. In CI (#479) a query typed right after the `beforeTest` reload
    // did not always stick, so each poll puts the query back whenever the field does not hold it
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
            const count = document.querySelector('[role="toolbar"][aria-label="Search matches"] [role="status"]');
            const selected = document.querySelector('ul[aria-label="Commit history"] li[aria-selected="true"]');
            return (count?.textContent ?? "") === "1 of 1" && (selected?.textContent ?? "").includes(summary);
          },
          QUERY,
          SUMMARY,
        ),
      { timeout: SLOW_UI_TIMEOUT, timeoutMsg: `no selected match for "${QUERY}" containing "${SUMMARY}"` },
    );
    await (await $(`dd*=Searchy McAuthor`)).waitForExist();

    // List: the matches alone, as before #687.
    await clickInPage(`[aria-label="Search matches"] button`, "List", { contains: true });
    await clickInPage(`ul[aria-label="Search results"] li`, SUMMARY, { contains: true });
    await (await $(`dd*=Searchy McAuthor`)).waitForExist();
    // Back to the graph view, the default the other specs expect.
    await clickInPage(`[aria-label="Search matches"] button`, "List", { contains: true });
    await (await $(`ul[aria-label="Commit history"]`)).waitForExist();

    const close = await $(`button[aria-label="Close search"]`);
    await browser.execute((el) => (el as HTMLElement).click(), close);
    await (await $(`ul[aria-label="Commit history"]`)).waitForExist();
    await expect($(`[aria-label="Search matches"]`)).not.toBeExisting();
    await expect($(`input[aria-label="Search commits"]`)).toHaveValue("");
  });
});
