import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { withFailureDiagnostics } from "../support/diagnostics";
import { clickInPage, waitUntilGoneInPage } from "../support/dom";
import { expandSidebarSection, sidebarState } from "../support/sidebar";
import { git } from "../support/git";
import { UI_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const WORKTREE_NAME = "feature-tree";
const WORKTREE_BRANCH = "feature/e2e-worktree";
// BranchTree splits a "/"-containing branch name into nested folders, so the leaf row's own
// text is only the last segment — see the identical note in branch-management.spec.ts.
const WORKTREE_BRANCH_LEAF = WORKTREE_BRANCH.split("/").pop() ?? WORKTREE_BRANCH;
const WORKTREE_PATH = path.join(os.tmpdir(), `browsitory-e2e-${Date.now()}`);

// Opening a worktree opens it as its own tab (`WorktreePanel`'s `onOpenWorktree` routes through
// `App.tsx`'s tab-opening path) rather than switching the current workspace's branch in place.
// `App.tsx` mounts only the active tab's `RepoWorkspace` (inactive tabs are unmounted — PERF-001),
// so every tab switch tears down one workspace's DOM and mounts a fresh one that renders its
// `BranchTree`/`WorktreePanel` only after its own refresh lands. Both tabs' `WorktreePanel`s list
// identical "Actions for <path>" menu buttons (the worktree list belongs to the underlying
// repository, not the tab — see `WorktreePanel.tsx`), so the label alone can't tell the old
// workspace's (about-to-unmount) copy from the new one's, and a reference captured before the
// switch goes stale. This polls until a displayed match exists after the switch, and returns it.
async function activeElement(selector: string, timeout = UI_TIMEOUT) {
  // Keep the match found *inside* the poll: re-querying afterwards raced a remount (the element
  // seen displayed could be gone by the second `$$`), which surfaced as a spurious failure.
  let found: WebdriverIO.Element | null = null;
  await browser.waitUntil(
    async () => {
      for (const candidate of await $$(selector)) {
        try {
          if (await candidate.isDisplayed()) {
            found = candidate;
            return true;
          }
        } catch {
          // Detached between the query and the check; poll again.
        }
      }
      return false;
    },
    { timeout, timeoutMsg: `expected a visible match for ${selector}` },
  );
  if (found === null) throw new Error(`expected a visible match for ${selector}`);
  return found as WebdriverIO.Element;
}

// BranchTree has no separate switcher control — the current branch is just the row in the Local
// folder whose text carries the " (current)" suffix. The row itself is the interactive element
// (a `role="button"` `<li>`, not a nested `<button>` — see `ListRow.tsx`). Since tabs stay
// mounted (#642), the tab being left still shows its own current-branch row for a moment after
// the click: waiting for the row that names `branch` keeps the poll from picking that one up.
function activeCurrentBranchButton(branch: string) {
  return activeElement(`//li[@role="button" and contains(., '${branch} (current)')]`);
}

/** The ⋯ menu button of the worktree at `worktreePath`, in the active tab's workspace. */
function worktreeActions(worktreePath: string) {
  return `[data-active-repo="true"] button[aria-label="Actions for ${worktreePath}"]`;
}

/**
 * Opens the ⋯ menu of the worktree at `worktreePath` in the active tab and picks `item` (#1011).
 * The button is transparent until its row is hovered or focused (like the Stashes rows), which
 * WebDriver's isDisplayed() counts as hidden: it is clicked in the page, once it renders.
 */
async function worktreeMenuItem(worktreePath: string, item: string) {
  await clickInPage(worktreeActions(worktreePath));
  const menuItem = await $(`button=${item}`);
  await menuItem.waitForExist();
  await menuItem.click();
}

describe("Browsitory worktrees", () => {
  before(() => {
    git(E2E_REPO_PATH, "commit", "--allow-empty", "-m", "e2e: seed commit for worktree");
  });

  it("creates, opens, returns from, and removes a linked worktree", async () => {
    // "Worktrees" holds the list and its "+" (New worktree…); "Branches" holds the current-branch
    // row this test reads text from throughout. Both default closed.
    await expandSidebarSection("Worktrees");
    await expandSidebarSection("Branches");

    const newWorktree = await $('button[aria-label="New worktree…"]');
    // On failure, the sidebar's sections (which are open) and a screenshot (#824).
    await withFailureDiagnostics("worktree-new-button", () => newWorktree.waitForExist(), sidebarState);
    const currentBranchButton = await $('//li[@role="button" and contains(., \' (current)\')]');
    await currentBranchButton.waitForExist();
    // The row reads "<name> (current)" optionally followed by ahead/behind counts (↑n ↓m).
    // From the DOM text, not getText(): WebKitWebDriver's visible text leaves out a branch row's
    // name (see branch-management.spec.ts).
    const rowText = String(await currentBranchButton.getProperty("textContent")).replace(/\s+/g, " ").trim();
    const mainBranch = rowText.replace(/ \(current\).*$/s, "");
    expect(mainBranch).not.toBe("");
    await newWorktree.click();
    const createDialog = await $("dialog[aria-label='New worktree']");
    await createDialog.waitForExist();
    await (await createDialog.$("aria/Branch")).setValue(WORKTREE_BRANCH);
    await (await createDialog.$("aria/Start point")).setValue("HEAD");
    await (await createDialog.$("aria/Worktree path")).setValue(WORKTREE_PATH);
    await (await createDialog.$("aria/Worktree name")).setValue(WORKTREE_NAME);
    await (await createDialog.$("button=Create worktree")).click();
    await waitUntilGoneInPage("dialog[aria-label='New worktree']");

    await (await $(worktreeActions(WORKTREE_PATH))).waitForExist();
    await worktreeMenuItem(WORKTREE_PATH, "Open in new tab");
    // The row may also carry ahead/behind counts (↑n ↓m) after the name.
    // The wait is the check: a displayed row whose DOM text has "<leaf> (current)" (not
    // toHaveText, which reads WebDriver's visible text; see mainBranch above).
    await activeCurrentBranchButton(WORKTREE_BRANCH_LEAF);

    await worktreeMenuItem(E2E_REPO_PATH, "Open in new tab");
    await activeCurrentBranchButton(mainBranch);

    await worktreeMenuItem(WORKTREE_PATH, "Remove…");
    // The dialog element: its title has the same name (ConfirmDialog, #987).
    const removalDialog = await $(`dialog[aria-label="Remove worktree ${WORKTREE_PATH}"]`);
    await removalDialog.waitForExist();
    await (await removalDialog.$("button=Remove worktree")).click();
    // Not a whole-DOM `aria/${WORKTREE_PATH}` absence check: the linked worktree's own tab
    // (opened earlier, never closed by this test — removing a worktree doesn't close a tab that
    // was pointed at it, a separate gap worth a product-level look) stays open, and its tab's
    // `title` attribute still references the removed path — so that check would never pass.
    // What actually matters here is that the *active* tab's own (live, refreshed) worktree list
    // no longer lists it.
    // Not isDisplayed(): the ⋯ button is transparent until hovered, so that would pass at once.
    await waitUntilGoneInPage(worktreeActions(WORKTREE_PATH));
  });
});
