import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilGoneInPage } from "../support/dom";
import { git } from "../support/git";
import { expandSidebarSection, sidebarBranchRow } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

// A rebase start refuses tracked local changes (#170), so every case here needs a clean tracked
// tree. Untracked files and submodules don't count, as in the app.
function assertCleanTrackedTree() {
  const dirty = git(E2E_REPO_PATH, "status", "--porcelain", "--untracked-files=no", "--ignore-submodules");
  if (dirty !== "") {
    throw new Error(`rebase.spec.ts needs a clean tracked tree, but the fixture repo has:\n${dirty}`);
  }
}

// A test that fails mid-rebase leaves the fixture repo in a rebase with conflicts, and every
// later test here then fails on that instead of on its own bug (#479). Undo such a leftover with
// the git CLI so one failure stays one failure. Browsitory's own rebase keeps its state in
// `.git/browsitory/rebase.json` (HEAD detached on the rebased commits, a conflicted step as a
// cherry-pick), not in `rebase-merge/`, so `git rebase --abort` does not apply: put the branch
// back on its original tip the way the app's Abort does, then drop the state file.
function abortLeftoverRebase() {
  const stateFile = path.join(E2E_REPO_PATH, ".git", "browsitory", "rebase.json");
  if (!fs.existsSync(stateFile)) return;
  try {
    const state = JSON.parse(fs.readFileSync(stateFile, "utf8")) as {
      originalBranchRef: string;
      originalTip: string;
    };
    const run = (...args: string[]) => git(E2E_REPO_PATH, ...args);
    run("reset", "--hard", "--quiet");
    if (state.originalBranchRef.startsWith("refs/heads/")) {
      run("checkout", "--force", "--quiet", state.originalBranchRef.slice("refs/heads/".length));
    }
    run("reset", "--hard", "--quiet", state.originalTip);
    fs.rmSync(stateFile);
  } catch (err) {
    console.warn(`[e2e] rebase.spec.ts: could not undo the leftover rebase: ${String(err)}`);
  }
}

describe("Browsitory interactive rebase", () => {
  afterEach(() => {
    abortLeftoverRebase();
  });

  before(() => {
    assertCleanTrackedTree();
    const baseBranch = git(E2E_REPO_PATH, "rev-parse", "--abbrev-ref", "HEAD");

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-base.txt"), "v1\n");
    git(E2E_REPO_PATH, "add", "rebase-base.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: rebase onto-point commit");

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-a.txt"), "a\n");
    git(E2E_REPO_PATH, "add", "rebase-a.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: rebase commit a");

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-b.txt"), "b\n");
    git(E2E_REPO_PATH, "add", "rebase-b.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: rebase commit b (to drop)");

    fs.writeFileSync(path.join(E2E_REPO_PATH, "rebase-c.txt"), "c\n");
    git(E2E_REPO_PATH, "add", "rebase-c.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: rebase commit c (squash target)");

    void baseBranch;
  });

  it("opens the planner, drops a commit, squashes another with a custom message, and finishes", async () => {
    // Prior specs (running before this one in file order) leave the app's in-memory commit
    // list stale relative to what `before()` above just committed via direct `git` calls —
    // same staleness `merge.spec.ts`'s add/delete test hits, fixed the same way: a full
    // reload re-runs `App`'s mount-time `openRepo()` against current on-disk state.
    await browser.refresh();

    const commitEntry = await $("li*=e2e: rebase onto-point commit");
    await commitEntry.waitForExist();
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
    await rebaseButton.waitForExist();
    await rebaseButton.click();

    // Mark "rebase commit b (to drop)" as Drop.
    const dropRowSelect = await $(
      "//li[contains(., 'rebase commit b (to drop)')]//select[starts-with(@aria-label, 'Action for ')]",
    );
    await dropRowSelect.waitForExist();
    await dropRowSelect.selectByVisibleText("Drop");

    // Mark "rebase commit c (squash target)" as Squash.
    const squashRowSelect = await $(
      "//li[contains(., 'rebase commit c (squash target)')]//select[starts-with(@aria-label, 'Action for ')]",
    );
    await squashRowSelect.selectByVisibleText("Squash");

    const combinedMessageField = await $("[aria-label='Combined message']");
    await combinedMessageField.waitForExist();
    await combinedMessageField.setValue("e2e: combined rebase commit");

    const startButton = await $("button=Start rebase");
    // Start waits for the group's full messages (#721).
    await startButton.waitForEnabled();
    await startButton.click();

    await browser.waitUntil(async () => (await $("li*=e2e: combined rebase commit")).isExisting());
    const commitGraphAfter = await $("li*=e2e: combined rebase commit");
    await waitUntilGoneInPage("li", "rebase commit b (to drop)", { contains: true });
  });

  // Sets up a rebase that pauses on a real conflict: `onto` introduces a file, one commit edits
  // a line, a second commit edits the same line again. Dropping the middle commit forces the
  // final commit's patch to apply against `onto`'s original content instead of the edit it was
  // actually written on top of — same construction as `git-core::rebase`'s
  // `a_conflicting_pick_pauses_and_resolving_then_continuing_lands_it` test, driven through the
  // UI instead of the API.
  function writeConflictCommit(fileName: string, content: string, message: string) {
    fs.writeFileSync(path.join(E2E_REPO_PATH, fileName), content);
    git(E2E_REPO_PATH, "add", fileName);
    git(E2E_REPO_PATH, "commit", "-m", message);
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
    await ontoEntry.waitForExist();
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, ontoEntry);

    const rebaseButton = await $("button*=Rebase onto here");
    await rebaseButton.waitForExist();
    await rebaseButton.click();

    // Drop "change on top" so replaying "conflicting change" lands on `onto`'s original content
    // instead of the edit it actually followed — the same file, same context lines, real conflict.
    const dropRowSelect = await $(
      "//li[contains(., 'e2e: conflict-resume change on top')]//select[starts-with(@aria-label, 'Action for ')]",
    );
    await dropRowSelect.waitForExist();
    await dropRowSelect.selectByVisibleText("Drop");

    const startButton = await $("button=Start rebase");
    await startButton.click();

    const rebasePanel = await $("h2*=Rebase in progress");
    await rebasePanel.waitForExist();

    const conflictedRow = await $("span=rebase-conflict-resume.txt (Conflicted)");
    await conflictedRow.waitForExist();
    await conflictedRow.scrollIntoView({ block: "center" });
    await browser.execute((el) => (el as HTMLElement).click(), conflictedRow);

    const acceptTheirs = await $("button=Accept theirs");
    await acceptTheirs.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), acceptTheirs);
    const saveResolution = await $("button=Save resolution");
    await browser.execute((el) => (el as HTMLElement).click(), saveResolution);

    const continueButton = await $("button=Continue rebase");
    await continueButton.waitForEnabled();
    await browser.execute((el) => (el as HTMLElement).click(), continueButton);

    await waitUntilGoneInPage("h2", "Rebase in progress", { contains: true });
    const landedEntry = await $("li*=e2e: conflict-resume conflicting change");
    await landedEntry.waitForExist();
    await waitUntilGoneInPage("li", "e2e: conflict-resume change on top", { contains: true });

    const headMessage = git(E2E_REPO_PATH, "log", "-1", "--format=%s");
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

    const headBeforeRebase = git(E2E_REPO_PATH, "rev-parse", "HEAD");

    await browser.refresh();

    const ontoEntry = await $("li*=e2e: conflict-abort onto point");
    await ontoEntry.waitForExist();
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, ontoEntry);

    const rebaseButton = await $("button*=Rebase onto here");
    await rebaseButton.waitForExist();
    await rebaseButton.click();

    const dropRowSelect = await $(
      "//li[contains(., 'e2e: conflict-abort change on top')]//select[starts-with(@aria-label, 'Action for ')]",
    );
    await dropRowSelect.waitForExist();
    await dropRowSelect.selectByVisibleText("Drop");

    const startButton = await $("button=Start rebase");
    await startButton.click();

    const rebasePanel = await $("h2*=Rebase in progress");
    await rebasePanel.waitForExist();
    const conflictedRow = await $("span=rebase-conflict-abort.txt (Conflicted)");
    await conflictedRow.waitForExist();

    // The operation status strip has an "Abort rebase" button too; this is the panel's. Clicked
    // in the page, like Continue above: a native WebDriver click sent right after the conflict
    // row appeared (while the panel was still laying out) left the rebase running in CI (#479).
    await clickInPage("section[aria-label='Rebase in progress'] button", "Abort rebase");

    await waitUntilGoneInPage("h2", "Rebase in progress", { contains: true });

    const headAfterAbort = git(E2E_REPO_PATH, "rev-parse", "HEAD");
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
    await stillOnTopEntry.waitForExist();
  });

  it("refuses to start a rebase over uncommitted changes and keeps them", async () => {
    assertCleanTrackedTree();
    writeConflictCommit("dirty-onto.txt", "onto\n", "e2e: dirty onto point");
    writeConflictCommit("dirty-tracked.txt", "committed\n", "e2e: dirty tracked file");
    const headBefore = git(E2E_REPO_PATH, "rev-parse", "HEAD");
    const dirtyFile = path.join(E2E_REPO_PATH, "dirty-tracked.txt");
    fs.writeFileSync(dirtyFile, "my uncommitted edit\n");

    try {
      await browser.refresh();
      const ontoEntry = await $("li*=e2e: dirty onto point");
      await ontoEntry.waitForExist();
      await browser.execute((el) => {
        el.dispatchEvent(
          new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
        );
      }, ontoEntry);
      const rebaseButton = await $("button*=Rebase onto here");
      await rebaseButton.waitForExist();
      await rebaseButton.click();
      await $("//li[contains(., 'e2e: dirty tracked file')]//select[starts-with(@aria-label, 'Action for ')]").waitForExist();

      await (await $("button=Start rebase")).click();

      const alert = await $("//*[@role='alert'][contains(., 'You have uncommitted changes to')]");
      await alert.waitForExist();
      const alertText = await browser.execute((el) => (el as HTMLElement).textContent ?? "", alert);
      expect(alertText).toContain(
        "You have uncommitted changes to dirty-tracked.txt. Commit or stash them before starting a rebase.",
      );
      // The planner stays open, nothing moved, and the edit is intact.
      await expect($("button=Start rebase")).toBeExisting();
      expect(fs.readFileSync(dirtyFile, "utf8")).toBe("my uncommitted edit\n");
      const headAfter = git(E2E_REPO_PATH, "rev-parse", "HEAD");
      expect(headAfter).toBe(headBefore);
      expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git/browsitory/rebase.json"))).toBe(false);

      await (await $("button=Cancel")).click();
      await waitUntilGoneInPage("button", "Start rebase");
    } finally {
      git(E2E_REPO_PATH, "checkout", "--", "dirty-tracked.txt");
    }
  });

  it("ctrl-selects two non-adjacent commits, drops them through the planner, and keeps the rest", async () => {
    writeConflictCommit("multi-1.txt", "1\n", "e2e: multi 1 (drop)");
    writeConflictCommit("multi-2.txt", "2\n", "e2e: multi 2 (keep)");
    writeConflictCommit("multi-3.txt", "3\n", "e2e: multi 3 (drop)");
    writeConflictCommit("multi-4.txt", "4\n", "e2e: multi 4 (keep)");

    await browser.refresh();

    const first = await $("li*=e2e: multi 1 (drop)");
    await first.waitForExist();
    await first.click();

    // A synthetic `click` carrying `ctrlKey` instead of a real modifier key press: a held
    // modifier driven through WebKitGTK/tauri-driver leaves Shift/Ctrl state stuck for the rest
    // of the session (see the context-menu workaround in the first test). `CommitGraph` only
    // reads `ctrlKey` off the click event.
    const third = await $("li*=e2e: multi 3 (drop)");
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
    }, third);

    const dropButton = await $("button=Drop 2 commits");
    await dropButton.waitForExist();
    await dropButton.click();

    // The planner lists the whole span; only the two selected commits are pre-marked Drop.
    const dropRow1 = await $("//li[contains(., 'multi 1 (drop)')]//select[starts-with(@aria-label, 'Action for ')]");
    await dropRow1.waitForExist();
    await expect(dropRow1).toHaveValue("Drop");
    const keepRow = await $("//li[contains(., 'multi 2 (keep)')]//select[starts-with(@aria-label, 'Action for ')]");
    await expect(keepRow).toHaveValue("Pick");
    const dropRow3 = await $("//li[contains(., 'multi 3 (drop)')]//select[starts-with(@aria-label, 'Action for ')]");
    await expect(dropRow3).toHaveValue("Drop");

    await $("button=Start rebase").click();

    await waitUntilGoneInPage("li", "e2e: multi 1 (drop)", { contains: true });
    await waitUntilGoneInPage("li", "e2e: multi 3 (drop)", { contains: true });
    await expect($("li*=e2e: multi 2 (keep)")).toBeExisting();
    await expect($("li*=e2e: multi 4 (keep)")).toBeExisting();
  });
  // #619: "Rebase current branch onto <name>…" in the branch menu opens the planner on that
  // branch, named in its title, and the rebase lands the branch's commits on top of it.
  it("rebases the current branch onto another branch from the branch menu", async () => {
    assertCleanTrackedTree();
    const baseBranch = git(E2E_REPO_PATH, "rev-parse", "--abbrev-ref", "HEAD");
    git(E2E_REPO_PATH, "checkout", "-b", "e2e-rebase-onto-target");
    writeConflictCommit("rebase-onto-target.txt", "target\n", "e2e: rebase-onto target commit");
    git(E2E_REPO_PATH, "checkout", baseBranch);
    writeConflictCommit("rebase-onto-mine.txt", "mine\n", "e2e: rebase-onto my commit");

    await browser.refresh();
    await expandSidebarSection("Branches");
    const branchRow = await $(sidebarBranchRow("e2e-rebase-onto-target"));
    await branchRow.waitForExist();
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
    }, branchRow);

    const rebaseItem = await $("button=Rebase current branch onto e2e-rebase-onto-target…");
    await rebaseItem.waitForExist();
    await rebaseItem.click();

    await $("h2=Rebase onto e2e-rebase-onto-target").waitForExist();
    const row = await $("//li[contains(., 'e2e: rebase-onto my commit')]//select[starts-with(@aria-label, 'Action for ')]");
    await row.waitForExist();
    const startButton = await $("button=Start rebase");
    await startButton.waitForEnabled();
    await startButton.click();
    await waitUntilGoneInPage("button", "Start rebase");

    await browser.waitUntil(
      () => git(E2E_REPO_PATH, "rev-parse", "HEAD~1") === git(E2E_REPO_PATH, "rev-parse", "e2e-rebase-onto-target"),
      { timeoutMsg: "the current branch was not rebased onto e2e-rebase-onto-target" },
    );
    expect(git(E2E_REPO_PATH, "log", "-1", "--format=%s")).toBe("e2e: rebase-onto my commit");
  });
});
