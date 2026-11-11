import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage } from "../support/dom";
import { dragInPage } from "../support/drag";
import { gitIn } from "../support/git";
import { expandSidebarSection, sidebarBranchRow } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const git = gitIn(E2E_REPO_PATH);

async function jsClick(selector: string) {
  const el = await $(selector);
  await el.waitForExist();
  await browser.execute((node) => (node as HTMLElement).click(), el);
}

describe("Browsitory cherry-pick sequence", () => {
  let baseBranch = "";
  let startTip = "";

  before(() => {
    baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "base\n");
    git("add", "seq-shared.txt");
    git("commit", "-m", "e2e: seq base");
    git("checkout", "-b", "e2e-seq-source");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-one.txt"), "1\n");
    git("add", "seq-one.txt");
    git("commit", "-m", "e2e: seq one");
    // The middle commit conflicts on seq-shared.txt and also adds a file, so taking "ours" for
    // the shared file still leaves a non-empty commit (an empty one would be skipped).
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "theirs\n");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-two.txt"), "2\n");
    git("add", "seq-shared.txt", "seq-two.txt");
    git("commit", "-m", "e2e: seq two");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-three.txt"), "3\n");
    git("add", "seq-three.txt");
    git("commit", "-m", "e2e: seq three");
    git("checkout", baseBranch);
    fs.writeFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "ours\n");
    git("commit", "-am", "e2e: seq ours");
    startTip = git("rev-parse", "HEAD");
  });

  it("cherry-picks three commits, resolves the conflict with whole-file ours, and continues", async () => {
    await browser.refresh();
    const first = await $("li*=e2e: seq one");
    await first.waitForExist();
    await browser.execute((el) => (el as HTMLElement).click(), first);
    // Synthetic ctrl-click: see rebase.spec.ts on why a real held modifier is avoided.
    for (const summary of ["e2e: seq two", "e2e: seq three"]) {
      const row = await $(`li*=${summary}`);
      await browser.execute((el) => {
        el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
      }, row);
    }
    await jsClick("button=Cherry-pick 3 commits");

    const strip = await $('[aria-label="Operation status"]');
    await strip.waitForExist();
    await expect(strip).toHaveText(expect.stringContaining("Cherry-picking 2/3"));
    // Paused on step two: one commit landed, the sequence is stored on disk.
    expect(git("log", "--format=%s", `${startTip}..HEAD`)).toBe("e2e: seq one");
    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git", "browsitory", "sequence.json"))).toBe(true);

    await jsClick("li*=Uncommitted changes");
    const conflicted = await $("span=seq-shared.txt (Conflicted)");
    await conflicted.waitForExist();
    await conflicted.scrollIntoView({ block: "center" });
    await browser.execute((el) => (el as HTMLElement).click(), conflicted);
    await clickInPage("button", "Use ours (whole file)");
    await browser.waitUntil(() => git("diff", "--name-only", "--diff-filter=U") === "", {
      timeoutMsg: "seq-shared.txt is still conflicted",
    });

    // Continue is a `ReasonButton`: blocked, it is `aria-disabled`, not `disabled`, and ignores
    // a click, so `waitForEnabled` passed at once and the click could land while "Use ours" was
    // still being applied and do nothing (#790). `clickInPage` waits until it isn't blocked.
    await clickInPage('[aria-label="Operation status"] button', "Continue");

    await browser.waitUntil(() => git("log", "-1", "--format=%s") === "e2e: seq three", {
      timeoutMsg: "the sequence did not finish",
    });
    expect(git("log", "--format=%s", `${startTip}..HEAD`).split("\n")).toEqual([
      "e2e: seq three",
      "e2e: seq two",
      "e2e: seq one",
    ]);
    expect(fs.readFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "utf8")).toBe("ours\n");
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "seq-two.txt"))).toBe(true);
    // Polled: the worker clears the stored sequence just after the last commit lands, so the
    // `git log` wait above can return while the file is still there (#182).
    await browser.waitUntil(
      () => !fs.existsSync(path.join(E2E_REPO_PATH, ".git", "browsitory", "sequence.json")),
      { timeoutMsg: "the stored sequence (sequence.json) was not cleared" },
    );
    // Polled in the page rather than through an element handle: the strip is removed while
    // the app refreshes, and a handle to it can go stale mid-wait (a tauri-driver flake).
    await browser.waitUntil(
      () => browser.execute(() => document.querySelector('[aria-label="Operation status"]') === null),
      { timeoutMsg: "the operation status strip did not go away" },
    );
  });

  // #744: a commit dragged from the graph onto the current branch in the sidebar offers
  // "Cherry-pick <commit> onto <branch>", which picks it like the commit menu's Cherry-pick.
  it("cherry-picks a commit dragged onto the current branch", async () => {
    git("checkout", "-b", "e2e-drag-source");
    fs.writeFileSync(path.join(E2E_REPO_PATH, "drag-pick.txt"), "dragged\n");
    git("add", "drag-pick.txt");
    git("commit", "-m", "e2e: drag pick");
    const shortId = git("rev-parse", "HEAD").slice(0, 7);
    git("checkout", baseBranch);
    const tipBefore = git("rev-parse", "HEAD");

    await browser.refresh();
    await expandSidebarSection("Branches");
    const row = await $("li*=e2e: drag pick");
    await row.waitForExist();
    // Pressed on the summary, not on a branch badge.
    const summary = await row.$("span*=e2e: drag pick");
    const currentRow = await $(sidebarBranchRow(baseBranch));
    await currentRow.waitForExist();
    await dragInPage(summary, currentRow);

    await jsClick(`button=Cherry-pick ${shortId} onto ${baseBranch}`);
    await browser.waitUntil(() => git("log", "-1", "--format=%s") === "e2e: drag pick", {
      timeoutMsg: "the dragged commit was not cherry-picked",
    });
    expect(git("rev-parse", "HEAD~1")).toBe(tipBefore);
    expect(git("rev-parse", "--abbrev-ref", "HEAD")).toBe(baseBranch);
  });
});
