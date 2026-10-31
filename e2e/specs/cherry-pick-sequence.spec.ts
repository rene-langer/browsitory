import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const git = (...args: string[]) =>
  execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();

async function jsClick(selector: string) {
  const el = await $(selector);
  await el.waitForExist({ timeout: 10000 });
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
    await first.waitForExist({ timeout: 10000 });
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
    await strip.waitForExist({ timeout: 10000 });
    await expect(strip).toHaveText(expect.stringContaining("Cherry-picking 2/3"));
    // Paused on step two: one commit landed, the sequence is stored on disk.
    expect(git("log", "--format=%s", `${startTip}..HEAD`)).toBe("e2e: seq one");
    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git", "browsitory", "sequence.json"))).toBe(true);

    await jsClick("li*=Uncommitted Changes");
    const conflicted = await $("span*=seq-shared.txt (Conflicted)");
    await conflicted.waitForExist({ timeout: 10000 });
    await conflicted.scrollIntoView({ block: "center" });
    await browser.execute((el) => (el as HTMLElement).click(), conflicted);
    const ours = await $("button=Use ours (whole file)");
    await ours.waitForEnabled({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), ours);

    const cont = await strip.$("button=Continue");
    await cont.waitForEnabled({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), cont);

    await browser.waitUntil(() => git("log", "-1", "--format=%s") === "e2e: seq three", {
      timeout: 10000,
      timeoutMsg: "the sequence did not finish",
    });
    expect(git("log", "--format=%s", `${startTip}..HEAD`).split("\n")).toEqual([
      "e2e: seq three",
      "e2e: seq two",
      "e2e: seq one",
    ]);
    expect(fs.readFileSync(path.join(E2E_REPO_PATH, "seq-shared.txt"), "utf8")).toBe("ours\n");
    expect(fs.existsSync(path.join(E2E_REPO_PATH, "seq-two.txt"))).toBe(true);
    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git", "browsitory", "sequence.json"))).toBe(false);
    // Polled in the page rather than through an element handle: the strip is removed while
    // the app refreshes, and a handle to it can go stale mid-wait (a tauri-driver flake).
    await browser.waitUntil(
      () => browser.execute(() => document.querySelector('[aria-label="Operation status"]') === null),
      { timeout: 10000, timeoutMsg: "the operation status strip did not go away" },
    );
  });
});
