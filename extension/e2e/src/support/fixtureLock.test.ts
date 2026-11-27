// Unit tests for the harness's own helpers (node:test, not the VSCode suite):
//   pnpm --ignore-workspace test:unit
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, test } from "node:test";
import { acquireFixtureLock } from "./fixtureLock";

let scratch = "";
let lockDir = "";

beforeEach(() => {
  scratch = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-fixture-lock-test-"));
  lockDir = path.join(scratch, "fixtures.lock");
});

afterEach(() => fs.rmSync(scratch, { recursive: true, force: true }));

interface Owner {
  pid: number;
  cwd: string;
  started: string;
}

function owner(): Owner {
  return JSON.parse(fs.readFileSync(path.join(lockDir, "owner.json"), "utf8")) as Owner;
}

function writeLock(lock: Owner) {
  fs.mkdirSync(lockDir);
  fs.writeFileSync(path.join(lockDir, "owner.json"), JSON.stringify(lock));
}

/** The pid of a process that has exited. */
async function deadPid(): Promise<number> {
  const child = spawn(process.execPath, ["-e", ""], { stdio: "ignore" });
  await once(child, "exit");
  assert.ok(child.pid);
  return child.pid;
}

test("acquireFixtureLock takes a free lock for this process, and release removes it", () => {
  const release = acquireFixtureLock(lockDir);

  assert.equal(owner().pid, process.pid);
  assert.equal(owner().cwd, process.cwd());
  release();
  assert.equal(fs.existsSync(lockDir), false);
});

test("acquireFixtureLock refuses a lock a live run holds, names that run and leaves its lock", async () => {
  const other = spawn(process.execPath, ["-e", "setTimeout(() => {}, 60000)"], { stdio: "ignore" });
  try {
    assert.ok(other.pid);
    const held = { pid: other.pid, cwd: "/src/other-worktree/e2e", started: "2026-11-27T10:00:00.000Z" };
    writeLock(held);

    assert.throws(
      () => acquireFixtureLock(lockDir, "Wait for it to finish."),
      (error: Error) =>
        error.message.includes(`pid ${other.pid}`) &&
        error.message.includes("/src/other-worktree/e2e") &&
        error.message.includes("Wait for it to finish."),
    );
    assert.deepEqual(owner(), held);
  } finally {
    other.kill();
  }
});

test("acquireFixtureLock takes over a lock whose run is gone", async () => {
  writeLock({ pid: await deadPid(), cwd: "/src/crashed-run/e2e", started: "2026-11-27T09:00:00.000Z" });

  const release = acquireFixtureLock(lockDir);

  assert.equal(owner().pid, process.pid);
  release();
  assert.equal(fs.existsSync(lockDir), false);
});

test("acquireFixtureLock takes over a lock folder without an owner", () => {
  fs.mkdirSync(lockDir);

  const release = acquireFixtureLock(lockDir);

  assert.equal(owner().pid, process.pid);
  release();
});

test("release leaves a lock that another run has taken over since", () => {
  const release = acquireFixtureLock(lockDir);
  const other = { pid: process.pid + 1, cwd: "/src/other-worktree/e2e", started: "2026-11-27T11:00:00.000Z" };
  fs.writeFileSync(path.join(lockDir, "owner.json"), JSON.stringify(other));

  release();

  assert.deepEqual(owner(), other);
});
