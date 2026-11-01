import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { waitUntilGoneInPage } from "../support/dom";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "hooks-e2e.txt";
const SUMMARY = "e2e: committed without hooks";
const HOOK_OUTPUT = "e2e-hook: lint failed";

function git(...args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();
}

describe("Browsitory git hooks", () => {
  let hookPath = "";

  before(function () {
    // Hooks run through Git for Windows' sh there; this flow covers the Unix path.
    if (process.platform === "win32") this.skip();
    const hooksDir = path.resolve(E2E_REPO_PATH, git("rev-parse", "--git-path", "hooks"));
    fs.mkdirSync(hooksDir, { recursive: true });
    hookPath = path.join(hooksDir, "pre-commit");
    fs.writeFileSync(hookPath, `#!/bin/sh\necho "${HOOK_OUTPUT}" >&2\nexit 1\n`, { mode: 0o755 });
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "hooked\n");
  });

  after(() => {
    // Later specs share this fixture repo: never leave the hook (or a stray staged file) behind.
    if (hookPath) fs.rmSync(hookPath, { force: true });
    if (process.platform === "win32") return;
    if (git("log", "-1", "--format=%s") !== SUMMARY) {
      try {
        git("reset", "-q", "--", FILE);
      } catch {
        // not staged
      }
      fs.rmSync(path.join(E2E_REPO_PATH, FILE), { force: true });
    }
  });

  it("shows a failing pre-commit's output and commits without hooks", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });
    const stageButton = await $(`button[aria-label="Stage ${FILE}"]`);
    await stageButton.waitForExist({ timeout: 10000 });
    // Same opacity-0 workaround as first-flow.spec.ts.
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    // The installed hook makes the "Skip hooks" checkbox appear.
    await (await $("label*=Skip hooks")).waitForExist({ timeout: 10000 });
    await browser.execute(
      (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
        setter?.call(el, value);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      commitMessageInput,
      SUMMARY,
    );
    const commitButton = await $("button=Commit");
    await browser.waitUntil(async () => commitButton.isEnabled(), {
      timeout: 10000,
      timeoutMsg: "Commit did not enable after staging",
    });
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    const dialog = await $(`dialog[aria-label="pre-commit hook failed"]`);
    await dialog.waitForExist({ timeout: 10000 });
    const output = await $(`pre[aria-label="Hook output"]`);
    await browser.waitUntil(async () => (await output.getText()).includes(HOOK_OUTPUT), {
      timeout: 10000,
      timeoutMsg: "hook output not shown",
    });
    if (git("log", "-1", "--format=%s") === SUMMARY) throw new Error("commit landed despite the failing hook");
    // Issue #109: a failed commit keeps the typed message.
    if ((await commitMessageInput.getValue()) !== SUMMARY) throw new Error("commit message was lost");

    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Commit without hooks"));

    await browser.waitUntil(() => git("log", "-1", "--format=%s") === SUMMARY, {
      timeout: 10000,
      timeoutMsg: "commit without hooks did not land",
    });
    await waitUntilGoneInPage(`dialog[aria-label="pre-commit hook failed"]`);
    await browser.waitUntil(async () => (await commitMessageInput.getValue()) === "", {
      timeout: 10000,
      timeoutMsg: "commit message was not cleared after the retried commit",
    });
  });
});

// #185: post-checkout runs with a status and Cancel, and Cancel keeps the switch.
describe("Browsitory post-checkout status", () => {
  const BRANCH = "e2e-hook-switch";
  let hookPath = "";
  let originalBranch = "";

  /** The text of the "Hook status" strip (empty while no hook runs), `null` without one. Read inside
   * the page, so a strip that unmounts between lookup and read can't go stale. */
  async function hookStatusText(): Promise<string | null> {
    return browser.execute(() => {
      const strip = document.querySelector('[role="status"][aria-label="Hook status"]');
      return strip === null ? null : (strip.textContent ?? "");
    });
  }

  before(function () {
    if (process.platform === "win32") this.skip();
    originalBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    git("branch", "-f", BRANCH, "HEAD");
    const hooksDir = path.resolve(E2E_REPO_PATH, git("rev-parse", "--git-path", "hooks"));
    fs.mkdirSync(hooksDir, { recursive: true });
    hookPath = path.join(hooksDir, "post-checkout");
    fs.writeFileSync(hookPath, "#!/bin/sh\nsleep 30\n", { mode: 0o755 });
  });

  after(() => {
    if (hookPath) fs.rmSync(hookPath, { force: true });
    if (process.platform === "win32" || originalBranch === "") return;
    git("checkout", "-q", originalBranch);
    git("branch", "-D", BRANCH);
  });

  it("shows a running post-checkout hook with Cancel, and Cancel keeps the switch", async () => {
    await expandSidebarSection("Branches");
    const row = await $(`//li[@role="button" and contains(., "${BRANCH}")]`);
    await row.waitForExist({ timeout: 10000 });
    // Double-click switches (BranchTree); dispatched in the page like the other row clicks.
    await browser.execute((el) => {
      (el as HTMLElement).dispatchEvent(new MouseEvent("dblclick", { bubbles: true }));
    }, row);

    await browser.waitUntil(async () => (await hookStatusText())?.includes("Running post-checkout hook…") === true, {
      timeout: 10000,
      timeoutMsg: "the Hook status strip did not show the running post-checkout hook",
    });
    const clicked = await browser.execute(() => {
      const strip = document.querySelector('[role="status"][aria-label="Hook status"]');
      const cancel = Array.from(strip?.querySelectorAll("button") ?? []).find(
        (button) => button.textContent?.trim() === "Cancel",
      );
      cancel?.click();
      return cancel !== undefined;
    });
    if (!clicked) throw new Error("no Cancel button in the Hook status strip");

    // Well before the hook's `sleep 30` ends: Cancel stopped it.
    // The strip's live region stays mounted; it empties when the hook ends.
    await browser.waitUntil(async () => !((await hookStatusText()) ?? "").includes("Running"), {
      timeout: 15000,
      timeoutMsg: "the Hook status strip stayed after Cancel",
    });
    if (git("rev-parse", "--abbrev-ref", "HEAD") !== BRANCH) {
      throw new Error("Cancel undid the switch; only the hook should stop");
    }
  });
});
