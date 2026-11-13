import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, setValueInPage, waitUntilInPage } from "../support/dom";
import { git, gitWith, waitForMergeCommit } from "../support/git";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "merge-editor.txt";
const BRANCH = "e2e-merge-editor-feature";

/** Ten lines; `two` and `nine` are far enough apart for git to make two conflicts of them. */
function lines(two: string, nine: string): string {
  return ["one", two, "three", "four", "five", "six", "seven", "eight", nine, "ten", ""].join("\n");
}

/** The value of the result field in the block named `block`. */
async function blockResult(block: string): Promise<string> {
  return browser.execute((name: string) => {
    const field = document.querySelector<HTMLTextAreaElement>(`section[aria-label="${name}"] [data-pane="result"] textarea`);
    return field?.value ?? "<no result field>";
  }, block);
}

// #880: the three-pane merge editor (#875 … #879) end to end: a two-block conflict resolved with
// the take-side arrows and an edit, saved, and the merge continued.
describe("Browsitory three-pane merge editor", () => {
  let originalSize = { width: 0, height: 0 };

  before(async () => {
    const baseBranch = git(E2E_REPO_PATH, "rev-parse", "--abbrev-ref", "HEAD");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), lines("two", "nine"));
    git(E2E_REPO_PATH, "add", FILE);
    git(E2E_REPO_PATH, "commit", "-m", "e2e: merge editor base");
    git(E2E_REPO_PATH, "checkout", "-b", BRANCH);
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), lines("two feature", "nine feature"));
    git(E2E_REPO_PATH, "commit", "-am", "e2e: merge editor feature");
    git(E2E_REPO_PATH, "checkout", baseBranch);
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), lines("two main", "nine main"));
    git(E2E_REPO_PATH, "commit", "-am", "e2e: merge editor main");
    // Expected to stop on the two conflicts (git exits 1).
    let stopped = false;
    try {
      gitWith(E2E_REPO_PATH, ["merge", "--no-edit", BRANCH], { stdio: "ignore" });
    } catch {
      stopped = true;
    }
    if (!stopped) throw new Error("expected the merge to stop on a conflict");

    // Wide enough for the three panes next to the commit list (below 680 px the editor shows the
    // blocks instead).
    originalSize = await browser.getWindowSize();
    await browser.setWindowSize(1800, 1000);
    // The merge started outside the app; `beforeTest` (wdio.conf.ts) reloads it before the test.
  });

  after(async () => {
    if (originalSize.width > 0) await browser.setWindowSize(originalSize.width, originalSize.height);
  });

  it("resolves two conflicts with the arrows and an edit, saves, and commits the merge", async () => {
    await clickInPage("span", `${FILE} (Conflicted)`);
    const first = "Conflict 1 of 2";
    const second = "Conflict 2 of 2";
    await waitUntilInPage(`section[aria-label="${first}"]`);
    await waitUntilInPage(`section[aria-label="${second}"]`);
    // The three panes, not the block view.
    await waitUntilInPage('button[aria-pressed="true"]', "Three panes");
    await waitUntilInPage('[role="status"]', "0 of 2 resolved");
    expect(await blockResult(first)).toContain("<<<<<<<");

    // Take theirs in the first block; ours, then theirs after it, in the second.
    await clickInPage(`section[aria-label="${first}"] button[aria-label="Take theirs"]`);
    await clickInPage(`section[aria-label="${second}"] button[aria-label="Take ours"]`);
    await clickInPage(`section[aria-label="${second}"] button[aria-label="Take theirs"]`);
    await waitUntilInPage('[role="status"]', "2 of 2 resolved");
    expect(await blockResult(first)).toBe("two feature\n");
    expect(await blockResult(second)).toBe("nine main\nnine feature\n");

    // An edit in the second block's result.
    await setValueInPage(`section[aria-label="${second}"] [data-pane="result"] textarea`, "nine, both\n");
    await waitUntilInPage(`section[aria-label="${second}"] h4`, "Edited", { contains: true });

    await clickInPage("button", "Save resolution");
    const expected = lines("two feature", "nine, both");
    await browser.waitUntil(() => fs.readFileSync(path.join(E2E_REPO_PATH, FILE), "utf8") === expected, {
      timeoutMsg: `the resolution was not saved: ${JSON.stringify(fs.readFileSync(path.join(E2E_REPO_PATH, FILE), "utf8"))}`,
    });
    // Saved and staged: nothing of the file is left unstaged or in conflict.
    await browser.waitUntil(() => git(E2E_REPO_PATH, "status", "--porcelain", "--", FILE) === `M  ${FILE}`, {
      timeoutMsg: `the file was not staged: ${git(E2E_REPO_PATH, "status", "--porcelain", "--", FILE)}`,
    });

    // Continue the merge: commit it.
    const commitButton = await $('button[data-action="commit"]');
    await commitButton.waitForEnabled();
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);
    await waitForMergeCommit(E2E_REPO_PATH);
    expect(git(E2E_REPO_PATH, "show", `HEAD:${FILE}`)).toBe(expected.trim());
  });
});
