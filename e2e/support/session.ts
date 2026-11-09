import { browser } from "@wdio/globals";

// `browser.reloadSession()` with a bound and one retry (#457).
//
// Restarting the app is `reloadSession()`: tauri-driver ends the session (closing the app) and
// a new `POST /session` launches a fresh app. On a busy machine that request was seen to hang
// (`UND_ERR_SOCKET`); WebdriverIO then retries it up to 3 times with 120 s each, far past
// mocha's timeout. Here each attempt gets one request with a bounded timeout. If it fails, the
// whole tauri-driver process group (tauri-driver, WebKitWebDriver and any half-started app) is
// restarted by wdio.conf.ts, and the session is requested once more.

// wdio.conf.ts registers its driver restart here. A global (not a module variable), so the
// config and the specs reach the same function even if the module is loaded twice.
const RESTART_DRIVER = Symbol.for("browsitory.e2e.restartTauriDriver");
type Registry = { [RESTART_DRIVER]?: () => Promise<void> };

export function registerDriverRestart(restart: () => Promise<void>): void {
  (globalThis as Registry)[RESTART_DRIVER] = restart;
}

/** Restarts the app under test through a new WebDriver session, bounded, with one retry. */
export async function reloadSessionWithRetry(attemptTimeoutMs = 30000): Promise<void> {
  const options = browser.options as { connectionRetryTimeout?: number; connectionRetryCount?: number };
  const saved = { timeout: options.connectionRetryTimeout, count: options.connectionRetryCount };
  options.connectionRetryTimeout = attemptTimeoutMs;
  options.connectionRetryCount = 0;
  try {
    for (let attempt = 1; ; attempt++) {
      const startedAt = Date.now();
      try {
        await browser.reloadSession();
        console.log(`[e2e] reloadSession attempt ${attempt}: session ${browser.sessionId} in ${Date.now() - startedAt} ms`);
        return;
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        console.warn(`[e2e] reloadSession attempt ${attempt} failed after ${Date.now() - startedAt} ms: ${message}`);
        const restartDriver = (globalThis as Registry)[RESTART_DRIVER];
        if (attempt >= 2 || !restartDriver) throw error;
        await restartDriver();
      }
    }
  } finally {
    if (saved.timeout === undefined) delete options.connectionRetryTimeout;
    else options.connectionRetryTimeout = saved.timeout;
    if (saved.count === undefined) delete options.connectionRetryCount;
    else options.connectionRetryCount = saved.count;
  }
}
