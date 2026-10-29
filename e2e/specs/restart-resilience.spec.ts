import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");

function git(...args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();
}

function commitFile(fileName: string, content: string, message: string) {
  fs.writeFileSync(path.join(E2E_REPO_PATH, fileName), content);
  git("add", fileName);
  git("commit", "-m", message);
}

/** The status strip's text, read via execute (WebKitGTK's getText() can come back empty). */
async function stripText(): Promise<string> {
  const strip = await $("[role='status'][aria-label='Operation status']");
  if (!(await strip.isExisting())) return "";
  return browser.execute((el) => (el as HTMLElement).textContent ?? "", strip);
}

// #108: a paused rebase is stored on disk, so it survives an app restart. The restart is
// `browser.reloadSession()`: tauri-driver ends the session (closing the app) and launches a fresh
// app process against the same XDG_DATA_HOME and fixture repo.
describe("Browsitory restart resilience", () => {
  before(() => {
    commitFile("restart-base.txt", "base\n", "e2e: restart onto-point commit");
    commitFile("restart-a.txt", "a\n", "e2e: restart commit a");
    commitFile("restart-b.txt", "b\n", "e2e: restart commit b");
  });

  it("keeps a paused rebase across an app restart and finishes it as one undo step", async () => {
    await browser.refresh();
    const onto = await $("li*=e2e: restart onto-point commit");
    await onto.waitForExist({ timeout: 10000 });
    // A synthetic contextmenu event, as rebase.spec.ts does (a real right-click leaves WebKitGTK's
    // modifier state stuck).
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
    }, onto);
    const rebaseButton = await $("button*=Rebase onto here");
    await rebaseButton.waitForExist({ timeout: 10000 });
    await rebaseButton.click();
    const editRow = await $("//li[contains(., 'e2e: restart commit a')]//select[@aria-label='Action']");
    await editRow.waitForExist({ timeout: 10000 });
    await editRow.selectByVisibleText("Edit");
    await (await $("button=Start rebase")).click();

    await browser.waitUntil(async () => (await stripText()).includes("Rebasing, step 1 of 2"), {
      timeout: 10000,
      timeoutMsg: "the rebase did not pause at the edit step",
    });
    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git/browsitory/rebase.json"))).toBe(true);

    await browser.reloadSession();

    await browser.waitUntil(async () => (await stripText()).includes("Rebasing, step 1 of 2"), {
      timeout: 20000,
      timeoutMsg: "the restarted app did not show the paused rebase",
    });
    const continueButton = await $("button=Continue rebase");
    await continueButton.waitForClickable({ timeout: 10000 });
    await continueButton.click();

    await browser.waitUntil(async () => (await stripText()) === "", {
      timeout: 10000,
      timeoutMsg: "the rebase did not finish",
    });
    expect(git("log", "-2", "--format=%s").split("\n")).toEqual(["e2e: restart commit b", "e2e: restart commit a"]);
    expect(fs.existsSync(path.join(E2E_REPO_PATH, ".git/browsitory/rebase.json"))).toBe(false);
    expect(git("symbolic-ref", "-q", "HEAD")).toMatch(/^refs\/heads\//);
    await $("button[aria-label^='Undo Rebase onto']").waitForExist({ timeout: 10000 });
  });
});
