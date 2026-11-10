import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";
import { git } from "../support/git";

// "Fetch automatically" (#440): the Preferences setting, then a background fetch against a local
// bare remote. The suite's config.toml (`wdio.conf.ts`, BROWSITORY_CONFIG_DIR) is shared by every
// spec file, so this one puts the setting back to off when it is done.
const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const CONFIG_FILE = path.join(os.tmpdir(), "browsitory-e2e-config", "config.toml");
const BARE_REMOTE_PATH = path.join(os.tmpdir(), "browsitory-e2e-autofetch-remote.git");
const REMOTE_SOURCE_PATH = path.join(os.tmpdir(), "browsitory-e2e-autofetch-source");
const REMOTE_NAME = "autofetch";
// The first background fetch runs a minute after the page loaded.
const AUTO_FETCH_TIMEOUT = 120_000;

function remoteTrackingTip(): string | null {
  try {
    return git(E2E_REPO_PATH, "rev-parse", "--verify", "--quiet", `refs/remotes/${REMOTE_NAME}/main`);
  } catch {
    return null;
  }
}

function setIntervalInConfig(minutes: number) {
  const text = fs.readFileSync(CONFIG_FILE, "utf8");
  fs.writeFileSync(CONFIG_FILE, text.replace(/auto_fetch_minutes = \d+/, `auto_fetch_minutes = ${minutes}`));
}

const bodyText = () => browser.execute(() => document.body.textContent ?? "");

describe("Browsitory fetch automatically", () => {
  before(() => {
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    fs.rmSync(REMOTE_SOURCE_PATH, { recursive: true, force: true });
    git(os.tmpdir(), "init", "--bare", BARE_REMOTE_PATH);
    git(E2E_REPO_PATH, "push", BARE_REMOTE_PATH, "HEAD:refs/heads/main");
    git(os.tmpdir(), "clone", "--branch", "main", BARE_REMOTE_PATH, REMOTE_SOURCE_PATH);
    git(REMOTE_SOURCE_PATH, "config", "user.name", "Test User");
    git(REMOTE_SOURCE_PATH, "config", "user.email", "test@example.com");
    fs.writeFileSync(path.join(REMOTE_SOURCE_PATH, "auto-fetched.txt"), "from the remote\n");
    git(REMOTE_SOURCE_PATH, "add", "auto-fetched.txt");
    git(REMOTE_SOURCE_PATH, "commit", "-m", "e2e: only on the auto-fetch remote");
    git(REMOTE_SOURCE_PATH, "push", "origin", "main");
    // Never fetched: the background fetch is what creates refs/remotes/autofetch/main.
    git(E2E_REPO_PATH, "remote", "add", REMOTE_NAME, BARE_REMOTE_PATH);
  });

  after(() => {
    if (fs.existsSync(CONFIG_FILE)) setIntervalInConfig(0);
    try {
      git(E2E_REPO_PATH, "remote", "remove", REMOTE_NAME);
    } catch {
      // already gone
    }
    fs.rmSync(BARE_REMOTE_PATH, { recursive: true, force: true });
    fs.rmSync(REMOTE_SOURCE_PATH, { recursive: true, force: true });
  });

  it("saves the Fetch automatically interval in Preferences, off by default", async () => {
    const preferences = await $("header button[aria-label='Preferences']");
    await preferences.waitForClickable();
    await preferences.click();
    const select = await $("//select[@id=//label[normalize-space(.)='Fetch automatically']/@for]");
    await select.waitForExist();
    expect(await select.getValue()).toBe("0");

    await select.selectByVisibleText("Every 5 minutes");

    await browser.waitUntil(() => /\[fetch\]\nauto_fetch_minutes = 5\n/.test(fs.readFileSync(CONFIG_FILE, "utf8")), {
      timeoutMsg: "the interval did not reach config.toml",
    });
    await (await $("button=Close")).click();
    expect(remoteTrackingTip()).toBeNull();
    // The shortest interval config.toml accepts, so the next test (after the reload every test
    // starts with) doesn't wait five minutes.
    setIntervalInConfig(1);
  });

  it("fetches every remote in the background and shows when", async function () {
    // The first background fetch runs a minute after the page loaded.
    this.timeout(180000);
    const remoteTip = git(REMOTE_SOURCE_PATH, "rev-parse", "main");

    await browser.waitUntil(() => remoteTrackingTip() === remoteTip, {
      timeout: AUTO_FETCH_TIMEOUT,
      interval: 1000,
      timeoutMsg: "the background fetch did not fetch the remote within two minutes",
    });
    await browser.waitUntil(async () => (await bodyText()).includes("Fetched just now"), {
      timeoutMsg: "the sync bar did not show the last fetch",
    });
    // Quiet: no progress in the toolbar, no "Fetched from …" toast, nothing to undo.
    const text = await bodyText();
    expect(text).not.toContain("Fetched from");
    expect(await $("button[aria-label^='Undo Fetch']").isExisting()).toBe(false);
  });
});
