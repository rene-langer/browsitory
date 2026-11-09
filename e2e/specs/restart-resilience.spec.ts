import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { reloadSessionWithRetry } from "../support/session";
import { gitIn } from "../support/git";
import { RESTART_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

const git = gitIn(E2E_REPO_PATH);

function commitFile(fileName: string, content: string, message: string) {
  fs.writeFileSync(path.join(E2E_REPO_PATH, fileName), content);
  git("add", fileName);
  git("commit", "-m", message);
}

/**
 * The status strip's text ("" without a strip), read in one in-page step: WebKitGTK's getText()
 * can come back empty, and an element reference can go stale when the strip unmounts (#182).
 */
async function stripText(): Promise<string> {
  return browser.execute(
    () => document.querySelector("[role='status'][aria-label='Operation status']")?.textContent ?? "",
  );
}

/** The text of every alert in the page (an action that failed shows its error in one). */
async function alertTexts(): Promise<string> {
  return browser.execute(() =>
    Array.from(document.querySelectorAll("[role='alert']"))
      .map((el) => (el.textContent ?? "").replace(/\s+/g, " ").trim())
      .filter((text) => text !== "")
      .join(" | "),
  );
}

/**
 * Clicks **Continue rebase** once the restarted app has settled: the same button element has been
 * in the page, enabled and rendered, for a full second. The strip shows the paused rebase as soon
 * as the restarted app's first refresh lands, but the workspace can still re-render or remount
 * right after (startup tab reconciliation, the next refresh), and a click on a button that is
 * replaced or moves at that moment can get lost (#453, like the palette race in #434). Each poll
 * marks the button it sees, so a remounted button has no mark and starts the second over. The
 * click happens in the same in-page step that sees the settled button, so nothing can replace it
 * in between.
 */
async function clickContinueRebaseWhenSettled(): Promise<void> {
  await browser.waitUntil(
    () =>
      browser.execute(() => {
        const button = Array.from(document.querySelectorAll<HTMLButtonElement>("button")).find(
          (el) => (el.textContent ?? "").trim() === "Continue rebase",
        );
        if (button === undefined || button.disabled || button.getClientRects().length === 0) return false;
        const now = Date.now();
        const seenAt = Number(button.dataset.e2eSeenAt ?? "0");
        if (seenAt === 0) {
          button.dataset.e2eSeenAt = String(now);
          return false;
        }
        if (now - seenAt < 1000) return false;
        button.click();
        return true;
      }),
    { timeout: RESTART_TIMEOUT, interval: 200, timeoutMsg: "Continue rebase never stayed clickable in the restarted app" },
  );
}

// #108: a paused rebase is stored on disk, so it survives an app restart. The restart is
// `reloadSessionWithRetry()` (`browser.reloadSession()`, bounded, #457): tauri-driver ends the session (closing the app) and launches a fresh
// app process against the same XDG_DATA_HOME and fixture repo.
describe("Browsitory restart resilience", () => {
  before(() => {
    // A rebase start refuses tracked local changes (#170).
    const dirty = git("status", "--porcelain", "--untracked-files=no", "--ignore-submodules");
    if (dirty !== "") {
      throw new Error(`restart-resilience.spec.ts needs a clean tracked tree, but the fixture repo has:\n${dirty}`);
    }
    commitFile("restart-base.txt", "base\n", "e2e: restart onto-point commit");
    commitFile("restart-a.txt", "a\n", "e2e: restart commit a");
    commitFile("restart-b.txt", "b\n", "e2e: restart commit b");
  });

  it("keeps a paused rebase across an app restart and finishes it as one undo step", async function () {
    // The restart, the settle wait and the finish wait take up to 20 s each: past mocha's 60 s
    // default, a slow run would end in mocha's timeout instead of the waits' own messages.
    this.timeout(120000);
    await browser.refresh();
    const onto = await $("li*=e2e: restart onto-point commit");
    await onto.waitForExist();
    // A synthetic contextmenu event, as rebase.spec.ts does (a real right-click leaves WebKitGTK's
    // modifier state stuck).
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
    }, onto);
    const rebaseButton = await $("button*=Rebase onto here");
    await rebaseButton.waitForExist();
    await rebaseButton.click();
    const editRow = await $("//li[contains(., 'e2e: restart commit a')]//select[@aria-label='Action']");
    await editRow.waitForExist();
    await editRow.selectByVisibleText("Edit");
    await (await $("button=Start rebase")).click();

    await browser.waitUntil(async () => (await stripText()).includes("Rebasing, step 1 of 2"), {
      timeoutMsg: "the rebase did not pause at the edit step",
    });
    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git/browsitory/rebase.json"))).toBe(true);

    await reloadSessionWithRetry();

    await browser.waitUntil(async () => (await stripText()).includes("Rebasing, step 1 of 2"), {
      timeout: RESTART_TIMEOUT,
      timeoutMsg: "the restarted app did not show the paused rebase",
    });
    await clickContinueRebaseWhenSettled();

    // 20 s like the restart wait: the first Continue in a fresh app process can be slow on a busy
    // runner. The strip's last text and any alert go into the failure message, so a Continue that
    // failed (an error instead of an empty strip) can be told apart from a slow one (#453).
    let lastStrip = "";
    try {
      await browser.waitUntil(
        async () => {
          lastStrip = await stripText();
          return lastStrip === "";
        },
        { timeout: RESTART_TIMEOUT },
      );
    } catch (err) {
      const alerts = await alertTexts().catch(() => "(could not read the alerts)");
      throw new Error(
        `the rebase did not finish within 20 s: the status strip still shows "${lastStrip}", alerts: "${alerts}" (${String(err)})`,
      );
    }
    expect(git("log", "-2", "--format=%s").split("\n")).toEqual(["e2e: restart commit b", "e2e: restart commit a"]);
    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git/browsitory/rebase.json"))).toBe(false);
    expect(git("symbolic-ref", "-q", "HEAD")).toMatch(/^refs\/heads\//);
    await $("button[aria-label^='Undo Rebase onto']").waitForExist();
  });
});
