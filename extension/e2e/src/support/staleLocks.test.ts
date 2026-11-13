// Unit tests for the harness's own helpers (node:test, not the VSCode suite):
//   pnpm --ignore-workspace test:unit
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { clearStaleLocks } from "./staleLocks";

function gitDir(): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-stale-locks-"));
  fs.mkdirSync(path.join(dir, "refs", "heads"), { recursive: true });
  fs.mkdirSync(path.join(dir, "browsitory"), { recursive: true });
  fs.writeFileSync(path.join(dir, "index"), "");
  return dir;
}

test("clearStaleLocks returns nothing and doesn't wait when no lock is there", async () => {
  const dir = gitDir();
  const delays: number[] = [];
  const removed = await clearStaleLocks(dir, {
    graceMs: 5000,
    pollMs: 100,
    sleep: async (ms) => void delays.push(ms),
  });
  assert.deepEqual(removed, []);
  assert.deepEqual(delays, []);
});

test("clearStaleLocks leaves a lock its holder releases within the grace period", async () => {
  const dir = gitDir();
  const lock = path.join(dir, "index.lock");
  fs.writeFileSync(lock, "");
  const removed = await clearStaleLocks(dir, {
    graceMs: 5000,
    pollMs: 100,
    // The holder (a git process still running) finishes after the first poll.
    sleep: async () => fs.rmSync(lock, { force: true }),
  });
  assert.deepEqual(removed, []);
});

test("clearStaleLocks removes the index and ref locks still there after the grace period", async () => {
  const dir = gitDir();
  fs.writeFileSync(path.join(dir, "index.lock"), "");
  fs.writeFileSync(path.join(dir, "refs", "heads", "main.lock"), "");
  // The app's own state lock is an OS lock on a file that stays by design: never touched.
  fs.writeFileSync(path.join(dir, "browsitory", "state.lock"), "");
  const delays: number[] = [];
  const removed = await clearStaleLocks(dir, {
    graceMs: 1000,
    pollMs: 250,
    sleep: async (ms) => void delays.push(ms),
  });
  assert.deepEqual(removed, ["index.lock", path.join("refs", "heads", "main.lock")]);
  assert.deepEqual(delays, [250, 250, 250, 250]);
  assert.equal(fs.existsSync(path.join(dir, "index.lock")), false);
  assert.equal(fs.existsSync(path.join(dir, "refs", "heads", "main.lock")), false);
  assert.equal(fs.existsSync(path.join(dir, "browsitory", "state.lock")), true);
});
