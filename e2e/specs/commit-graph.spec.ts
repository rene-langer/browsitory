import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { git } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
// Spec-unique name on purpose — see the same constant in `blame-viewer.spec.ts` for why two
// specs sharing a throwaway `prime.txt` is only safe as long as each spec file gets its own
// freshly re-cloned fixture repo.
const PRIME_FILE = "graph-prime.txt";
const DETACHED_FILE = "graph-detached.txt";

describe("Browsitory commit graph", () => {
  before(() => {
    const fixturePath = path.join(E2E_REPO_PATH, "graph-fixture.txt");
    fs.writeFileSync(fixturePath, "base\n");
    git(E2E_REPO_PATH, "add", "graph-fixture.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: commit graph base commit");

    const baseBranch = git(E2E_REPO_PATH, "rev-parse", "--abbrev-ref", "HEAD");

    git(E2E_REPO_PATH, "checkout", "-b", "e2e-graph-feature");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "feature-fixture.txt"), "feature\n");
    git(E2E_REPO_PATH, "add", "feature-fixture.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: commit graph feature commit");

    git(E2E_REPO_PATH, "checkout", baseBranch);
    fs.writeFileSync(path.join(E2E_REPO_PATH, PRIME_FILE), "prime\n");
  });

  it("shows commits from every local branch with correct branch labels", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist();

    // The per-row stage control is icon-only (aria-label only, no visible text) since the
    // file-list migration to `ListRow` — target the throwaway file this spec's `before` hook
    // wrote for exactly this "prime the refresh" commit.
    const stageButton = await $(`button[aria-label="Stage ${PRIME_FILE}"]`);
    await stageButton.waitForExist();
    // `opacity: 0` until the row is hovered/focused — see `first-flow.spec.ts`'s note on why
    // both the scroll and the click go through `browser.execute`.
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    await browser.execute((el) => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set; setter?.call(el, "e2e: prime the refresh"); el.dispatchEvent(new Event("input", { bubbles: true })); }, commitMessageInput);
    const commitButton = await $("button=Commit");
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    const baseCommitEntry = await $("li*=e2e: commit graph base commit");
    await baseCommitEntry.waitForExist();
    const featureCommitEntry = await $("li*=e2e: commit graph feature commit");
    await featureCommitEntry.waitForExist();

    // The feature branch's tip commit should carry a branch-name badge in the same row —
    // confirms `branch_refs` made it end-to-end from git-core through to the rendered row.
    // Assert on `featureCommitEntry` itself (not a fresh `$("li*=...")` lookup) so this can
    // only pass if the badge is in that specific commit's row, not some unrelated `<li>`
    // elsewhere in the app (e.g. BranchTree's Local folder rows).
    const featureCommitEntryText = await featureCommitEntry.getText();
    expect(featureCommitEntryText).toContain("e2e-graph-feature");
  });

  // #467: a commit on a detached HEAD that no branch contains used to be missing from the graph.
  it("shows a commit made on a detached HEAD, marked HEAD", async () => {
    git(E2E_REPO_PATH, "checkout", "--detach");
    fs.writeFileSync(path.join(E2E_REPO_PATH, DETACHED_FILE), "detached\n");

    const stageButton = await $(`button[aria-label="Stage ${DETACHED_FILE}"]`);
    await stageButton.waitForExist();
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await browser.execute((el) => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set; setter?.call(el, "e2e: commit on a detached HEAD"); el.dispatchEvent(new Event("input", { bubbles: true })); }, commitMessageInput);
    const commitButton = await $("button=Commit");
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    await browser.waitUntil(
      () =>
        git(E2E_REPO_PATH, "log", "-1", "--format=%s") ===
        "e2e: commit on a detached HEAD",
      { timeoutMsg: "the commit on the detached HEAD did not land" },
    );
    // No branch contains it: only HEAD reaches it.
    expect(
      git(E2E_REPO_PATH, "for-each-ref", "--contains", "HEAD", "refs/heads"),
    ).toBe("");

    const detachedRow = await $("li.commit-row*=e2e: commit on a detached HEAD");
    await detachedRow.waitForExist();
    await detachedRow.$("span=HEAD").waitForExist({
      timeoutMsg: "the detached HEAD's row has no HEAD badge",
    });
  });
});
