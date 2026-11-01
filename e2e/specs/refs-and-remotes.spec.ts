import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { waitUntilGoneInPage } from "../support/dom";
import { expandSidebarSection } from "../support/sidebar";

// Owns everything it touches: the remote `publish-origin`, the local branch `e2e-publish` and
// the bare directory. It never uses Fetch all (other specs may leave unreachable remotes in the
// shared repo) and selects its remote explicitly (the shared repo already has `origin`).
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_PATH = path.join(os.tmpdir(), "browsitory-e2e-publish-remote.git");
const REMOTE = "publish-origin";
const BRANCH = "e2e-publish";

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}

function bareHas(branch: string): boolean {
  try {
    git(BARE_PATH, "rev-parse", "--verify", "--quiet", `refs/heads/${branch}`);
    return true;
  } catch {
    return false;
  }
}

function localHas(ref: string): boolean {
  try {
    git(E2E_REPO_PATH, "rev-parse", "--verify", "--quiet", ref);
    return true;
  } catch {
    return false;
  }
}

// Right-click via a synthetic DOM event (see rebase.spec.ts on why not `.click({ button: "right" })`).
async function contextMenu(selector: string) {
  const element = await $(selector);
  await element.waitForExist({ timeout: 10000 });
  await browser.execute((el) => {
    el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
  }, element);
}

describe("Browsitory refs and remotes", () => {
  before(() => {
    // Order-independent: some earlier spec may or may not have committed yet.
    git(E2E_REPO_PATH, "commit", "--allow-empty", "-m", "e2e: seed for publish");
    fs.rmSync(BARE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_PATH], { stdio: "ignore" });
    try {
      git(E2E_REPO_PATH, "remote", "remove", REMOTE);
    } catch {
      // not there from an aborted earlier run
    }
    git(E2E_REPO_PATH, "remote", "add", REMOTE, BARE_PATH);
    git(E2E_REPO_PATH, "branch", "-f", BRANCH, "HEAD");
  });

  after(() => {
    try {
      git(E2E_REPO_PATH, "remote", "remove", REMOTE); // also drops branch.<x>.remote/merge pointing at it
    } catch {
      // already gone
    }
    try {
      git(E2E_REPO_PATH, "branch", "-D", BRANCH);
    } catch {
      // already gone
    }
    fs.rmSync(BARE_PATH, { recursive: true, force: true });
  });

  it("publishes a new branch with Push to… and deletes it on the remote", async () => {
    await expandSidebarSection("Branches");
    const pushedTip = git(E2E_REPO_PATH, "rev-parse", BRANCH);

    // Local row for e2e-publish, found by its unique swatch label (remote rows say "<remote>/<name>").
    await contextMenu(`//button[@aria-label="Show ${BRANCH} in graph"]/ancestor::li[1]`);
    const pushTo = await $("button=Push to…");
    await pushTo.waitForEnabled({ timeout: 10000 });
    await pushTo.click();

    const dialog = await $(`dialog[aria-label="Push ${BRANCH}"]`);
    await dialog.waitForExist({ timeout: 10000 });
    const remoteSelect = await dialog.$("select");
    await browser.execute(
      (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value")?.set;
        setter?.call(el, value);
        el.dispatchEvent(new Event("change", { bubbles: true }));
      },
      remoteSelect,
      REMOTE,
    );
    await expect(remoteSelect).toHaveValue(REMOTE);
    await expect(await dialog.$("input[type='checkbox']")).toBeSelected();
    await (await dialog.$("button=Push")).click();

    await browser.waitUntil(() => bareHas(BRANCH), {
      timeout: 15000,
      timeoutMsg: "expected the bare remote to receive the published branch",
    });
    expect(git(BARE_PATH, "rev-parse", `refs/heads/${BRANCH}`)).toBe(pushedTip);
    await browser.waitUntil(
      () => {
        try {
          return (
            git(E2E_REPO_PATH, "config", `branch.${BRANCH}.remote`) === REMOTE &&
            git(E2E_REPO_PATH, "config", `branch.${BRANCH}.merge`) === `refs/heads/${BRANCH}`
          );
        } catch {
          return false;
        }
      },
      { timeout: 10000, timeoutMsg: "expected Set as upstream to record the upstream" },
    );

    // Expand the remote folder and delete the branch from its row's menu.
    const remoteFolder = await $(`button=${REMOTE}`);
    await remoteFolder.waitForExist({ timeout: 10000 });
    await remoteFolder.click();
    await contextMenu(`//button[@aria-label="Show ${REMOTE}/${BRANCH} in graph"]/ancestor::li[1]`);
    const deleteItem = await $(`button=Delete ${REMOTE}/${BRANCH}…`);
    await deleteItem.waitForEnabled({ timeout: 10000 });
    await deleteItem.click();

    const confirm = await $(`dialog[aria-label="Delete remote branch"]`);
    await confirm.waitForExist({ timeout: 10000 });
    await expect(confirm).toHaveText(expect.stringContaining(`Delete ${REMOTE}/${BRANCH} on the remote?`));
    await (await confirm.$("button=Delete remote branch")).click();

    await browser.waitUntil(() => !bareHas(BRANCH) && !localHas(`refs/remotes/${REMOTE}/${BRANCH}`), {
      timeout: 15000,
      timeoutMsg: "expected the remote branch and its tracking ref to be deleted",
    });
    // The open remote folder reloads after the refresh, so the row disappears in place.
    await waitUntilGoneInPage(`button[aria-label="Show ${REMOTE}/${BRANCH} in graph"]`);
    // The local branch stays, still configured to track the now-gone remote branch.
    expect(localHas(`refs/heads/${BRANCH}`)).toBe(true);
  });
});
