import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-remote.git");
const REMOTE_SOURCE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-source");
const TRANSFER_SEED_FILE = "remote-transfer-seed.txt";

describe("Browsitory remote transfer", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRANSFER_SEED_FILE), "transfer seed\n");
    execFileSync("git", ["add", TRANSFER_SEED_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: seed transfer base"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    fs.rmSync(REMOTE_SOURCE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_REMOTE_PATH], { stdio: "inherit" });
    execFileSync("git", ["push", BARE_REMOTE_PATH, "HEAD:main"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["clone", "--branch", "main", BARE_REMOTE_PATH, REMOTE_SOURCE_PATH], { stdio: "inherit" });
    execFileSync("git", ["config", "user.name", "Test User"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
    execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(REMOTE_SOURCE_PATH, "remote-change.txt"), "remote change\n");
    execFileSync("git", ["add", "remote-change.txt"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: remote change"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
    execFileSync("git", ["push", "origin", "main"], { cwd: REMOTE_SOURCE_PATH, stdio: "inherit" });
  });

  after(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    fs.rmSync(REMOTE_SOURCE_PATH, { recursive: true, force: true });
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

  it("fast-forwards a clean tracked upstream", async () => {
    const upstreamRemote = await $("form[aria-label='Set upstream'] select");
    await upstreamRemote.selectByAttribute("value", "transfer-origin");
    const upstreamBranch = await $("form[aria-label='Set upstream'] input");
    await upstreamBranch.setValue("main");
    await (await $("button=Set upstream")).click();

    const pullButton = await $("button=Pull");
    await pullButton.waitForEnabled({ timeout: 10000 });
    await pullButton.click();

    const remoteHead = execFileSync("git", ["rev-parse", "refs/heads/main"], {
      cwd: REMOTE_SOURCE_PATH,
      encoding: "utf8",
    }).trim();
    await browser.waitUntil(
      () => execFileSync("git", ["rev-parse", "HEAD"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim() === remoteHead,
      { timeout: 10000, timeoutMsg: "expected Pull to fast-forward the local branch" },
    );
  });
});
