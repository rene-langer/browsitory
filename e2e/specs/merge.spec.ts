import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { waitUntilGoneInPage, waitUntilInPage } from "../support/dom";
import { git, waitForMergeCommit } from "../support/git";
import { expandSidebarSection, sidebarBranchRow } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

/**
 * "Merge into current branch…" opens the merge options dialog. Picks "Fast-forward if possible"
 * without Squash (a remembered choice or the machine's `merge.ff` could preselect something
 * else) and confirms with Merge, which is enabled once the dialog has loaded its options.
 */
async function confirmMergeDialog(branch: string) {
  const dialog = `dialog[aria-label="Merge ${branch}"]`;
  await $(dialog).waitForExist();
  // WebdriverIO cannot mix a CSS prefix with its `button=Text` selector, so chain them.
  const confirmMerge = await $(dialog).$("button=Merge");
  await confirmMerge.waitForExist();
  await browser.waitUntil(async () => confirmMerge.isEnabled(), {
    timeoutMsg: "the merge dialog did not load its options",
  });
  const squash = await $(`${dialog} input[type="checkbox"]`);
  if (await squash.isSelected()) await browser.execute((el) => (el as HTMLElement).click(), squash);
  const allow = await $(`${dialog} input[type="radio"][value="Allow"]`);
  await browser.execute((el) => (el as HTMLElement).click(), allow);
  await browser.execute((el) => (el as HTMLElement).click(), confirmMerge);
}

/**
 * The UI side of a landed merge commit: its subject shows as a row in the commit graph. (No
 * `li*=<subject>` selector: the subject quotes the branch name.)
 */
async function expectMergeCommitInGraph() {
  const subject = git(E2E_REPO_PATH, "log", "-1", "--format=%s");
  await waitUntilInPage("li", subject, { contains: true });
}

describe("Browsitory merge with conflict resolution", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, "shared.txt"), "line one\nline two\nline three\n");
    git(E2E_REPO_PATH, "add", "shared.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: merge base commit");

    const baseBranch = git(E2E_REPO_PATH, "rev-parse", "--abbrev-ref", "HEAD");

    git(E2E_REPO_PATH, "checkout", "-b", "e2e-merge-feature");
    fs.writeFileSync(
      path.join(E2E_REPO_PATH, "shared.txt"),
      "line one\nfeature two\nline three\n",
    );
    git(E2E_REPO_PATH, "commit", "-am", "e2e: merge feature commit");

    git(E2E_REPO_PATH, "checkout", baseBranch);
    fs.writeFileSync(path.join(E2E_REPO_PATH, "shared.txt"), "line one\nmain two\nline three\n");
    git(E2E_REPO_PATH, "commit", "-am", "e2e: merge base-branch commit");
  });

  it("merges a diverged branch, resolves a conflict per hunk, and commits the merge", async () => {
    // "Branches" defaults closed; expand it before its tree rows exist.
    await expandSidebarSection("Branches");

    // BranchTree's mutating actions live on each row's right-click context menu (see
    // rebase.spec.ts's comment on why a synthetic `contextmenu` DOM event is used instead of
    // WebdriverIO's `.click({ button: "right" })`). The row itself (a `role="button"` `<li>`, not
    // a nested `<button>` — see `ListRow.tsx`) owns the handler, so dispatch on it directly.
    const branchRow = await $(sidebarBranchRow("e2e-merge-feature"));
    await branchRow.waitForExist();
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, branchRow);

    const mergeButton = await $("button*=Merge into current branch");
    await mergeButton.waitForExist();
    await mergeButton.click();
    await confirmMergeDialog("e2e-merge-feature");

    // The uncommitted file list is a `role="listbox"` of `<li role="option">` rows, each holding
    // a `<span>{path} ({kind})</span>` plus icon-only stage/unstage controls — there is no
    // per-file `<button>` carrying the path text any more. Clicking the span still selects the
    // row: the click bubbles to the `<li>`'s own onClick (see `DiffPane.tsx`'s `FileListRow`).
    const conflictedRow = await $("span*=shared.txt (Conflicted)");
    await conflictedRow.waitForExist();
    await conflictedRow.scrollIntoView({ block: "center" });

    const acceptTheirs = await $("button=Accept theirs");
    await browser.execute((el) => (el as HTMLElement).click(), conflictedRow);
    await acceptTheirs.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), acceptTheirs);
    const saveResolution = await $("button=Save resolution");
    await browser.execute((el) => (el as HTMLElement).click(), saveResolution);

    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist();
    await expect(commitMessageInput).toHaveValue(expect.stringContaining("e2e-merge-feature"));

    const commitButton = await $("button=Commit");
    await commitButton.waitForEnabled();
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    // Not a `li*=<branch>` row: the sidebar's branch row matches before the commit lands (#182).
    await waitForMergeCommit(E2E_REPO_PATH);
    await expectMergeCommitInGraph();

    const parentsLine = git(E2E_REPO_PATH, "rev-list", "--parents", "-n", "1", "HEAD");
    expect(parentsLine.split(" ").length).toBe(3); // commit oid + 2 parents = a real merge commit
  });

  it("resolves an add/delete conflict via keep-theirs", async () => {
    git(E2E_REPO_PATH, "checkout", "-B", "e2e-merge-adddelete-base");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "adddelete.txt"), "v1\n");
    git(E2E_REPO_PATH, "add", "adddelete.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: add/delete base commit");
    git(E2E_REPO_PATH, "checkout", "-b", "e2e-merge-adddelete-feature");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "adddelete.txt"), "v2\n");
    git(E2E_REPO_PATH, "commit", "-am", "e2e: add/delete feature modifies");
    git(E2E_REPO_PATH, "checkout", "e2e-merge-adddelete-base");
    fs.rmSync(path.join(E2E_REPO_PATH, "adddelete.txt"));
    git(E2E_REPO_PATH, "commit", "-am", "e2e: add/delete base deletes");

    // `state.branches` only refreshes on a `runMutation` call (see `useAppState.ts`), and the
    // branch/checkout work above was all done directly on disk via the git CLI, bypassing the
    // app entirely — so the just-created branches aren't in `state.branches` yet. Unlike
    // `commit-graph.spec.ts`'s "prime the refresh" trick (which relies on the app's one-time
    // mount-time `openRepo()` call racing a `before()` hook and losing), this is the second
    // `it()` in this file: the app has been running and mutating state for a while, so there's
    // no mount race left to win, and every mutation that could force a refresh (Stage, a
    // same-branch re-switch) is itself gated on state that's equally stale. A full reload is the
    // direct fix — it re-mounts `App`, which re-runs the same `openRepo()` auto-open effect
    // against the current on-disk repo state, no stale cache involved.
    await browser.refresh();

    // Idempotent re-expand — see the previous test's comment. Not reset by the reload above:
    // `AccordionSection`'s open state persists in localStorage, which survives a page refresh.
    await expandSidebarSection("Branches");

    const branchRow = await $(sidebarBranchRow("e2e-merge-adddelete-feature"));
    await branchRow.waitForExist();
    // The row itself owns the context-menu handler (see the previous test's comment).
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, branchRow);

    const mergeButton = await $("button*=Merge into current branch");
    await mergeButton.waitForExist();
    await mergeButton.click();
    await confirmMergeDialog("e2e-merge-adddelete-feature");

    // See the first test: the row's path text lives in a `<span>` inside `<li role="option">`.
    const conflictedRow = await $("span*=adddelete.txt (Conflicted)");
    await conflictedRow.waitForExist();
    await conflictedRow.scrollIntoView({ block: "center" });
    await browser.execute((el) => (el as HTMLElement).click(), conflictedRow);

    const keepTheirs = await $("button=Keep their version");
    await keepTheirs.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), keepTheirs);

    const commitButton = await $("button=Commit");
    await commitButton.waitForEnabled();
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    // Not a `li*=<branch>` row: the sidebar's branch row matches before the commit lands (#182).
    await waitForMergeCommit(E2E_REPO_PATH);
    await expectMergeCommitInGraph();

    const parentsLine = git(E2E_REPO_PATH, "rev-list", "--parents", "-n", "1", "HEAD");
    expect(parentsLine.split(" ").length).toBe(3); // commit oid + 2 parents = a real merge commit
  });

  it("aborts a conflicted merge from the UI, leaving the working tree clean", async () => {
    git(E2E_REPO_PATH, "checkout", "-B", "e2e-merge-abort-base");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "abort.txt"), "line one\nline two\n");
    git(E2E_REPO_PATH, "add", "abort.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: abort-merge base commit");

    git(E2E_REPO_PATH, "checkout", "-b", "e2e-merge-abort-feature");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "abort.txt"), "line one\nfeature two\n");
    git(E2E_REPO_PATH, "commit", "-am", "e2e: abort-merge feature commit");

    git(E2E_REPO_PATH, "checkout", "e2e-merge-abort-base");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "abort.txt"), "line one\nbase two\n");
    git(E2E_REPO_PATH, "commit", "-am", "e2e: abort-merge base-branch commit");

    const headBeforeMerge = git(E2E_REPO_PATH, "rev-parse", "HEAD");

    // See the second test's comment: branches created directly via the git CLI aren't in
    // `state.branches` until a full reload re-runs the mount-time `openRepo()`.
    await browser.refresh();
    await expandSidebarSection("Branches");

    const branchRow = await $(sidebarBranchRow("e2e-merge-abort-feature"));
    await branchRow.waitForExist();
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, branchRow);

    const mergeButton = await $("button*=Merge into current branch");
    await mergeButton.waitForExist();
    await mergeButton.click();
    await confirmMergeDialog("e2e-merge-abort-feature");

    const conflictedRow = await $("span*=abort.txt (Conflicted)");
    await conflictedRow.waitForExist();

    // The commit box offers "Abort merge" too; this is the operation status strip's.
    const abortMergeButton = await $("div[aria-label='Operation status']").$("button=Abort merge");
    await abortMergeButton.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), abortMergeButton);

    await waitUntilGoneInPage("button", "Abort merge");

    const headAfterAbort = git(E2E_REPO_PATH, "rev-parse", "HEAD");
    expect(headAfterAbort).toBe(headBeforeMerge);

    const fileContents = fs.readFileSync(path.join(E2E_REPO_PATH, "abort.txt"), "utf8");
    expect(fileContents).toBe("line one\nbase two\n");

    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git", "MERGE_HEAD"))).toBe(false);

    await waitUntilGoneInPage("span", "abort.txt (Conflicted)", { contains: true });
  });
});
