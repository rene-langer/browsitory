import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect } from "@wdio/globals";
import { reloadSessionWithRetry } from "../support/session";

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

/**
 * Processes other than this one that may still write under the data home: any process with a
 * file open below it, and any WebKit helper (network or web process) started for this run (its
 * environment has this `XDG_DATA_HOME`). A helper can outlive the app binary `appIsRunning`
 * looks for and recreate the data folder after the spec moved it (#465). As "pid command".
 */
function dataHomeProcesses(): string[] {
  const found: string[] = [];
  for (const pid of fs.readdirSync("/proc")) {
    if (!/^\d+$/.test(pid) || Number(pid) === process.pid) continue;
    try {
      const cmdline = fs.readFileSync(`/proc/${pid}/cmdline`, "utf8").split("\0").join(" ").trim();
      let holds = false;
      try {
        holds = fs
          .readdirSync(`/proc/${pid}/fd`)
          .some((fd) => {
            try {
              return fs.readlinkSync(`/proc/${pid}/fd/${fd}`).startsWith(E2E_DATA_HOME);
            } catch {
              return false;
            }
          });
      } catch {
        // Not ours to read, or gone.
      }
      // WebKitWebDriver (tauri-driver's native driver) serves every session and stays.
      if (!holds && cmdline.includes("WebKit") && !cmdline.includes("WebKitWebDriver")) {
        const environ = fs.readFileSync(`/proc/${pid}/environ`, "utf8").split("\0");
        holds = environ.includes(`XDG_DATA_HOME=${E2E_DATA_HOME}`);
      }
      if (holds) found.push(`${pid} ${cmdline}`);
    } catch {
      // The process exited, or isn't ours to read.
    }
  }
  return found;
}

/** Every file under `dir`, recursively (none when it doesn't exist). */
function filesUnder(dir: string): string[] {
  let entries: fs.Dirent[];
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return []; // Missing, or removed while we looked.
  }
  return entries.flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return filesUnder(full);
    return entry.isFile() ? [full] : [];
  });
}

/**
 * The files under `dir` that hold `PROBE_KEY`, relative to `dir`. WebKitGTK keeps localStorage in
 * SQLite databases (and their `-wal` journals) below the data folder; the key may be stored as
 * UTF-8 or UTF-16, so both are searched.
 */
function filesWithProbe(dir: string): string[] {
  const needles = [Buffer.from(PROBE_KEY, "utf8"), Buffer.from(PROBE_KEY, "utf16le")];
  return filesUnder(dir)
    .filter((file) => {
      try {
        const contents = fs.readFileSync(file);
        return needles.some((needle) => contents.includes(needle));
      } catch {
        return false; // Removed or replaced while we looked.
      }
    })
    .map((file) => path.relative(dir, file));
}

/** For failure messages: the files under `dir`, relative to it. */
function listing(dir: string): string {
  const files = filesUnder(dir).map((file) => path.relative(dir, file));
  return files.length === 0 ? "(no files)" : files.join(", ");
}

// #433: the bundle identifier changed from com.browsitory.app to com.browsitory.desktop. The first
// start of the new version renames the old folder, so the webview's localStorage (theme, panel
// sizes, ...) and the logs carry over. The test turns this run's folder into a 0.3.x one while
// the app is closed and restarts the app on it.
describe("Browsitory identifier migration", () => {
  it("moves the 0.3.x data folder to the new identifier and keeps localStorage", async function () {
    // The restart may take two bounded session attempts (#457), on top of the 20 s waits.
    this.timeout(120000);
    await browser.execute((key) => window.localStorage.setItem(key, "kept"), PROBE_KEY);
    // WebKitGTK writes localStorage to disk asynchronously, and ending the session stops the
    // app: without this wait the write can be lost before the folder is moved (#465).
    await browser.waitUntil(async () => filesWithProbe(NEW_DIR).length > 0, {
      timeout: 20000,
      interval: 200,
      timeoutMsg: `the localStorage probe never reached the disk; files under ${NEW_DIR}: ${listing(NEW_DIR)}`,
    });

    await browser.deleteSession();
    await browser.waitUntil(async () => !appIsRunning(), {
      timeout: 20000,
      timeoutMsg: "the app did not exit after the session ended",
    });
    // Its WebKit helpers too: one that is still running can recreate the folder (#465).
    await browser.waitUntil(async () => dataHomeProcesses().length === 0, {
      timeout: 20000,
      interval: 200,
      timeoutMsg: `processes still use ${E2E_DATA_HOME} after the app exited: ${dataHomeProcesses().join("; ")}`,
    });
    expect(fs.existsSync(NEW_DIR)).toBe(true);
    fs.rmSync(OLD_DIR, { recursive: true, force: true });
    fs.renameSync(NEW_DIR, OLD_DIR);
    // Nothing may bring the new folder back before the app starts again: the migration would
    // then find both folders and keep the new one.
    const settleUntil = Date.now() + 2000;
    while (Date.now() < settleUntil) {
      if (fs.existsSync(NEW_DIR)) {
        throw new Error(
          `${NEW_DIR} reappeared after it was moved, before the app restarted; files under it: ` +
            `${listing(NEW_DIR)}; processes using the data home: [${dataHomeProcesses().join("; ")}]`,
        );
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    const probeFilesBefore = filesWithProbe(OLD_DIR);
    if (probeFilesBefore.length === 0) {
      throw new Error(`the localStorage probe is gone from disk after the app exited; files under ${OLD_DIR}: ${listing(OLD_DIR)}`);
    }

    // reloadSession's own deleteSession fails on the ended session (WebdriverIO only logs it) and
    // then starts a fresh app process.
    await reloadSessionWithRetry();

    let seen: unknown = null;
    await browser
      .waitUntil(
        async () => {
          seen = await browser.execute((key) => window.localStorage.getItem(key), PROBE_KEY);
          return seen === "kept";
        },
        { timeout: 20000 },
      )
      .catch(() => {
        throw new Error(
          "localStorage did not carry over to the new identifier. " +
            `The new session read ${JSON.stringify(seen)}; before the restart the probe was in ` +
            `${probeFilesBefore.join(", ")} under the old folder; the old folder is ` +
            `${fs.existsSync(OLD_DIR) ? "still there" : "gone"}; the probe is now in ` +
            `[${filesWithProbe(NEW_DIR).join(", ")}]; files under ${NEW_DIR}: ${listing(NEW_DIR)}; ` +
            `the new folder did not reappear for 2 s after the move`,
        );
      });
    expect(fs.existsSync(OLD_DIR)).toBe(false);
    expect(fs.existsSync(path.join(NEW_DIR, "logs", "browsitory.log"))).toBe(true);
    const log = fs.readFileSync(path.join(NEW_DIR, "logs", "browsitory.log"), "utf8");
    expect(log).toContain(`moved ${OLD_DIR} to ${NEW_DIR}`);
  });
});
