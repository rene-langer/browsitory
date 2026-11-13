import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { clickInPage, waitUntilInPage } from "../support/dom";
import { expandSidebarSection, sidebarRemoteFolder } from "../support/sidebar";
import { gitIn } from "../support/git";
import { SLOW_UI_TIMEOUT } from "../support/timeouts";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const REMOTE_NAME = "oplog-origin";
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-oplog-remote.git");
const HOOK_OUTPUT = "e2e-oplog: push refused by pre-push";

const git = gitIn(E2E_REPO_PATH);

// The operation log (#887-#891): a push fails on its pre-push hook; the error's "Show details"
// opens the log on that push, with the hook's output.
describe("Browsitory operation log", () => {
  let hookPath = "";

  before(function () {
    // Hooks run through Git for Windows' sh there; this flow covers the Unix path.
    if (process.platform === "win32") this.skip();
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    gitIn(os.tmpdir())("init", "--bare", BARE_REMOTE_PATH);
    git("remote", "add", REMOTE_NAME, BARE_REMOTE_PATH);
    const hooksDir = path.resolve(E2E_REPO_PATH, git("rev-parse", "--git-path", "hooks"));
    fs.mkdirSync(hooksDir, { recursive: true });
    hookPath = path.join(hooksDir, "pre-push");
    fs.writeFileSync(hookPath, `#!/bin/sh\necho "${HOOK_OUTPUT}" >&2\nexit 1\n`, { mode: 0o755 });
  });

  after(() => {
    // Later specs share this fixture repo: leave no hook or remote behind.
    if (process.platform === "win32") return;
    if (hookPath) fs.rmSync(hookPath, { force: true });
    try {
      git("remote", "remove", REMOTE_NAME);
    } catch {
      // never added
    }
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
  });

  it("shows a failed push and its hook's output from the error's Show details", async () => {
    await expandSidebarSection("Branches");
    const remoteHeader = await $(sidebarRemoteFolder(REMOTE_NAME));
    await remoteHeader.waitForExist({ timeout: SLOW_UI_TIMEOUT });
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
    }, remoteHeader);
    await clickInPage("[role='menu'] button", "Push current branch here");
    // Depending on `push.default`, Push may first ask where to: confirm the suggestion.
    const branch = git("branch", "--show-current");
    const pushDialog = `dialog[aria-label="Push ${branch}"]`;
    await browser.waitUntil(
      () =>
        browser.execute(
          (dialogSelector: string) =>
            document.querySelector(dialogSelector) !== null ||
            Array.from(document.querySelectorAll("[role='alert']")).some((el) =>
              (el.textContent ?? "").includes("The pre-push hook failed."),
            ),
          pushDialog,
        ),
      { timeout: SLOW_UI_TIMEOUT, timeoutMsg: "expected the push to ask where to, or to fail" },
    );
    if (await (await $(pushDialog)).isExisting()) await clickInPage(`${pushDialog} button`, "Push");

    await waitUntilInPage("[role='alert']", "The pre-push hook failed.", { contains: true, timeout: SLOW_UI_TIMEOUT });
    await clickInPage("button", "Show details");

    await waitUntilInPage("dialog[aria-label='Operation log'] section[aria-label='Operation details'] h3", `Push ${REMOTE_NAME}`, {
      timeout: SLOW_UI_TIMEOUT,
    });
    await waitUntilInPage("pre[aria-label='Output of pre-push']", HOOK_OUTPUT, { contains: true });
    // Nothing was pushed.
    expect(gitIn(BARE_REMOTE_PATH)("for-each-ref", "refs/heads")).toBe("");
    await clickInPage("dialog[aria-label='Operation log'] button", "Close");
  });
});
