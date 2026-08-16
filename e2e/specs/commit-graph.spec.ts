import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

describe("Browsitory commit graph", () => {
  before(() => {
    const fixturePath = path.join(E2E_REPO_PATH, "graph-fixture.txt");
    fs.writeFileSync(fixturePath, "base\n");
    execFileSync("git", ["add", "graph-fixture.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: commit graph base commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    const baseBranch = execFileSync("git", ["rev-parse", "--abbrev-ref", "HEAD"], {
      cwd: E2E_REPO_PATH,
    })
      .toString()
      .trim();

    execFileSync("git", ["checkout", "-b", "e2e-graph-feature"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    fs.writeFileSync(path.join(E2E_REPO_PATH, "feature-fixture.txt"), "feature\n");
    execFileSync("git", ["add", "feature-fixture.txt"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: commit graph feature commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    execFileSync("git", ["checkout", baseBranch], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(E2E_REPO_PATH, "prime.txt"), "prime\n");
  });

  it("shows commits from every local branch with correct branch labels", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });

    const stageButton = await $("button=Stage");
    await stageButton.scrollIntoView({ block: "center" });
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    await browser.execute((el) => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set; setter?.call(el, "e2e: prime the refresh"); el.dispatchEvent(new Event("input", { bubbles: true })); }, commitMessageInput);
    const commitButton = await $("button=Commit");
    await commitButton.click();

    const baseCommitEntry = await $("li*=e2e: commit graph base commit");
    await baseCommitEntry.waitForExist({ timeout: 10000 });
    const featureCommitEntry = await $("li*=e2e: commit graph feature commit");
    await featureCommitEntry.waitForExist({ timeout: 10000 });

    // The feature branch's tip commit should carry a branch-name badge in the same row —
    // confirms `branch_refs` made it end-to-end from git-core through to the rendered row.
    // Assert on `featureCommitEntry` itself (not a fresh `$("li*=...")` lookup) so this can
    // only pass if the badge is in that specific commit's row, not some unrelated `<li>`
    // elsewhere in the app (e.g. BranchSwitcher's popover).
    const featureCommitEntryText = await featureCommitEntry.getText();
    expect(featureCommitEntryText).toContain("e2e-graph-feature");
  });
});
