import { execFileSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const WORKTREE_NAME = "feature-tree";
const WORKTREE_BRANCH = "feature/e2e-worktree";
const WORKTREE_PATH = path.join(os.tmpdir(), `browsitory-e2e-${Date.now()}`);

// Opening a worktree opens it as its own tab (`WorktreePanel`'s `onOpenWorktree` routes through
// `App.tsx`'s tab-opening path) rather than switching the current workspace's branch in place —
// and every open tab's `RepoWorkspace` stays mounted (toggling `display: none`/`display:
// contents` for snappy switching, never unmounting), so once a second tab is open, BOTH tabs'
// current-branch rows in their respective `BranchTree`s exist simultaneously, and both tabs'
// `WorktreePanel`s render identical "Open <path>"/"Remove <path>" buttons for the *same*
// underlying worktree list (it's shared by the underlying repository, not scoped per tab — see
// `WorktreePanel.tsx`, which never excludes the tab's own current worktree). A plain
// `$(selector)` — or a reference captured before the second tab existed — resolves to whichever
// match is first in the DOM, which can be the wrong (hidden, non-interactable) tab's copy. This
// polls for, and returns, the one match that's actually displayed.
async function activeElement(selector: string, timeout = 10000) {
  await browser.waitUntil(
    async () => {
      const candidates = await $$(selector);
      for (const candidate of candidates) {
        if (await candidate.isDisplayed()) return true;
      }
      return false;
    },
    { timeout, timeoutMsg: `expected a visible match for ${selector}` },
  );
  const candidates = await $$(selector);
  for (const candidate of candidates) {
    if (await candidate.isDisplayed()) return candidate;
  }
  throw new Error(`unreachable: activeElement's own waitUntil already confirmed a visible match for ${selector}`);
}

// BranchTree has no separate switcher control — the current branch is just the row in the
// Local folder whose button text carries the " (current)" suffix (`BranchTree.tsx`).
function activeCurrentBranchButton() {
  return activeElement("//button[contains(., ' (current)')]");
}

describe("Browsitory worktrees", () => {
  before(() => {
    execFileSync(
      "git",
      ["commit", "--allow-empty", "-m", "e2e: seed commit for worktree"],
      { cwd: E2E_REPO_PATH, stdio: "inherit" },
    );
  });

  it("creates, opens, returns from, and removes a linked worktree", async () => {
    // "Worktrees" holds the create form; "Branches" holds the current-branch row this test reads
    // text from throughout. Both default closed.
    await expandSidebarSection("Worktrees");
    await expandSidebarSection("Branches");

    const createForm = await $("aria/Create worktree");
    await createForm.waitForExist({ timeout: 10000 });
    const currentBranchButton = await $("//button[contains(., ' (current)')]");
    await currentBranchButton.waitForExist({ timeout: 10000 });
    const mainBranch = (await currentBranchButton.getText()).replace(/ \(current\)$/, "");
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
    await expect(await activeCurrentBranchButton()).toHaveText(`${WORKTREE_BRANCH} (current)`);
    const openMainWorktree = await activeElement(`button=Open ${E2E_REPO_PATH}`);

    await openMainWorktree.click();
    await expect(await activeCurrentBranchButton()).toHaveText(`${mainBranch} (current)`);
    const removeLinkedWorktree = await activeElement(`button=Remove ${WORKTREE_PATH}`);

    await removeLinkedWorktree.click();
    const removalDialog = await $("aria/Remove worktree " + WORKTREE_PATH);
    await removalDialog.waitForExist({ timeout: 10000 });
    await (await removalDialog.$("button=Remove worktree")).click();
    // Not a whole-DOM `aria/${WORKTREE_PATH}` absence check: the linked worktree's own tab
    // (opened earlier, never closed by this test — removing a worktree doesn't close a tab that
    // was pointed at it, a separate gap worth a product-level look) stays open, and its `title`
    // attribute and its own now-stale `WorktreePanel` listing both still reference the removed
    // path — so that check would never pass. What actually matters here is that the *active*
    // tab's own (live, refreshed) worktree list no longer lists it.
    await browser.waitUntil(
      async () => {
        const candidates = await $$(`button=Open ${WORKTREE_PATH}`);
        for (const candidate of candidates) {
          if (await candidate.isDisplayed()) return false;
        }
        return true;
      },
      { timeout: 10000, timeoutMsg: "expected the linked worktree to no longer be listed in the active tab" },
    );
  });
});
