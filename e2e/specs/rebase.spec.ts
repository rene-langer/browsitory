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

    const commitGraphAfter = await $("li*=e2e: combined rebase commit");
    await commitGraphAfter.waitForExist({ timeout: 10000 });
    const droppedEntry = await $("li*=rebase commit b (to drop)");
    await expect(droppedEntry).not.toBeExisting();
  });
});
