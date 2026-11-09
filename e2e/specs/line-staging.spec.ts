import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilGoneInPage, waitUntilInPage } from "../support/dom";
import { git, gitWith } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "line-fixture.txt";
const UNTRACKED = "line-untracked.tmp";

describe("Browsitory line staging and discard", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "a\nb\nc\n");
    git(E2E_REPO_PATH, "add", FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: seed line fixture");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "a\nB\nc\nd\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, UNTRACKED), "tmp\n");
  });

  after(() => {
    git(E2E_REPO_PATH, "reset", "-q", "--", FILE);
    git(E2E_REPO_PATH, "checkout", "--", FILE);
    fs.rmSync(path.join(E2E_REPO_PATH, UNTRACKED), { force: true });
  });

  it("stages two selected lines of a hunk", async () => {
    const row = await $(`span=${FILE} (Modified)`);
    await row.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), row);

    await waitUntilInPage(".diff-line-add");
    const addLines = await $$(".diff-line-add");
    await browser.execute((el) => (el as HTMLElement).click(), addLines[0]);
    await browser.execute(
      (el) => el.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true })),
      addLines[1],
    );
    const stageLines = await $("button=Stage lines (2)");
    await browser.execute((el) => (el as HTMLElement).click(), stageLines);

    await browser.waitUntil(
      // Untrimmed: the index blob is compared whole, trailing newline included.
      () => gitWith(E2E_REPO_PATH, ["show", `:${FILE}`], { trim: false }) === "a\nb\nB\nc\nd\n",
      { timeoutMsg: "expected exactly the two added lines in the index" },
    );
  });

  it("discards an untracked file through the file menu", async () => {
    // Menu items mount and unmount as the menu and dialog open and close; look them up and
    // click them inside the page, so none can go stale between the two (#182).
    await clickInPage(`button[aria-label="Actions for ${UNTRACKED}"]`);
    await clickInPage("button", "Discard changes…");
    await clickInPage("button", "Delete file");
    await browser.waitUntil(() => !fs.existsSync(path.join(E2E_REPO_PATH, UNTRACKED)), {
      timeoutMsg: "expected the untracked file to be deleted",
    });
    // Even waitForExist({ reverse: true }) hit a stale element when the row unmounted
    // mid-lookup (#145, #182); poll inside the page instead.
    await waitUntilGoneInPage("span", `${UNTRACKED} (New)`);
  });
});
