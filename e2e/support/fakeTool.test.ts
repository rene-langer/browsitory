import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { once } from "node:events";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { after, test } from "node:test";

// Unit test for the fake external tool (`fakeTool.ts`), run with `pnpm test:support` (#916).
// The fake tool lives under os.tmpdir(); point that at a fresh directory first, so this test
// never touches the HOME of an E2E run going on at the same time.
process.env["TMPDIR"] = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-fake-tool-test-"));
const { FAKE_TOOL, fakeToolLaunches, prepareFakeToolHome } = await import("./fakeTool");
after(() => fs.rmSync(process.env["TMPDIR"] as string, { recursive: true, force: true }));

test(
  "a launch the fake tool is still writing is not reported, a finished one is (#916)",
  { skip: process.platform === "win32" ? "the fake tool is a shell script" : false },
  async () => {
    prepareFakeToolHome();
    const dir = process.env["TMPDIR"] as string;
    // LOCAL is a FIFO: the tool's `cat "$LOCAL"` blocks until this test writes to it, which
    // holds the launch half-written for as long as the test needs.
    const local = path.join(dir, "local-fifo");
    execFileSync("mkfifo", [local]);
    const remote = path.join(dir, "remote.txt");

    const tool = spawn(FAKE_TOOL, ["diff", local, remote], {
      env: { ...process.env, LOCAL: local, REMOTE: remote },
      stdio: "ignore",
    });
    const exited = once(tool, "exit");
    // Opening the FIFO for writing returns once the tool's `cat` has opened it for reading:
    // the tool is now in the middle of the launch.
    const writer = await fs.promises.open(local, "w");
    try {
      assert.deepEqual(fakeToolLaunches(), [], "a launch still being written must not be visible");
      await writer.write("one\ntwo\n");
    } finally {
      await writer.close();
    }
    const [code] = await exited;
    assert.equal(code, 0);

    assert.deepEqual(fakeToolLaunches(), [
      ["arg: diff", `arg: ${local}`, `arg: ${remote}`, "LOCAL contents:", "one", "two", `REMOTE: ${remote}`],
    ]);
  },
);
