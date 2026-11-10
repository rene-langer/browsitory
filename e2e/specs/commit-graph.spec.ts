import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilInPage } from "../support/dom";
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
    // Wait for the row to move under Staged before committing (#697).
    await waitUntilInPage(`button[aria-label="Unstage ${PRIME_FILE}"]`);
    await browser.execute((el) => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set; setter?.call(el, "e2e: prime the refresh"); el.dispatchEvent(new Event("input", { bubbles: true })); }, commitMessageInput);
    await clickInPage("button", "Commit");

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

  // #620: a commit's row shows the lines it added and removed; its details show the totals.
  it("shows a commit's lines added and removed in its row and details", async () => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, "graph-fixture.txt"), "base\nmore\nmore\nmore\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "graph-stats.txt"), "one\ntwo\n");
    git(E2E_REPO_PATH, "add", "graph-fixture.txt", "graph-stats.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: commit graph line stats");
    await browser.refresh();

    const row = await $("li.commit-row*=e2e: commit graph line stats");
    await row.waitForExist();
    const badge = await row.$('[data-testid="line-stats"]');
    await badge.waitForExist({ timeoutMsg: "the commit's row got no lines-changed badge" });
    // `getText` reads what is shown; the screen-reader text is clipped away.
    expect(await badge.getAttribute("title")).toBe("5 lines added, 0 removed");
    expect(await badge.getText()).toContain("+5");

    await browser.execute((el) => (el as HTMLElement).click(), row);
    const details = await $('section[aria-label="Commit details"]');
    await details.waitForExist();
    await browser.waitUntil(async () => (await details.getText()).includes("2 files changed"), {
      timeoutMsg: "the commit details show no totals",
    });
  });

  // #617: with "Collapse merges" the graph follows first parents; a merge row expands what it
  // brought in, and hides it again.
  it("collapses a merge and expands it on demand", async () => {
    const baseBranch = git(E2E_REPO_PATH, "rev-parse", "--abbrev-ref", "HEAD");
    git(E2E_REPO_PATH, "checkout", "-b", "e2e-graph-merge-topic");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "graph-merge-topic.txt"), "topic\n");
    git(E2E_REPO_PATH, "add", "graph-merge-topic.txt");
    git(E2E_REPO_PATH, "commit", "-m", "e2e: commit graph merged topic");
    git(E2E_REPO_PATH, "checkout", baseBranch);
    git(E2E_REPO_PATH, "merge", "--no-ff", "-m", "e2e: commit graph merge topic", "e2e-graph-merge-topic");
    // Gone once merged: otherwise the graph walks the topic branch's own line too.
    git(E2E_REPO_PATH, "branch", "-d", "e2e-graph-merge-topic");
    await browser.refresh();

    const topicRow = "li.commit-row*=e2e: commit graph merged topic";
    await $(topicRow).waitForExist({ timeoutMsg: "the merged commit is not in the full graph" });
    const collapse = await $('button[aria-label="Collapse merges"]');
    await collapse.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), collapse);
    await $(topicRow).waitForExist({ reverse: true, timeoutMsg: "Collapse merges left the merged commit in" });

    const mergeRow = await $("li.commit-row*=e2e: commit graph merge topic");
    const expand = await mergeRow.$('button[aria-label="Show the 1 commit this merge brought in"]');
    await expand.waitForExist({ timeoutMsg: "the merge row has no +N control" });
    expect(await expand.getText()).toBe("+1");
    await browser.execute((el) => (el as HTMLElement).click(), expand);
    await $(topicRow).waitForExist({ timeoutMsg: "expanding the merge did not show its commit" });

    const hide = await mergeRow.$('button[aria-label="Hide the 1 commit this merge brought in"]');
    await browser.execute((el) => (el as HTMLElement).click(), hide);
    await $(topicRow).waitForExist({ reverse: true, timeoutMsg: "collapsing the merge left its commit in" });

    // Back to the full graph for the cases after this one.
    await browser.execute((el) => (el as HTMLElement).click(), collapse);
    await $(topicRow).waitForExist({ timeoutMsg: "turning Collapse merges off did not bring it back" });
  });

  // #467: a commit on a detached HEAD that no branch contains used to be missing from the graph.
  it("shows a commit made on a detached HEAD, marked HEAD", async () => {
    git(E2E_REPO_PATH, "checkout", "--detach");
    fs.writeFileSync(path.join(E2E_REPO_PATH, DETACHED_FILE), "detached\n");
    // The app doesn't watch the working tree: it reads status when the page loads and after its
    // own actions. `beforeTest`'s reload ran before the two lines above, so its first status
    // read could land before the file existed, and the Stage button never appeared (#556).
    // Reload now, after the change on disk, so that read sees the detached HEAD and the file.
    await browser.refresh();

    const stageButton = await $(`button[aria-label="Stage ${DETACHED_FILE}"]`);
    await stageButton.waitForExist();
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    // Wait for the row to move under Staged before committing (#697).
    await waitUntilInPage(`button[aria-label="Unstage ${DETACHED_FILE}"]`);
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await browser.execute((el) => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set; setter?.call(el, "e2e: commit on a detached HEAD"); el.dispatchEvent(new Event("input", { bubbles: true })); }, commitMessageInput);
    await clickInPage("button", "Commit");

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
