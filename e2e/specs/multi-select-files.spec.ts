import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { git } from "../support/git";
import { UI_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILES = ["multi-select-a.txt", "multi-select-b.txt", "multi-select-c.txt"];

// Selecting several changed files and staging them with one action (#449).
describe("Browsitory multi-select in the change lists", () => {
  before(() => {
    for (const file of FILES) fs.writeFileSync(path.join(E2E_REPO_PATH, file), `${file}\n`);
  });

  after(() => {
    // Leave the shared fixture repo clean for the next spec: nothing staged, no new files.
    git(E2E_REPO_PATH, "reset", "-q", "--", ...FILES);
    for (const file of FILES) fs.rmSync(path.join(E2E_REPO_PATH, file), { force: true });
  });

  it("Ctrl+click selects two files and the selection bar stages both", async () => {
    const first = await $(`span=${FILES[0]} (New)`);
    await first.waitForExist({ timeout: UI_TIMEOUT });
    // The row's `{path} ({kind})` text is a `<span>` inside `<li role="option">`; the click
    // bubbles to the row, as in `hunk-staging.spec.ts`.
    await browser.execute((el) => (el as HTMLElement).click(), first);
    await browser.execute(
      (el) => (el as HTMLElement).dispatchEvent(new MouseEvent("click", { bubbles: true, ctrlKey: true })),
      await $(`span=${FILES[2]} (New)`),
    );

    const stageSelected = await $("button=Stage 2 files");
    await stageSelected.waitForExist({ timeout: UI_TIMEOUT });
    await browser.execute((el) => (el as HTMLElement).click(), stageSelected);

    await browser.waitUntil(
      () => {
        const staged = git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n");
        return staged.includes(FILES[0]) && staged.includes(FILES[2]);
      },
      { timeout: UI_TIMEOUT, timeoutMsg: "the two selected files were not staged" },
    );
    expect(git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n")).not.toContain(FILES[1]);
  });
});

const TRACKED = "stage-modified-tracked.txt";
const UNTRACKED = "stage-modified-new.txt";

// "Stage modified" stages the tracked files' changes and leaves untracked files alone (#621).
describe("Browsitory Stage modified in the Changes heading", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRACKED), "committed\n");
    git(E2E_REPO_PATH, "add", TRACKED);
    git(E2E_REPO_PATH, "commit", "-q", "-m", "e2e: stage modified fixture");
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRACKED), "edited\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, UNTRACKED), "new\n");
  });

  after(() => {
    git(E2E_REPO_PATH, "reset", "-q", "--", TRACKED, UNTRACKED);
    git(E2E_REPO_PATH, "checkout", "--", TRACKED);
    fs.rmSync(path.join(E2E_REPO_PATH, UNTRACKED), { force: true });
  });

  it("stages the modified tracked file and not the untracked one", async () => {
    await $(`span=${UNTRACKED} (New)`).waitForExist({ timeout: UI_TIMEOUT });
    const stageModified = await $("button=Stage modified");
    await stageModified.waitForExist({ timeout: UI_TIMEOUT });
    await browser.execute((el) => (el as HTMLElement).click(), stageModified);

    await browser.waitUntil(
      () => git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n").includes(TRACKED),
      { timeout: UI_TIMEOUT, timeoutMsg: "the modified tracked file was not staged" },
    );
    expect(git(E2E_REPO_PATH, "diff", "--cached", "--name-only").split("\n")).not.toContain(UNTRACKED);
    expect(git(E2E_REPO_PATH, "status", "--porcelain", "--", UNTRACKED).trim()).toBe(`?? ${UNTRACKED}`);
  });
});
