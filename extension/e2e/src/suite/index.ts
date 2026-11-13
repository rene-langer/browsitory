import path from "node:path";
import Mocha from "mocha";
import { globSync } from "node:fs";
import * as vscode from "vscode";
import { clearStaleLocks } from "../support/staleLocks";

export function run(): Promise<void> {
  const mocha = new Mocha({ ui: "bdd", timeout: 60000, color: true });
  const specsRoot = path.resolve(__dirname, "..", "specs");
  const files = globSync("**/*.spec.js", { cwd: specsRoot });
  for (const file of files) mocha.addFile(path.join(specsRoot, file));

  // Every spec shares the workspace repository (#961): a flow that leaves a git lock behind
  // would fail every later spec on it. After each spec, a lock still there after a grace period
  // (a git process may still be finishing) is removed so the next spec starts clean, and the run
  // fails at the end naming the spec that left it, even when that spec itself passed.
  const leftLocks: string[] = [];
  mocha.suite.afterEach(async function () {
    this.timeout(15000);
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) return;
    const removed = await clearStaleLocks(path.join(workspaceFolder.uri.fsPath, ".git"), {
      graceMs: 5000,
      pollMs: 250,
    });
    if (removed.length > 0) {
      const entry = `"${this.currentTest?.fullTitle() ?? "unknown spec"}" left ${removed.join(", ")}`;
      console.error(`[e2e] removed stale git locks: ${entry}`);
      leftLocks.push(entry);
    }
  });
  mocha.suite.afterAll(function () {
    if (leftLocks.length > 0) {
      throw new Error(`specs left git lock files behind:\n${leftLocks.join("\n")}`);
    }
  });

  return new Promise((resolve, reject) => {
    mocha.run((failures) => {
      if (failures > 0) reject(new Error(`${failures} test(s) failed`));
      else resolve();
    });
  });
}
