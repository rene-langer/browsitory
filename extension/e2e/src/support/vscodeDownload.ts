import { downloadAndUnzipVSCode } from "@vscode/test-electron";

// Downloads (or finds in `.vscode-test`) the VSCode that the E2E suite (`runTests.ts`) and the
// packaged-VSIX smoke test (`runVsixSmoke.ts`) run in (#918).
//
// CI pins the version with BROWSITORY_E2E_VSCODE_VERSION (`.github/workflows/ci.yml`, kept
// current by Renovate) and caches `.vscode-test/vscode-<platform>-<version>`, so a run with a
// warm cache makes no request at all. Without the variable (a local run), test-electron picks
// the latest stable release that satisfies the extension's `engines.vscode`.
//
// test-electron asks the update service for its version list (to resolve "stable", or to
// check a pinned version it hasn't downloaded yet) with a 15 s timeout and no retry; that
// request timing out failed unrelated pull requests. So each attempt here gets a longer
// timeout, and a failed attempt is retried after a growing pause.

export const VSCODE_VERSION_ENV = "BROWSITORY_E2E_VSCODE_VERSION";

const DOWNLOAD_ATTEMPTS = 4;
const FIRST_RETRY_DELAY_MS = 5_000;
const REQUEST_TIMEOUT_MS = 45_000;

export interface RetryOptions {
  attempts: number;
  // The pause before the second attempt; it doubles before each one after that.
  initialDelayMs: number;
  sleep?: (ms: number) => Promise<void>;
  onRetry?: (attempt: number, error: unknown, delayMs: number) => void;
}

const defaultSleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

// Runs `run` (passing the 1-based attempt number) until it resolves or `attempts` tries have
// failed; then rejects with the last error.
export async function withRetry<T>(run: (attempt: number) => Promise<T>, options: RetryOptions): Promise<T> {
  const { attempts, initialDelayMs, sleep = defaultSleep, onRetry } = options;
  if (!(attempts >= 1)) throw new RangeError(`attempts must be at least 1, got ${attempts}`);
  for (let attempt = 1; ; attempt++) {
    try {
      return await run(attempt);
    } catch (error) {
      if (attempt >= attempts) throw error;
      const delayMs = initialDelayMs * 2 ** (attempt - 1);
      onRetry?.(attempt, error, delayMs);
      await sleep(delayMs);
    }
  }
}

export function pinnedVSCodeVersion(env: NodeJS.ProcessEnv): string | undefined {
  const version = env[VSCODE_VERSION_ENV]?.trim();
  return version ? version : undefined;
}

// Returns the VSCode executable path. `extensionDevelopmentPath` lets an unpinned run pick a
// version that satisfies that extension's `engines.vscode`.
export function downloadVSCode(extensionDevelopmentPath?: string): Promise<string> {
  const version = pinnedVSCodeVersion(process.env);
  return withRetry(
    () => downloadAndUnzipVSCode({ version, extensionDevelopmentPath, timeout: REQUEST_TIMEOUT_MS }),
    {
      attempts: DOWNLOAD_ATTEMPTS,
      initialDelayMs: FIRST_RETRY_DELAY_MS,
      onRetry: (attempt, error, delayMs) =>
        console.warn(
          `VSCode download attempt ${attempt}/${DOWNLOAD_ATTEMPTS} failed, retrying in ${delayMs / 1000} s:`,
          error,
        ),
    },
  );
}
