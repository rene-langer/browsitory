import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";

// The app's webview storage and logs live in `$XDG_DATA_HOME/<identifier>` (wdio.conf.ts points
// XDG_DATA_HOME at this throwaway dir).
const E2E_DATA_HOME = path.join(os.tmpdir(), "browsitory-e2e-data-home");
const OLD_DIR = path.join(E2E_DATA_HOME, "com.browsitory.app");
const NEW_DIR = path.join(E2E_DATA_HOME, "com.browsitory.desktop");
const APP_BINARY = fileURLToPath(new URL("../../target/debug/tauri-app", import.meta.url));
const PROBE_KEY = "browsitory-e2e-identifier-migration-probe";

/** Whether a process started from the app binary under test is still running. */
function appIsRunning(): boolean {
  for (const pid of fs.readdirSync("/proc")) {
    if (!/^\d+$/.test(pid)) continue;
    try {
      const argv0 = fs.readFileSync(`/proc/${pid}/cmdline`, "utf8").split("\0")[0];
      if (argv0 === APP_BINARY) return true;
    } catch {
      // The process exited while we looked.
    }
  }
  return false;
}

// #433: the bundle identifier changed from com.browsitory.app to com.browsitory.desktop. The first
// start of the new version renames the old folder, so the webview's localStorage (theme, panel
// sizes, ...) and the logs carry over. The test turns this run's folder into a 0.3.x one while
// the app is closed and restarts the app on it.
describe("Browsitory identifier migration", () => {
  it("moves the 0.3.x data folder to the new identifier and keeps localStorage", async () => {
    await browser.execute((key) => window.localStorage.setItem(key, "kept"), PROBE_KEY);

    await browser.deleteSession();
    await browser.waitUntil(async () => !appIsRunning(), {
      timeout: 20000,
      timeoutMsg: "the app did not exit after the session ended",
    });
    expect(fs.existsSync(NEW_DIR)).toBe(true);
    fs.rmSync(OLD_DIR, { recursive: true, force: true });
    fs.renameSync(NEW_DIR, OLD_DIR);

    // reloadSession's own deleteSession fails on the ended session (WebdriverIO only logs it) and
    // then starts a fresh app process.
    await browser.reloadSession();

    await browser.waitUntil(
      async () => (await browser.execute((key) => window.localStorage.getItem(key), PROBE_KEY)) === "kept",
      { timeout: 20000, timeoutMsg: "localStorage did not carry over to the new identifier" },
    );
    expect(fs.existsSync(OLD_DIR)).toBe(false);
    expect(fs.existsSync(path.join(NEW_DIR, "logs", "browsitory.log"))).toBe(true);
    const log = fs.readFileSync(path.join(NEW_DIR, "logs", "browsitory.log"), "utf8");
    expect(log).toContain(`moved ${OLD_DIR} to ${NEW_DIR}`);
  });
});
