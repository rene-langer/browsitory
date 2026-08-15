import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-remote.git");
const REMOTE_SOURCE_PATH = path.join(os.tmpdir(), "browsitory-e2e-transfer-source");
const TRANSFER_SEED_FILE = "remote-transfer-seed.txt";
const BRANCH_PUSH_FILE = "branch-push.txt";

describe("Browsitory remote transfer", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, TRANSFER_SEED_FILE), "transfer seed\n");
    execFileSync("git", ["add", TRANSFER_SEED_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: seed transfer base"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    fs.rmSync(REMOTE_SOURCE_PATH, { recursive: true, force: true });
    execFileSync("git", ["init", "--bare", BARE_REMOTE_PATH], { stdio: "inherit" });
    const localBranch = execFileSync("git", ["branch", "--show-current"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim();
    const initialRefspecs = localBranch === "main" ? ["HEAD:main"] : ["HEAD:main", `HEAD:${localBranch}`];
    execFileSync("git", ["push", BARE_REMOTE_PATH, ...initialRefspecs], { cwd: E2E_REPO_PATH, stdio: "inherit" });
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

    const transferPanel = await $("section[aria-label='Transfer progress']");
    await transferPanel.waitForExist({ timeout: 10000 });
    await browser.waitUntil(async () => !(await transferPanel.isExisting()), {
      timeout: 10000,
      timeoutMsg: "expected fetch transfer to complete",
    });
    await expect(fetchButton).toBeEnabled();
  });

  it("selects SSH-agent authentication without rendering an HTTPS token field", async () => {
    await (await $("button=Credentials for transfer-origin")).click();
    const credentialsForm = await $("form[aria-label='Credentials for transfer-origin']");
    await credentialsForm.waitForExist({ timeout: 10000 });
    await credentialsForm.$("select").selectByAttribute("value", "SshAgent");

    expect(await credentialsForm.$("input[type='password']").isExisting()).toBe(false);

    await (await $("button=Use SSH agent")).click();
    await browser.waitUntil(
      () => {
        try {
          return execFileSync(
            "git",
            ["config", "--get", "browsitory.remote.transfer-origin.auth-mode"],
            { cwd: E2E_REPO_PATH, encoding: "utf8" },
          ).trim() === "ssh-agent";
        } catch {
          return false;
        }
      },
      { timeout: 10000, timeoutMsg: "expected SSH-agent mode to be stored without an HTTPS credential" },
    );
    expect(
      execFileSync("git", ["config", "--get-regexp", "^browsitory\\.remote\\.transfer-origin\\."], {
        cwd: E2E_REPO_PATH,
        encoding: "utf8",
      }).trim(),
    ).toBe("browsitory.remote.transfer-origin.auth-mode ssh-agent");
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

  it("pushes the current branch and a local tag", async () => {
    const currentBranch = execFileSync("git", ["branch", "--show-current"], {
      cwd: E2E_REPO_PATH,
      encoding: "utf8",
    }).trim();
    const remoteHeadBeforePush = execFileSync("git", ["rev-parse", `refs/heads/${currentBranch}`], {
      cwd: BARE_REMOTE_PATH,
      encoding: "utf8",
    }).trim();
    fs.writeFileSync(path.join(E2E_REPO_PATH, BRANCH_PUSH_FILE), "branch push\n");
    execFileSync("git", ["add", BRANCH_PUSH_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: branch push change"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    const localHead = execFileSync("git", ["rev-parse", "HEAD"], { cwd: E2E_REPO_PATH, encoding: "utf8" }).trim();
    expect(localHead).not.toBe(remoteHeadBeforePush);

    const pushBranch = await $("button=Push branch to transfer-origin");
    await pushBranch.waitForEnabled({ timeout: 10000 });
    await pushBranch.click();
    const transferPanel = await $("section[aria-label='Transfer progress']");
    await transferPanel.waitForExist({ timeout: 10000 });
    await browser.waitUntil(async () => !(await transferPanel.isExisting()), {
      timeout: 10000,
      timeoutMsg: "expected branch push to complete",
    });
    await browser.waitUntil(
      () => execFileSync("git", ["rev-parse", `refs/heads/${currentBranch}`], { cwd: BARE_REMOTE_PATH, encoding: "utf8" }).trim() === localHead,
      { timeout: 10000, timeoutMsg: "expected Push to advance the remote branch" },
    );

    await (await $("form[aria-label='Create tag'] input")).setValue("e2e-transfer-tag");
    await (await $("button=Create tag")).click();
    const pushTags = await $("button=Push all tags");
    await pushTags.waitForEnabled({ timeout: 10000 });
    await pushTags.click();

    await browser.waitUntil(
      () => {
        try {
          execFileSync("git", ["show-ref", "--verify", "refs/tags/e2e-transfer-tag"], { cwd: BARE_REMOTE_PATH, stdio: "ignore" });
          return true;
        } catch {
          return false;
        }
      },
      { timeout: 10000, timeoutMsg: "expected tag push to complete" },
    );
  });
});
