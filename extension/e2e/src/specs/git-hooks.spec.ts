import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

// Hooks over the sidecar transport (#123): `hookProgress` notifications, a `HookFailed` commit
// outcome with its retry, and `cancel_transfer` stopping a running hook.
describe("Browsitory VSCode extension git hooks", () => {
  let cwd = "";
  let hookPath = "";

  const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();
  const lastSummary = () => {
    try {
      return git("log", "-1", "--format=%s");
    } catch {
      return "";
    }
  };
  const installHook = (body: string) => {
    fs.writeFileSync(hookPath, `#!/bin/sh\n${body}\n`, { mode: 0o755 });
  };
  const waitUntil = async (done: () => boolean, what: string, timeoutMs = 15000) => {
    const deadline = Date.now() + timeoutMs;
    while (!done()) {
      if (Date.now() > deadline) throw new Error(what);
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  };
  const openWebview = async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    return connectToWebview(`http://127.0.0.1:${cdpPort}`);
  };
  // Later specs share the fixture repository: leave no hook, staged file or stray file behind.
  const cleanUp = async (file: string, summary: string) => {
    fs.rmSync(hookPath, { force: true });
    if (lastSummary() !== summary) {
      try {
        git("reset", "-q", "--", file);
      } catch {
        // not staged
      }
      fs.rmSync(path.join(cwd, file), { force: true });
    }
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  };

  before(function () {
    // Hooks run through Git for Windows' sh there; this covers the Unix path, as the Tauri spec.
    if (process.platform === "win32") this.skip();
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    cwd = workspaceFolder.uri.fsPath;
    const hooksDir = path.resolve(cwd, git("rev-parse", "--git-path", "hooks"));
    fs.mkdirSync(hooksDir, { recursive: true });
    hookPath = path.join(hooksDir, "pre-commit");
  });

  it("shows a failing pre-commit's output and commits without hooks", async function () {
    this.timeout(120000);
    const file = "vscode-hooks.txt";
    const summary = "vscode-e2e: committed without hooks";
    installHook('sleep 3; echo "vscode-e2e-hook: lint failed" >&2; exit 1');
    fs.writeFileSync(path.join(cwd, file), "hooked\n");
    const webview = await openWebview();
    try {
      // `HeadInfo.commitHooks` over the sidecar: the installed hook shows the checkbox.
      await webview.waitForText("label", "Skip hooks", 15000);
      await webview.waitForText("span", `${file} (New)`, 15000);
      await webview.clickByText("button", "Stage all");
      await webview.fill('textarea[aria-label="Commit message"]', summary);
      await webview.clickByText("button", "Commit");
      // `hookProgress` while the hook sleeps.
      await webview.waitForText("p", "Running pre-commit hook", 15000);
      // `CommitOutcome::HookFailed` over the sidecar.
      await webview.waitForSelector('dialog[aria-label="pre-commit hook failed"]', 20000);
      await webview.waitForText('pre[aria-label="Hook output"]', "vscode-e2e-hook: lint failed", 15000);
      if (lastSummary() === summary) throw new Error("commit landed despite the failing hook");

      await webview.clickByText("button", "Commit without hooks");
      await waitUntil(() => lastSummary() === summary, "commit without hooks did not land");
    } finally {
      await webview.close();
      await cleanUp(file, summary);
    }
  });

  it("Cancel stops a running pre-commit", async function () {
    this.timeout(120000);
    const file = "vscode-hooks-cancel.txt";
    const summary = "vscode-e2e: cancelled by the user";
    installHook("sleep 60");
    fs.writeFileSync(path.join(cwd, file), "cancelled\n");
    const webview = await openWebview();
    try {
      await webview.waitForText("span", `${file} (New)`, 15000);
      await webview.clickByText("button", "Stage all");
      await webview.fill('textarea[aria-label="Commit message"]', summary);
      await webview.clickByText("button", "Commit");
      await webview.waitForText('p[role="status"]', "Running pre-commit hook", 15000);
      const cancelledAt = Date.now();
      // `cancel_transfer` over the sidecar, through the running hook's own Cancel button (other
      // panels have "Cancel" buttons too).
      await webview.click('p[role="status"] button');
      // The hook sleeps 60 s: its status line going away within 20 s means Cancel stopped it.
      const deadline = Date.now() + 20000;
      while ((await webview.bodyText()).includes("Running pre-commit hook")) {
        if (Date.now() > deadline) {
          throw new Error(`the hook kept running after Cancel\npage text:\n${await webview.bodyText()}`);
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
      if (Date.now() - cancelledAt > 20000) throw new Error("Cancel did not stop the hook in time");
      if (lastSummary() === summary) throw new Error("commit landed despite Cancel");
      // The cancel's error banner stays until it is dismissed or the next change starts (#177).
      await webview.waitForText('p[role="alert"]', "The pre-commit hook was cancelled.", 10000);
      await new Promise((resolve) => setTimeout(resolve, 3000));
      if (!(await webview.bodyText()).includes("The pre-commit hook was cancelled.")) {
        throw new Error(`the cancel banner went away on its own\npage text:\n${await webview.bodyText()}`);
      }
    } finally {
      await webview.close();
      await cleanUp(file, summary);
    }
  });
});
