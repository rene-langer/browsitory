import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilInPage } from "../support/dom";
import { git, gitWith } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FIRST = "stacked-a.txt";
const SECOND = "stacked-b.txt";

/** A stacked file's section (#793), by its accessible name. */
const section = (file: string, side: "unstaged" | "staged") => `section[aria-label="Changes in ${file} (${side})"]`;

// #793: the working tree's stacked view shows every changed file's diff, each with its own hunk
// actions; the list's List | Stacked choice is remembered.
describe("Browsitory stacked diffs", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FIRST), "one\ntwo\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, SECOND), "three\nfour\n");
    git(E2E_REPO_PATH, "add", FIRST, SECOND);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: seed stacked diff fixtures");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FIRST), "one\ntwo changed\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, SECOND), "three\nfour changed\n");
  });

  after(() => {
    git(E2E_REPO_PATH, "reset", "-q", "--", FIRST, SECOND);
    git(E2E_REPO_PATH, "checkout", "--", FIRST, SECOND);
  });

  it("shows both files' diffs at once and stages a hunk of the one that isn't current", async () => {
    await $(`span=${FIRST} (Modified)`).waitForExist();
    await clickInPage("button", "Stacked");

    await waitUntilInPage(`${section(FIRST, "unstaged")} .diff-line-add`, "two changed", { contains: true });
    await waitUntilInPage(`${section(SECOND, "unstaged")} .diff-line-add`, "four changed", { contains: true });

    await clickInPage(`${section(SECOND, "unstaged")} button`, "Stage hunk");

    await browser.waitUntil(() => git(E2E_REPO_PATH, "diff", "--cached", "--name-only") === SECOND, {
      timeoutMsg: `expected only ${SECOND} to be staged`,
    });
    // The staged change shows in the stack too, the other file still unstaged above it.
    await waitUntilInPage(`${section(SECOND, "staged")} .diff-line-add`, "four changed", { contains: true });
    await waitUntilInPage(`${section(FIRST, "unstaged")} .diff-line-add`, "two changed", { contains: true });
    expect(gitWith(E2E_REPO_PATH, ["diff", "--name-only"])).toBe(FIRST);
  });

  it("keeps the stacked view after a reload", async () => {
    await browser.refresh();
    await waitUntilInPage(`${section(FIRST, "unstaged")} .diff-line-add`, "two changed", { contains: true });
    const stacked = await $('div[aria-label="Diffs shown"] button[aria-pressed="true"]');
    await expect(stacked).toHaveText("Stacked");
  });
});
