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
    // "Remotes" defaults closed; expand it before its Add remote button exists.
    await expandSidebarSection("Remotes");

    // The Add-remote form is gated behind a button — open it before reaching for its fields.
    await (await $("button=Add remote")).click();

    const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
    await remoteNameInput.waitForExist({ timeout: 10000 });
    const addRemoteButton = await (await $("form[aria-label='Add remote']")).$("button=Add remote");
    await addRemoteButton.waitForEnabled({ timeout: 10000 });
    await remoteNameInput.setValue("origin");
    const fetchUrlInput = await $("[data-testid='add-remote-fetch-url']");
    await fetchUrlInput.setValue(BARE_REMOTE_PATH);
    await addRemoteButton.click();

    await browser.waitUntil(async () => await $("aria/Remove origin").isExisting(), {
      timeout: 10000,
      timeoutMsg: "expected the newly added origin remote to appear",
    });

    const upstreamRemote = await $("form[aria-label='Set upstream'] select");
    await upstreamRemote.selectByAttribute("value", "origin");
    const upstreamBranch = await $("form[aria-label='Set upstream'] input");
    await upstreamBranch.setValue("main");
    await (await $("button=Set upstream")).click();

    // `onSetUpstream` is an async IPC round-trip; without waiting for `state.upstream` to
    // actually reflect it, the immediate "Remove origin" click below can race ahead of the
    // mutation (observed as an intermittent failure under load: `requestRemove` reads a stale
    // `remoteUpstreams["origin"]` and skips the "clear upstreams" confirmation entirely, so the
    // confirmation `<dialog>` never appears).
    await browser.waitUntil(
      async () => (await (await $("section[aria-labelledby='upstream-heading']")).getText()).includes("tracks origin/main"),
      { timeout: 10000, timeoutMsg: "expected the upstream to be set before removing its remote" },
    );

    await (await $("aria/Remove origin")).click();
    const blockingDialog = await $("dialog[aria-label='Remove remote confirmation']");
    await blockingDialog.waitForExist({ timeout: 10000 });
    expect(await blockingDialog.getText()).toContain("clear upstreams for");
    await (await blockingDialog.$("button=Confirm remove")).click();
    await browser.waitUntil(
      async () => (await (await $("section[aria-labelledby='upstream-heading']")).getText()).includes("No upstream"),
      { timeout: 10000, timeoutMsg: "expected removing the remote to clear its upstream" },
    );

    await browser.waitUntil(
      () => !execFileSync("git", ["remote"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).includes("origin"),
      { timeout: 10000, timeoutMsg: "expected origin to be removed after confirmation" },
    );
  });
});
