import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

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
    const branchSwitcherButton = await $("[aria-label='Branch switcher']");
    await branchSwitcherButton.waitForExist({ timeout: 10000 });
    await branchSwitcherButton.click();

    const mergeButton = await $("li*=e2e-merge-feature").then((li) =>
      li.$("button*=Merge into current branch"),
    );
    await mergeButton.click();

    const conflictedRow = await $("button*=shared.txt (Conflicted)");
    await conflictedRow.waitForExist({ timeout: 10000 });
    await conflictedRow.click();

    const acceptTheirs = await $("button=Accept Theirs");
    await acceptTheirs.waitForExist({ timeout: 10000 });
    await acceptTheirs.click();
    const saveResolution = await $("button=Save resolution");
    await saveResolution.click();

    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });
    await expect(commitMessageInput).toHaveValue(expect.stringContaining("e2e-merge-feature"));

    const commitButton = await $("button=Commit");
    await commitButton.waitForEnabled({ timeout: 10000 });
    await commitButton.click();

    const mergeCommitEntry = await $("li*=e2e-merge-feature");
    await mergeCommitEntry.waitForExist({ timeout: 10000 });
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

    const branchSwitcherButton = await $("[aria-label='Branch switcher']");
    await branchSwitcherButton.waitForExist({ timeout: 10000 });
    await branchSwitcherButton.click();

    const mergeButton = await $("li*=e2e-merge-adddelete-feature").then((li) =>
      li.$("button*=Merge into current branch"),
    );
    await mergeButton.click();

    const conflictedRow = await $("button*=adddelete.txt (Conflicted)");
    await conflictedRow.waitForExist({ timeout: 10000 });
    await conflictedRow.click();

    const keepTheirs = await $("button=Keep Their Version");
    await keepTheirs.waitForExist({ timeout: 10000 });
    await keepTheirs.click();

    const commitButton = await $("button=Commit");
    await commitButton.waitForEnabled({ timeout: 10000 });
    await commitButton.click();

    const mergeCommitEntry = await $("li*=e2e-merge-adddelete-feature");
    await mergeCommitEntry.waitForExist({ timeout: 10000 });
  });
});
