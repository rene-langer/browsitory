import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const HUNK_FIXTURE_FILE = "hunk-fixture.txt";

describe("Browsitory hunk staging", () => {
  before(() => {
    const filePath = path.join(E2E_REPO_PATH, HUNK_FIXTURE_FILE);
    const original = Array.from({ length: 15 }, (_, i) => `line ${i + 1}`).join("\n") + "\n";
    fs.writeFileSync(filePath, original);
    execFileSync("git", ["add", HUNK_FIXTURE_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: seed hunk fixture file"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    const lines = original.split("\n");
    lines[1] = "line 2 changed";
    lines[13] = "line 14 changed";
    fs.writeFileSync(filePath, lines.join("\n"));
  });

  after(() => {
    // Leaves two new commits in the shared fixture repo's history (the seed commit plus this
    // spec's "stage one hunk" commit) — same tradeoff `stash-management.spec.ts`'s seed commit
    // already makes; nothing in this suite asserts an exact commit count. All that matters for
    // the next spec (alphabetically after "hunk-") is a clean working tree, so just check the
    // fixture file back out to match the latest commit.
    execFileSync("git", ["checkout", "--", HUNK_FIXTURE_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  });

  it("stages a single hunk, commits it, and leaves the other hunk's edit unstaged", async () => {
    const fileRow = await $(`li*=${HUNK_FIXTURE_FILE}`);
    await fileRow.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), await $(`button=${HUNK_FIXTURE_FILE} (Modified)`));

    const stageHunkButtons = await $$("button=Stage Hunk");
    await expect(stageHunkButtons).toBeElementsArrayOfSize(2);
    await browser.execute((el) => (el as HTMLElement).click(), stageHunkButtons[0]);

    await browser.waitUntil(
      async () => (await $$("button=Stage Hunk")).length === 1,
      { timeout: 10000, timeoutMsg: "expected only one unstaged hunk to remain after staging the other" },
    );

    const commitMessageBox = await $("textarea");
    await commitMessageBox.setValue("stage one hunk");
    const commitButton = await $("button=Commit");
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    await browser.waitUntil(
      async () => {
        const content = fs.readFileSync(path.join(E2E_REPO_PATH, HUNK_FIXTURE_FILE), "utf8");
        return content.includes("line 14 changed");
      },
      { timeout: 10000, timeoutMsg: "expected the file to still have the unstaged hunk's dirty content" },
    );

    const committedShow = execFileSync("git", ["show", "HEAD:" + HUNK_FIXTURE_FILE], {
      cwd: E2E_REPO_PATH,
      encoding: "utf8",
    });
    expect(committedShow).toContain("line 2 changed");
    expect(committedShow).not.toContain("line 14 changed");
  });
});
