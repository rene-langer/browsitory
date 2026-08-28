import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

describe("Browsitory merge with conflict resolution", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, "shared.txt"), "line one\nline two\nline three\n");
    execFileSync("git", ["add", "shared.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: merge base commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    const baseBranch = execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();

    execFileSync("git", ["checkout", "-b", "e2e-merge-feature"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    fs.writeFileSync(
      path.join(E2E_REPO_PATH, "shared.txt"),
      "line one\nfeature two\nline three\n",
    );
    execFileSync("git", ["commit", "-am", "e2e: merge feature commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    execFileSync("git", ["checkout", baseBranch], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(E2E_REPO_PATH, "shared.txt"), "line one\nmain two\nline three\n");
    execFileSync("git", ["commit", "-am", "e2e: merge base-branch commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
  });

  it("merges a diverged branch, resolves a conflict per hunk, and commits the merge", async () => {
    // "Branches" defaults closed; expand it before its tree rows exist.
    await expandSidebarSection("Branches");

    // BranchTree's mutating actions live on each row's right-click context menu (see
    // rebase.spec.ts's comment on why a synthetic `contextmenu` DOM event is used instead of
    // WebdriverIO's `.click({ button: "right" })`). The handler is bound to the branch-name
    // `<button>` inside the row, not the row `<li>` itself, so dispatch on that button.
    const branchRow = await $("li*=e2e-merge-feature");
    await branchRow.waitForExist({ timeout: 10000 });
    const branchButton = await branchRow.$("button");
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, branchButton);

    const mergeButton = await $("button*=Merge into current branch");
    await mergeButton.waitForExist({ timeout: 10000 });
    await mergeButton.click();

    // The uncommitted file list is a `role="listbox"` of `<li role="option">` rows, each holding
    // a `<span>{path} ({kind})</span>` plus icon-only stage/unstage controls — there is no
    // per-file `<button>` carrying the path text any more. Clicking the span still selects the
    // row: the click bubbles to the `<li>`'s own onClick (see `DiffPane.tsx`'s `FileListRow`).
    const conflictedRow = await $("span*=shared.txt (Conflicted)");
    await conflictedRow.waitForExist({ timeout: 10000 });
    await conflictedRow.scrollIntoView({ block: "center" });

    const acceptTheirs = await $("button=Accept Theirs");
    await browser.execute((el) => (el as HTMLElement).click(), conflictedRow);
    await acceptTheirs.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), acceptTheirs);
    const saveResolution = await $("button=Save resolution");
    await browser.execute((el) => (el as HTMLElement).click(), saveResolution);

    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });
    await expect(commitMessageInput).toHaveValue(expect.stringContaining("e2e-merge-feature"));

    const commitButton = await $("button=Commit");
    await commitButton.waitForEnabled({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    const mergeCommitEntry = await $("li*=e2e-merge-feature");
    await mergeCommitEntry.waitForExist({ timeout: 10000 });

    const parentsLine = execFileSync("git", ["rev-list", "--parents", "-n", "1", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();
    expect(parentsLine.split(" ").length).toBe(3); // commit oid + 2 parents = a real merge commit
  });

  it("resolves an add/delete conflict via keep-theirs", async () => {
    execFileSync("git", ["checkout", "-B", "e2e-merge-adddelete-base"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    fs.writeFileSync(path.join(E2E_REPO_PATH, "adddelete.txt"), "v1\n");
    execFileSync("git", ["add", "adddelete.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: add/delete base commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    execFileSync("git", ["checkout", "-b", "e2e-merge-adddelete-feature"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    fs.writeFileSync(path.join(E2E_REPO_PATH, "adddelete.txt"), "v2\n");
    execFileSync("git", ["commit", "-am", "e2e: add/delete feature modifies"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    execFileSync("git", ["checkout", "e2e-merge-adddelete-base"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    fs.rmSync(path.join(E2E_REPO_PATH, "adddelete.txt"));
    execFileSync("git", ["commit", "-am", "e2e: add/delete base deletes"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    // `state.branches` only refreshes on a `runMutation` call (see `useAppState.ts`), and the
    // branch/checkout work above was all done directly on disk via `execFileSync`, bypassing the
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

    const branchRow = await $("li*=e2e-merge-adddelete-feature");
    await branchRow.waitForExist({ timeout: 10000 });
    const branchButton = await branchRow.$("button");
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, branchButton);

    const mergeButton = await $("button*=Merge into current branch");
    await mergeButton.waitForExist({ timeout: 10000 });
    await mergeButton.click();

    // See the first test: the row's path text lives in a `<span>` inside `<li role="option">`.
    const conflictedRow = await $("span*=adddelete.txt (Conflicted)");
    await conflictedRow.waitForExist({ timeout: 10000 });
    await conflictedRow.scrollIntoView({ block: "center" });
    await browser.execute((el) => (el as HTMLElement).click(), conflictedRow);

    const keepTheirs = await $("button=Keep Their Version");
    await keepTheirs.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), keepTheirs);

    const commitButton = await $("button=Commit");
    await commitButton.waitForEnabled({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    const mergeCommitEntry = await $("li*=e2e-merge-adddelete-feature");
    await mergeCommitEntry.waitForExist({ timeout: 10000 });

    const parentsLine = execFileSync("git", ["rev-list", "--parents", "-n", "1", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();
    expect(parentsLine.split(" ").length).toBe(3); // commit oid + 2 parents = a real merge commit
  });
});
