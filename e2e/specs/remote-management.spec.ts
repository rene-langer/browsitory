import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { expandSidebarSection } from "../support/sidebar";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-remote.git");

describe("Browsitory remote management", () => {
  before(() => {
    execFileSync("git", ["add", "README.md"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "--allow-empty", "-m", "e2e: seed remote management branch"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_REMOTE_PATH], { stdio: "inherit" });
  });

  after(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
  });

  it("clears affected upstreams when removing a remote", async () => {
    // Remotes now live inside the unified "Branches" tree (`BranchTree.tsx`), not their own
    // "Remotes" section — it defaults closed; expand it before its Add button exists.
    await expandSidebarSection("Branches");

    // Adding a remote is reached via the tree's "Add" toolbar button (opens a context menu with
    // "New Branch…"/"Add Remote…"), not a standalone "Add remote" toggle.
    const addButton = await $('[aria-label="Add"]');
    await addButton.waitForExist({ timeout: 10000 });
    await addButton.click();
    await (await $("button=Add Remote…")).click();

    const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
    await remoteNameInput.waitForExist({ timeout: 10000 });
    const addRemoteButton = await (await $("form[aria-label='Add remote']")).$("button=Add remote");
    await addRemoteButton.waitForEnabled({ timeout: 10000 });
    await remoteNameInput.setValue("origin");
    const fetchUrlInput = await $("[data-testid='add-remote-fetch-url']");
    await fetchUrlInput.setValue(BARE_REMOTE_PATH);
    await addRemoteButton.click();

    // The new remote renders as its own folder header in the tree, named exactly "origin".
    const remoteFolderHeader = await $("button=origin");
    await remoteFolderHeader.waitForExist({ timeout: 10000 });

    // Setting upstream is a context-menu action on the *current* local branch (its row carries
    // the " (current)" suffix), opening a dialog with the remote/branch fields.
    const currentBranchButton = await $("//button[contains(., ' (current)')]");
    await currentBranchButton.waitForExist({ timeout: 10000 });
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, currentBranchButton);
    await (await $("button=Set upstream…")).click();

    const upstreamDialog = await $("dialog[aria-label^='Set upstream for']");
    await upstreamDialog.waitForExist({ timeout: 10000 });
    const upstreamRemote = await upstreamDialog.$("select");
    await upstreamRemote.selectByAttribute("value", "origin");
    const upstreamBranch = await upstreamDialog.$("input");
    await upstreamBranch.setValue("main");
    await (await upstreamDialog.$("button=Set upstream")).click();

    // The Upstream summary is a plain `<section>` headed "Upstream" (no `aria-labelledby`
    // any more — select it by its heading text instead).
    const upstreamSection = await $("//section[h3[text()='Upstream']]");

    // `onSetUpstream` is an async IPC round-trip; without waiting for `state.upstream` to
    // actually reflect it, the immediate remove-remote click below can race ahead of the
    // mutation (observed as an intermittent failure under load: `requestRemove` reads a stale
    // `remoteUpstreams["origin"]` and skips the "clear upstreams" confirmation entirely, so the
    // confirmation `<dialog>` never appears).
    await browser.waitUntil(
      async () => (await upstreamSection.getText()).includes("tracks origin/main"),
      { timeout: 10000, timeoutMsg: "expected the upstream to be set before removing its remote" },
    );

    // Removing a remote is now a context-menu item ("Remove remote") on the remote's folder
    // header, not a standalone "Remove <name>" icon button.
    await browser.execute((el) => {
      el.dispatchEvent(
        new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 50, clientY: 50 }),
      );
    }, remoteFolderHeader);
    await (await $("button=Remove remote")).click();
    const blockingDialog = await $("dialog[aria-label='Remove remote confirmation']");
    await blockingDialog.waitForExist({ timeout: 10000 });
    expect(await blockingDialog.getText()).toContain("clear upstreams for");
    await (await blockingDialog.$("button=Confirm remove")).click();
    await browser.waitUntil(
      async () => (await upstreamSection.getText()).includes("No upstream"),
      { timeout: 10000, timeoutMsg: "expected removing the remote to clear its upstream" },
    );

    await browser.waitUntil(
      () => !execFileSync("git", ["remote"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).includes("origin"),
      { timeout: 10000, timeoutMsg: "expected origin to be removed after confirmation" },
    );
  });
});
