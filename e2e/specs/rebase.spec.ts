import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

describe("Browsitory interactive rebase", () => {
  before(() => {
    const baseBranch = execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-base.txt"), "v1\n");
    execFileSync("git", ["add", "rebase-base.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: rebase onto-point commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-a.txt"), "a\n");
    execFileSync("git", ["add", "rebase-a.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: rebase commit a"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-b.txt"), "b\n");
    execFileSync("git", ["add", "rebase-b.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: rebase commit b (to drop)"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-c.txt"), "c\n");
    execFileSync("git", ["add", "rebase-c.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: rebase commit c (squash target)"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    void baseBranch;
  });

  it("opens the planner, drops a commit, squashes another with a custom message, and finishes", async () => {
    // Prior specs (running before this one in file order) leave the app's in-memory commit
    // list stale relative to what `before()` above just committed via direct `git` calls —
    // same staleness `merge.spec.ts`'s add/delete test hits, fixed the same way: a full
    // reload re-runs `App`'s mount-time `openRepo()` against current on-disk state.
    await browser.refresh();

    const commitEntry = await $("li*=e2e: rebase onto-point commit");
    await commitEntry.waitForExist({ timeout: 10000 });
    // Trigger the row's context menu via a synthetic DOM `contextmenu` event rather than
    // WebdriverIO's `.click({ button: "right" })`. The latter drives a real secondary-button
    // pointer action through the WebKitGTK/tauri-driver automation stack, and in this
    // environment that leaves some modifier/button state stuck afterwards: every
    // Shift-modified character typed anywhere in the session for the rest of the test
    // (regardless of which field, and even with no further `<select>` interaction in between)
    // silently loses its Shift level — reproduced in isolation with a colon typed into the
    // unrelated commit-message box right after just the right-click, no rebase UI involved.
    // `CommitGraph`'s `handleContextMenu` only cares about the `contextmenu` DOM event, not
    // how it was produced, so dispatching it directly sidesteps the driver bug entirely while
    // still exercising the exact same app code path.
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", {
          bubbles: true,
          cancelable: true,
          clientX: 50,
          clientY: 50,
        }),
      );
    }, commitEntry);

    const rebaseButton = await $("button*=Rebase onto here");
    await rebaseButton.waitForExist({ timeout: 10000 });
    await rebaseButton.click();

    // Mark "rebase commit b (to drop)" as Drop.
    const dropRowSelect = await $(
      "//li[contains(., 'rebase commit b (to drop)')]//select[@aria-label='Action']",
    );
    await dropRowSelect.waitForExist({ timeout: 10000 });
    await dropRowSelect.selectByVisibleText("Drop");

    // Mark "rebase commit c (squash target)" as Squash.
    const squashRowSelect = await $(
      "//li[contains(., 'rebase commit c (squash target)')]//select[@aria-label='Action']",
    );
    await squashRowSelect.selectByVisibleText("Squash");

    const combinedMessageField = await $("[aria-label='Combined message']");
    await combinedMessageField.waitForExist({ timeout: 10000 });
    await combinedMessageField.setValue("e2e: combined rebase commit");

    const startButton = await $("button=Start Rebase");
    await startButton.click();

    await browser.waitUntil(async () => (await $("li*=e2e: combined rebase commit")).isExisting(), { timeout: 10000 });
    const commitGraphAfter = await $("li*=e2e: combined rebase commit");
    const droppedEntry = await $("li*=rebase commit b (to drop)");
    await expect(droppedEntry).not.toBeExisting();
  });

  // Sets up a rebase that pauses on a real conflict: `onto` introduces a file, one commit edits
  // a line, a second commit edits the same line again. Dropping the middle commit forces the
  // final commit's patch to apply against `onto`'s original content instead of the edit it was
  // actually written on top of — same construction as `git-core::rebase`'s
  // `a_conflicting_pick_pauses_and_resolving_then_continuing_lands_it` test, driven through the
  // UI instead of the API.
  function writeConflictCommit(fileName: string, content: string, message: string) {
    fs.writeFileSync(path.join(E2E_REPO_PATH, fileName), content);
    execFileSync("git", ["add", fileName], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", message], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  }

  it("pauses on a rebase conflict, resolves it, and continues to completion", async () => {
    writeConflictCommit(
      "rebase-conflict-resume.txt",
      "line one\nline two\n",
      "e2e: conflict-resume onto point",
    );
    writeConflictCommit(
      "rebase-conflict-resume.txt",
      "line one\nchanged on top\n",
      "e2e: conflict-resume change on top",
    );
    writeConflictCommit(
      "rebase-conflict-resume.txt",
      "line one\nchanged again\n",
      "e2e: conflict-resume conflicting change",
    );

    await browser.refresh();

    const ontoEntry = await $("li*=e2e: conflict-resume onto point");
    await ontoEntry.waitForExist({ timeout: 10000 });
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, ontoEntry);

    const rebaseButton = await $("button*=Rebase onto here");
    await rebaseButton.waitForExist({ timeout: 10000 });
    await rebaseButton.click();

    // Drop "change on top" so replaying "conflicting change" lands on `onto`'s original content
    // instead of the edit it actually followed — the same file, same context lines, real conflict.
    const dropRowSelect = await $(
      "//li[contains(., 'e2e: conflict-resume change on top')]//select[@aria-label='Action']",
    );
    await dropRowSelect.waitForExist({ timeout: 10000 });
    await dropRowSelect.selectByVisibleText("Drop");

    const startButton = await $("button=Start Rebase");
    await startButton.click();

    const rebasePanel = await $("h2*=Rebase in progress");
    await rebasePanel.waitForExist({ timeout: 10000 });

    const conflictedRow = await $("span*=rebase-conflict-resume.txt (Conflicted)");
    await conflictedRow.waitForExist({ timeout: 10000 });
    await conflictedRow.scrollIntoView({ block: "center" });
    await browser.execute((el) => (el as HTMLElement).click(), conflictedRow);

    const acceptTheirs = await $("button=Accept Theirs");
    await acceptTheirs.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), acceptTheirs);
    const saveResolution = await $("button=Save resolution");
    await browser.execute((el) => (el as HTMLElement).click(), saveResolution);

    const continueButton = await $("button=Continue Rebase");
    await continueButton.waitForEnabled({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), continueButton);

    await browser.waitUntil(
      async () => !(await $("h2*=Rebase in progress").isExisting()),
      { timeout: 10000 },
    );
    const landedEntry = await $("li*=e2e: conflict-resume conflicting change");
    await landedEntry.waitForExist({ timeout: 10000 });
    const droppedStillGone = await $("li*=e2e: conflict-resume change on top");
    await expect(droppedStillGone).not.toBeExisting();

    const headMessage = execFileSync("git", ["log", "-1", "--format=%s"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();
    expect(headMessage).toBe("e2e: conflict-resume conflicting change");
  });

  it("aborts a rebase mid-conflict and restores the pre-rebase state", async () => {
    writeConflictCommit(
      "rebase-conflict-abort.txt",
      "line one\nline two\n",
      "e2e: conflict-abort onto point",
    );
    writeConflictCommit(
      "rebase-conflict-abort.txt",
      "line one\nchanged on top\n",
      "e2e: conflict-abort change on top",
    );
    writeConflictCommit(
      "rebase-conflict-abort.txt",
      "line one\nchanged again\n",
      "e2e: conflict-abort conflicting change",
    );

    const headBeforeRebase = execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();

    await browser.refresh();

    const ontoEntry = await $("li*=e2e: conflict-abort onto point");
    await ontoEntry.waitForExist({ timeout: 10000 });
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, ontoEntry);

    const rebaseButton = await $("button*=Rebase onto here");
    await rebaseButton.waitForExist({ timeout: 10000 });
    await rebaseButton.click();

    const dropRowSelect = await $(
      "//li[contains(., 'e2e: conflict-abort change on top')]//select[@aria-label='Action']",
    );
    await dropRowSelect.waitForExist({ timeout: 10000 });
    await dropRowSelect.selectByVisibleText("Drop");

    const startButton = await $("button=Start Rebase");
    await startButton.click();

    const rebasePanel = await $("h2*=Rebase in progress");
    await rebasePanel.waitForExist({ timeout: 10000 });
    const conflictedRow = await $("span*=rebase-conflict-abort.txt (Conflicted)");
    await conflictedRow.waitForExist({ timeout: 10000 });

    const abortButton = await $("button=Abort Rebase");
    await abortButton.click();

    await browser.waitUntil(
      async () => !(await $("h2*=Rebase in progress").isExisting()),
      { timeout: 10000 },
    );

    const headAfterAbort = execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();
    expect(headAfterAbort).toBe(headBeforeRebase);

    const fileContents = fs.readFileSync(
      path.join(E2E_REPO_PATH, "rebase-conflict-abort.txt"),
      "utf8",
    );
    expect(fileContents).toBe("line one\nchanged again\n");

    for (const dir of ["rebase-merge", "rebase-apply"]) {
      expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git", dir))).toBe(false);
    }

    const stillOnTopEntry = await $("li*=e2e: conflict-abort conflicting change");
    await stillOnTopEntry.waitForExist({ timeout: 10000 });
  });
});
