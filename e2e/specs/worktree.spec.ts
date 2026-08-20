import { execFileSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const WORKTREE_NAME = "feature-tree";
const WORKTREE_BRANCH = "feature/e2e-worktree";
const WORKTREE_PATH = path.join(os.tmpdir(), `browsitory-e2e-${Date.now()}`);

describe("Browsitory worktrees", () => {
  before(() => {
    execFileSync(
      "git",
      ["commit", "--allow-empty", "-m", "e2e: seed commit for worktree"],
      { cwd: E2E_REPO_PATH, stdio: "inherit" },
    );
  });

  it("creates, opens, returns from, and removes a linked worktree", async () => {
    // "Worktrees" holds the create form; "Branches" holds the switcher this test reads text
    // from throughout. Both default closed.
    await expandSidebarSection("Worktrees");
    await expandSidebarSection("Branches");

    const createForm = await $("aria/Create worktree");
    await createForm.waitForExist({ timeout: 10000 });
    const branchSwitcher = await $("aria/Branch switcher");
    const mainBranch = await branchSwitcher.getText();
    const nameInput = await $("aria/Worktree name");
    await nameInput.setValue(WORKTREE_NAME);
    await (await $("aria/Worktree path")).setValue(WORKTREE_PATH);
    await (await $("aria/Branch")).setValue(WORKTREE_BRANCH);
    await (await $("aria/Start point")).setValue("HEAD");
    await (await createForm.$("button=Create worktree")).click();
    await browser.waitUntil(async () => (await nameInput.getValue()) === "", { timeout: 10000, timeoutMsg: "expected worktree creation to complete" });

    const openLinkedWorktree = await $(`button=Open ${WORKTREE_PATH}`);
    await openLinkedWorktree.waitForExist({ timeout: 10000 });
    await expect(openLinkedWorktree).toBeExisting();

    await openLinkedWorktree.click();
    const openMainWorktree = await $(`button=Open ${E2E_REPO_PATH}`);
    await expect(branchSwitcher).toHaveText(WORKTREE_BRANCH);
    await openMainWorktree.waitForExist({ timeout: 10000 });

    await openMainWorktree.click();
    const removeLinkedWorktree = await $(`button=Remove ${WORKTREE_PATH}`);
    await expect(branchSwitcher).toHaveText(mainBranch);
    await removeLinkedWorktree.waitForExist({ timeout: 10000 });

    await removeLinkedWorktree.click();
    const removalDialog = await $("aria/Remove worktree " + WORKTREE_PATH);
    await removalDialog.waitForExist({ timeout: 10000 });
    await (await removalDialog.$("button=Remove worktree")).click();
    await browser.waitUntil(
      async () => (await (await browser.$$("aria/" + WORKTREE_PATH)).length) === 0,
      { timeout: 10000, timeoutMsg: "expected the linked worktree to be removed" },
    );
  });
});
