// Unit tests for the harness's own helpers (node:test, not the VSCode suite):
//   pnpm --ignore-workspace test:unit
import assert from "node:assert/strict";
import { test } from "node:test";
import { pinnedVSCodeVersion, withRetry } from "./vscodeDownload";

function recordingSleep(): { delays: number[]; sleep: (ms: number) => Promise<void> } {
  const delays: number[] = [];
  return { delays, sleep: async (ms) => void delays.push(ms) };
}

test("withRetry returns the first success without waiting", async () => {
  const { delays, sleep } = recordingSleep();
  let calls = 0;
  const result = await withRetry(
    async () => {
      calls += 1;
      return "ok";
    },
    { attempts: 4, initialDelayMs: 1000, sleep },
  );
  assert.equal(result, "ok");
  assert.equal(calls, 1);
  assert.deepEqual(delays, []);
});

test("withRetry retries a failure with doubling delays until it succeeds", async () => {
  const { delays, sleep } = recordingSleep();
  const seen: number[] = [];
  const result = await withRetry(
    async (attempt) => {
      seen.push(attempt);
      if (attempt < 3) throw new Error(`timeout ${attempt}`);
      return attempt;
    },
    { attempts: 4, initialDelayMs: 1000, sleep },
  );
  assert.equal(result, 3);
  assert.deepEqual(seen, [1, 2, 3]);
  assert.deepEqual(delays, [1000, 2000]);
});

test("withRetry throws the last error after the last attempt, without a final wait", async () => {
  const { delays, sleep } = recordingSleep();
  let calls = 0;
  await assert.rejects(
    withRetry(
      async (attempt) => {
        calls += 1;
        throw new Error(`timeout ${attempt}`);
      },
      { attempts: 3, initialDelayMs: 500, sleep },
    ),
    { message: "timeout 3" },
  );
  assert.equal(calls, 3);
  assert.deepEqual(delays, [500, 1000]);
});

test("withRetry reports each retry with its attempt, error and delay", async () => {
  const { sleep } = recordingSleep();
  const reports: [number, string, number][] = [];
  await withRetry(
    async (attempt) => {
      if (attempt === 1) throw new Error("timeout");
      return undefined;
    },
    {
      attempts: 2,
      initialDelayMs: 250,
      sleep,
      onRetry: (attempt, error, delayMs) => reports.push([attempt, (error as Error).message, delayMs]),
    },
  );
  assert.deepEqual(reports, [[1, "timeout", 250]]);
});

test("withRetry rejects fewer than one attempt", async () => {
  await assert.rejects(withRetry(async () => 1, { attempts: 0, initialDelayMs: 1 }), RangeError);
});

test("pinnedVSCodeVersion reads BROWSITORY_E2E_VSCODE_VERSION, unset or blank means none", () => {
  assert.equal(pinnedVSCodeVersion({ BROWSITORY_E2E_VSCODE_VERSION: "1.146.0" }), "1.146.0");
  assert.equal(pinnedVSCodeVersion({ BROWSITORY_E2E_VSCODE_VERSION: " 1.146.0\n" }), "1.146.0");
  assert.equal(pinnedVSCodeVersion({ BROWSITORY_E2E_VSCODE_VERSION: "  " }), undefined);
  assert.equal(pinnedVSCodeVersion({}), undefined);
});
