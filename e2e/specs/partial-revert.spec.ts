import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { waitForAppReady } from "../support/app";
import { git } from "../support/git";
import { UI_TIMEOUT } from "../support/timeouts";

// Reverting one hunk of an older commit into the working tree, never as a commit (#448).
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "partial-revert-e2e.txt";
const SUMMARY = "e2e: partial revert fixture";

const lines = (changed: Record<number, string>) =>
  Array.from({ length: 20 }, (_, i) => `${changed[i] ?? `line ${i}`}\n`).join("");
const BASE = lines({});
const CHANGED = lines({ 2: "LINE TWO", 17: "LINE SEVENTEEN" });

function fileContents(): string {
  return fs.readFileSync(path.join(E2E_REPO_PATH, FILE), "utf8");
}

describe("Browsitory partial revert", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), BASE);
    git(E2E_REPO_PATH, "add", FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: partial revert base");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), CHANGED);
    git(E2E_REPO_PATH, "add", FILE);
    git(E2E_REPO_PATH, "commit", "-m", SUMMARY);
  });

  after(() => {
    // Leave the shared fixture repo clean for the next spec.
    git(E2E_REPO_PATH, "checkout", "--", FILE);
  });

  it("reverts one hunk of a commit as an unstaged change and commits nothing", async () => {
    await waitForAppReady();
    const head = git(E2E_REPO_PATH, "rev-parse", "HEAD").trim();

    const entry = await $(`li*=${SUMMARY}`);
    await entry.waitForExist({ timeout: UI_TIMEOUT });
    await browser.execute((el) => (el as HTMLElement).click(), entry);

    // The commit's two hunks each have a header button; the first is line 2's.
    await browser.waitUntil(async () => (await (await $$("button=Revert hunk")).length) === 2, {
      timeout: UI_TIMEOUT,
      timeoutMsg: "expected the commit's two hunks with Revert hunk",
    });
    const [revertHunk] = await $$("button=Revert hunk");
    await browser.execute((el) => (el as HTMLElement).click(), revertHunk);

    await browser.waitUntil(() => fileContents() === lines({ 17: "LINE SEVENTEEN" }), {
      timeout: UI_TIMEOUT,
      timeoutMsg: "the first hunk was not reverted in the working tree",
    });
    expect(git(E2E_REPO_PATH, "rev-parse", "HEAD").trim()).toBe(head);
    expect(git(E2E_REPO_PATH, "diff", "--cached", "--name-only").trim()).toBe("");
    await $(`button[aria-label="Undo Revert hunk in ${FILE} from ${head.slice(0, 7)}"]`).waitForExist({
      timeout: UI_TIMEOUT,
    });
  });
});
