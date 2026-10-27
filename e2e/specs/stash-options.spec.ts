import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "stash-options-e2e.txt";
const MESSAGE = "e2e stash options";

function git(...args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString();
}
const contents = () => fs.readFileSync(path.join(E2E_REPO_PATH, FILE), "utf8");

async function click(selector: string) {
  const el = await $(selector);
  await el.waitForExist({ timeout: 10000 });
  await browser.execute((e) => (e as HTMLElement).click(), el);
}

describe("Browsitory stash options", () => {
  before(() => {
    // libgit2 refuses to apply a stash while anything is staged; start from an unstaged index.
    git("reset", "-q");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "committed\n");
    git("add", FILE);
    git("commit", "-m", "e2e: stash options fixture");
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "stash me\n");
  });

  // The fixture repo is shared by every spec of the run: leave no stash entry or dirty file.
  after(() => {
    // Drop any entry this spec left (newest first, so indices stay valid), then restore the file.
    const entries = git("stash", "list", "--format=%gd %gs")
      .split("\n")
      .filter((line) => line.includes(MESSAGE));
    for (const line of entries.reverse()) git("stash", "drop", line.split(" ")[0]);
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "committed\n");
  });

  it("stashes one file with a message, shows it, and pops it", async () => {
    // Reload so the app sees the commit and the dirty file made on disk above.
    await browser.refresh();
    await click(`button[aria-label="Actions for ${FILE}"]`);
    await click("button=Stash file…");

    const input = await $(`dialog[aria-label="Stash changes"] input[aria-label="Stash message"]`);
    await input.waitForExist({ timeout: 10000 });
    // React tracks the input's value through its own setter; set it the same way a keystroke
    // would and fire `input` so the controlled component sees the change.
    await browser.execute(
      (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
        setter?.call(el, value);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      input,
      MESSAGE,
    );
    // WebdriverIO cannot mix a CSS prefix with its `button=Text` selector, so chain them.
    const confirm = await $(`dialog[aria-label="Stash changes"]`).$("button=Stash");
    await browser.execute((el) => (el as HTMLElement).click(), confirm);
    await browser.waitUntil(() => contents() === "committed\n", {
      timeout: 10000,
      timeoutMsg: "file was not stashed",
    });

    await expandSidebarSection("Stashes");
    await click(`li*=${MESSAGE}`);
    const details = await $(`section[aria-label="Stash details"]`);
    await details.waitForExist({ timeout: 10000 });
    await details.$(`span*=${FILE}`).waitForExist({ timeout: 10000 });

    const pop = await details.$("button=Pop");
    await pop.waitForEnabled({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), pop);
    await browser.waitUntil(() => contents() === "stash me\n", {
      timeout: 10000,
      timeoutMsg: "pop did not restore the file",
    });
    await browser.waitUntil(() => !git("stash", "list", "--format=%gs").includes(MESSAGE), {
      timeout: 10000,
      timeoutMsg: "the popped stash is still listed",
    });
  });
});
