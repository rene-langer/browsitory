import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "line-fixture.txt";
const UNTRACKED = "line-untracked.tmp";

describe("Browsitory line staging and discard", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "a\nb\nc\n");
    execFileSync("git", ["add", FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: seed line fixture"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "a\nB\nc\nd\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, UNTRACKED), "tmp\n");
  });

  after(() => {
    execFileSync("git", ["reset", "-q", "--", FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["checkout", "--", FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.rmSync(path.join(E2E_REPO_PATH, UNTRACKED), { force: true });
  });

  it("stages two selected lines of a hunk", async () => {
    const row = await $(`span=${FILE} (Modified)`);
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), row);

    await $(".diff-line-add").waitForExist({ timeout: 10000 });
    const addLines = await $$(".diff-line-add");
    await browser.execute((el) => (el as HTMLElement).click(), addLines[0]);
    await browser.execute(
      (el) => el.dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true })),
      addLines[1],
    );
    const stageLines = await $("button=Stage lines (2)");
    await browser.execute((el) => (el as HTMLElement).click(), stageLines);

    await browser.waitUntil(
      () => execFileSync("git", ["show", `:${FILE}`], { cwd: E2E_REPO_PATH, encoding: "utf8" }) === "a\nb\nB\nc\nd\n",
      { timeout: 10000, timeoutMsg: "expected exactly the two added lines in the index" },
    );
  });

  it("discards an untracked file through the file menu", async () => {
    const actions = await $(`button[aria-label="Actions for ${UNTRACKED}"]`);
    await actions.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), actions);
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Discard changes…"));
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Delete file"));
    await browser.waitUntil(() => !fs.existsSync(path.join(E2E_REPO_PATH, UNTRACKED)), {
      timeout: 10000,
      timeoutMsg: "expected the untracked file to be deleted",
    });
    // waitForExist({ reverse: true }) re-queries until the row is gone; `not.toBeExisting()`
    // can hit a stale element when the row unmounts mid-lookup (#145).
    await $(`span=${UNTRACKED} (New)`).waitForExist({ reverse: true, timeout: 10000 });
  });
});
