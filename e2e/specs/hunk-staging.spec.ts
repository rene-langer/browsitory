import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { git } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const HUNK_FIXTURE_FILE = "hunk-fixture.txt";

describe("Browsitory hunk staging", () => {
  before(() => {
    const filePath = path.join(E2E_REPO_PATH, HUNK_FIXTURE_FILE);
    const original = Array.from({ length: 15 }, (_, i) => `line ${i + 1}`).join("\n") + "\n";
    fs.writeFileSync(filePath, original);
    git(E2E_REPO_PATH, "add", HUNK_FIXTURE_FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: seed hunk fixture file");
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
    git(E2E_REPO_PATH, "checkout", "--", HUNK_FIXTURE_FILE);
  });

  it("stages a single hunk, commits it, and leaves the other hunk's edit unstaged", async () => {
    const fileRow = await $(`li*=${HUNK_FIXTURE_FILE}`);
    await fileRow.waitForExist();
    // The row's `{path} ({kind})` text lives in a `<span>` inside `<li role="option">` (see
    // `DiffPane.tsx`'s `FileListRow`), not in a `<button>`; the click bubbles to the `<li>`.
    await browser.execute((el) => (el as HTMLElement).click(), await $(`span=${HUNK_FIXTURE_FILE} (Modified)`));

    const stageHunkButtons = await $$("button=Stage hunk");
    await expect(stageHunkButtons).toBeElementsArrayOfSize(2);
    await browser.execute((el) => (el as HTMLElement).click(), stageHunkButtons[0]);

    await browser.waitUntil(
      async () => (await (await $$("button=Stage hunk")).length) === 1,
      { timeoutMsg: "expected only one unstaged hunk to remain after staging the other" },
    );

    const commitMessageBox = await $("textarea");
    await commitMessageBox.setValue("stage one hunk");
    const commitButton = await $("button=Commit");
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    // Wait for the commit itself. The working file has "line 14 changed" before and after the
    // commit, so it says nothing about whether the commit landed: reading HEAD right after it
    // could still see the seed commit, without "line 2 changed" (#182).
    await browser.waitUntil(
      () =>
        git(E2E_REPO_PATH, "log", "-1", "--format=%s") ===
        "stage one hunk",
      { timeoutMsg: "the commit of the staged hunk did not land" },
    );
    expect(fs.readFileSync(path.join(E2E_REPO_PATH, HUNK_FIXTURE_FILE), "utf8")).toContain("line 14 changed");

    const committedShow = git(E2E_REPO_PATH, "show", "HEAD:" + HUNK_FIXTURE_FILE);
    expect(committedShow).toContain("line 2 changed");
    expect(committedShow).not.toContain("line 14 changed");
  });
});
