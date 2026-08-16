import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BLAME_FIXTURE_FILE = "blame-fixture.txt";

describe("Browsitory blame", () => {
  before(() => {
    const fixturePath = path.join(E2E_REPO_PATH, BLAME_FIXTURE_FILE);
    fs.writeFileSync(fixturePath, "line one\nline two\n");
    execFileSync("git", ["add", BLAME_FIXTURE_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: blame fixture first commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    fs.writeFileSync(fixturePath, "line one\nCHANGED LINE\n");
    execFileSync("git", ["add", BLAME_FIXTURE_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: blame fixture second commit"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });

    fs.writeFileSync(path.join(E2E_REPO_PATH, "prime.txt"), "prime\n");
  });

  it("shows per-line blame for a historic commit and jumps history selection on a line click", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });

    // Stage+commit the throwaway prime file through the real UI — the only way to make
    // useAppState refetch, which is what makes the two git-CLI commits above show up below.
    const stageButton = await $("button=Stage");
    await stageButton.scrollIntoView({ block: "center" });
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    await browser.execute((el) => { const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set; setter?.call(el, "e2e: prime the refresh"); el.dispatchEvent(new Event("input", { bubbles: true })); }, commitMessageInput);
    const commitButton = await $("button=Commit");
    await commitButton.click();

    const secondCommitEntry = await $("li*=e2e: blame fixture second commit");
    await secondCommitEntry.waitForExist({ timeout: 10000 });
    await secondCommitEntry.click();

    const fileEntry = await $(`button=${BLAME_FIXTURE_FILE}`);
    await fileEntry.waitForExist({ timeout: 10000 });

    const blameButton = await $("button=Blame");
    await blameButton.click();

    // Click a `<td>` inside the row, not the `<tr>` itself: WebKitGTK's WebDriver
    // implementation (what `tauri-driver` proxies to on Linux) reports bare `display:table-row`
    // elements as "element not interactable" even when they're fully visible on screen and have
    // a non-zero bounding rect — a driver quirk, not an app bug. The click still bubbles up to
    // the row's `onClick` handler (`BlameView.tsx`'s `<tr onClick={...}>`), so this exercises
    // the same behavior the brief's row click was meant to.
    const unchangedLineCell = await $("td*=line one");
    await unchangedLineCell.waitForExist({ timeout: 10000 });
    await unchangedLineCell.click();

    const firstCommitEntry = await $("li*=e2e: blame fixture first commit");
    await browser.waitUntil(
      async () => (await firstCommitEntry.getAttribute("aria-selected")) === "true",
      {
        timeout: 10000,
        timeoutMsg:
          "expected clicking the unchanged blame line to select the commit that introduced it, not the commit whose diff was already open",
      },
    );
  });
});
