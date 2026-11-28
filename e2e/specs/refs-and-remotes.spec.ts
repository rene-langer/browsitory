import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { withFailureDiagnostics } from "../support/diagnostics";
import { waitUntilGoneInPage } from "../support/dom";
import { expandSidebarSection, sidebarRemoteFolder } from "../support/sidebar";
import { git, gitWith } from "../support/git";
import { SLOW_UI_TIMEOUT } from "../support/timeouts";

// Owns everything it touches: the remote `publish-origin`, the local branch `e2e-publish` and
// the bare directory. It never uses Fetch all (other specs may leave unreachable remotes in the
// shared repo) and selects its remote explicitly (the shared repo already has `origin`).
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_PATH = path.join(os.tmpdir(), "browsitory-e2e-publish-remote.git");
const REMOTE = "publish-origin";
const BRANCH = "e2e-publish";

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

/** The remote folder as the page shows it, for a failure message: open, its rows, an error. */
async function remoteFolderState(remote: string): Promise<unknown> {
  return browser.execute((name) => {
    const header = Array.from(document.querySelectorAll("[data-active-repo='true'] button[aria-expanded]")).find(
      (button) => button.nextElementSibling?.getAttribute("aria-label") === `Actions for remote ${name}`,
    );
    const folder = header?.closest("li");
    const section = header?.closest("section");
    return {
      expanded: header?.getAttribute("aria-expanded") ?? null,
      rows: (folder?.querySelector("ul")?.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 300),
      alerts: Array.from(section?.querySelectorAll("[role='alert']") ?? []).map((alert) => alert.textContent),
      activity: document.querySelector("[data-active-repo='true'] [role='group'][aria-label='Repository']")?.textContent,
    };
  }, remote);
}

// Right-click via a synthetic DOM event (see rebase.spec.ts on why not `.click({ button: "right" })`).
async function contextMenu(selector: string) {
  const element = await $(selector);
  await element.waitForExist();
  await browser.execute((el) => {
    el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }));
  }, element);
}

describe("Browsitory refs and remotes", () => {
  before(() => {
    // Order-independent: some earlier spec may or may not have committed yet.
    git(E2E_REPO_PATH, "commit", "--allow-empty", "-m", "e2e: seed for publish");
    fs.rmSync(BARE_PATH, { recursive: true, force: true });
    gitWith(os.tmpdir(), ["init", "--bare", BARE_PATH], { stdio: "ignore" });
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
    await pushTo.waitForEnabled();
    await pushTo.click();

    const dialog = await $(`dialog[aria-label="Push ${BRANCH}"]`);
    await dialog.waitForExist();
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
      timeout: SLOW_UI_TIMEOUT,
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
      { timeoutMsg: "expected Set as upstream to record the upstream" },
    );

    // The git CLI sees the push land before the app has finished it: its toolbar still shows
    // the push, and its end moves the sidebar under a click (#1188). Go on once the app says
    // it's done, as a user would.
    await browser.waitUntil(
      async () =>
        (await browser.execute(() => document.body.textContent ?? "")).includes(
          `Pushed ${BRANCH} to ${REMOTE}/${BRANCH} and set it as the upstream.`,
        ),
      { timeoutMsg: "expected the app to report the push" },
    );

    // Expand the remote folder and delete the branch from its row's menu. The click is repeated
    // until the folder says it's open: a click while the page moves can land beside it (#824).
    const remoteFolder = await $(sidebarRemoteFolder(REMOTE));
    await remoteFolder.waitForExist();
    await browser.waitUntil(
      async () => {
        if ((await remoteFolder.getAttribute("aria-expanded")) === "true") return true;
        await remoteFolder.click();
        return (await remoteFolder.getAttribute("aria-expanded")) === "true";
      },
      { interval: 250, timeoutMsg: `expected the ${REMOTE} folder to open` },
    );
    await withFailureDiagnostics(
      "remote-tracking-row",
      () => $(`//button[@aria-label="Show ${REMOTE}/${BRANCH} in graph"]`).waitForExist(),
      () => remoteFolderState(REMOTE),
    );
    await contextMenu(`//button[@aria-label="Show ${REMOTE}/${BRANCH} in graph"]/ancestor::li[1]`);
    const deleteItem = await $(`button=Delete ${REMOTE}/${BRANCH}…`);
    await deleteItem.waitForEnabled();
    await deleteItem.click();

    const confirm = await $(`dialog[aria-label="Delete remote branch"]`);
    await confirm.waitForExist();
    await expect(confirm).toHaveText(expect.stringContaining(`Delete ${REMOTE}/${BRANCH} on the remote?`));
    await (await confirm.$("button=Delete remote branch")).click();

    await browser.waitUntil(() => !bareHas(BRANCH) && !localHas(`refs/remotes/${REMOTE}/${BRANCH}`), {
      timeout: SLOW_UI_TIMEOUT,
      timeoutMsg: "expected the remote branch and its tracking ref to be deleted",
    });
    // The open remote folder reloads after the refresh, so the row disappears in place.
    await waitUntilGoneInPage(`button[aria-label="Show ${REMOTE}/${BRANCH} in graph"]`);
    // The local branch stays, still configured to track the now-gone remote branch.
    expect(localHas(`refs/heads/${BRANCH}`)).toBe(true);
  });
});
