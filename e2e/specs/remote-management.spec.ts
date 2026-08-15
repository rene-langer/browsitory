import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-remote.git");

describe("Browsitory remote management", () => {
  before(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_REMOTE_PATH], { stdio: "inherit" });
  });

  after(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
  });

  it("blocks removing an upstream remote until the upstream is cleared", async () => {
    const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
    await remoteNameInput.waitForExist({ timeout: 10000 });
    await remoteNameInput.setValue("origin");
    const fetchUrlInput = await $("[data-testid='add-remote-fetch-url']");
    await fetchUrlInput.setValue(BARE_REMOTE_PATH);
    await (await $("button=Add remote")).click();

    await browser.waitUntil(async () => await $("button=Remove origin").isExisting(), {
      timeout: 10000,
      timeoutMsg: "expected the newly added origin remote to appear",
    });

    const upstreamRemote = await $("form[aria-label='Set upstream'] select");
    await upstreamRemote.selectByAttribute("value", "origin");
    const upstreamBranch = await $("form[aria-label='Set upstream'] input");
    await upstreamBranch.setValue("main");
    await (await $("button=Set upstream")).click();

    await (await $("button=Remove origin")).click();
    const blockingDialog = await $("div[role='alertdialog']");
    await blockingDialog.waitForExist({ timeout: 10000 });
    expect(await blockingDialog.getText()).toContain("Clear");

    await (await $("div[role='alertdialog'] button=Clear upstream")).click();
    await browser.waitUntil(
      async () => (await (await $("section[aria-labelledby='upstream-heading']")).getText()).includes("No upstream"),
      { timeout: 10000, timeoutMsg: "expected the refreshed upstream state to be cleared" },
    );
    await browser.waitUntil(async () => !(await $("div[role='alertdialog']")).isExisting(), {
      timeout: 10000,
      timeoutMsg: "expected the upstream-clear confirmation to close",
    });

    await (await $("button=Remove origin")).click();
    await (await $("div[role='alertdialog'] button=Confirm remove")).click();

    await browser.waitUntil(
      () => !execFileSync("git", ["remote"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).includes("origin"),
      { timeout: 10000, timeoutMsg: "expected origin to be removed after confirmation" },
    );
  });
});
