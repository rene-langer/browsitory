import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-remote.git");

describe("Browsitory remote transfer", () => {
  before(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_REMOTE_PATH], { stdio: "inherit" });
  });

  after(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
  });

  it("fetches a configured remote", async () => {
    const remoteNameInput = await $("form[aria-label='Add remote'] input:nth-of-type(1)");
    await remoteNameInput.waitForExist({ timeout: 10000 });
    await remoteNameInput.setValue("transfer-origin");
    await (await $("[data-testid='add-remote-fetch-url']")).setValue(BARE_REMOTE_PATH);
    await (await $("button=Add remote")).click();

    const fetchButton = await $("button=Fetch transfer-origin");
    await fetchButton.waitForExist({ timeout: 10000 });
    await fetchButton.click();

    const transferPanel = await $("section[aria-label='Fetch progress']");
    await transferPanel.waitForExist({ timeout: 10000 });
    await browser.waitUntil(async () => !(await transferPanel.isExisting()), {
      timeout: 10000,
      timeoutMsg: "expected fetch transfer to complete",
    });
    await expect(fetchButton).toBeEnabled();
  });
});
