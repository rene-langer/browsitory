import fs from "node:fs";
import path from "node:path";
import * as vscode from "vscode";
import { gitWith } from "../support/git";
import { connectToWebview } from "../support/webviewPage";

// The operation log over the sidecar transport (#887-#891): `list_operations` and the
// `operationLog` notifications. A commit fails on its pre-commit hook; the error's "Show details"
// opens the log on that commit, with the hook's output.
describe("Browsitory VSCode extension operation log", () => {
  const FILE = "vscode-oplog.txt";
  const SUMMARY = "vscode-e2e: operation log";
  const HOOK_OUTPUT = "vscode-e2e-oplog: lint failed";
  let cwd = "";
  let hookPath = "";

  const git = (...args: string[]) => gitWith(cwd, args);

  before(function () {
    // Hooks run through Git for Windows' sh there; this covers the Unix path, as the hooks spec.
    if (process.platform === "win32") this.skip();
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    cwd = workspaceFolder.uri.fsPath;
    const hooksDir = path.resolve(cwd, git("rev-parse", "--git-path", "hooks"));
    fs.mkdirSync(hooksDir, { recursive: true });
    hookPath = path.join(hooksDir, "pre-commit");
  });

  after(async () => {
    // Later specs share the fixture repository: leave no hook, staged file or stray file behind.
    if (process.platform === "win32") return;
    fs.rmSync(hookPath, { force: true });
    try {
      git("reset", "-q", "--", FILE);
    } catch {
      // not staged
    }
    fs.rmSync(path.join(cwd, FILE), { force: true });
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  });

  it("shows a failed commit and its hook's output from the error's Show details", async function () {
    this.timeout(120000);
    fs.writeFileSync(hookPath, `#!/bin/sh\necho "${HOOK_OUTPUT}" >&2\nexit 1\n`, { mode: 0o755 });
    fs.writeFileSync(path.join(cwd, FILE), "logged\n");
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);
    try {
      await webview.waitForText("span", `${FILE} (New)`, 15000);
      await webview.clickByText("button", "Stage all");
      await webview.fill('textarea[aria-label="Commit message"]', SUMMARY);
      await webview.clickBySelector('button[data-action="commit"]');
      await webview.waitForSelector('dialog[aria-label="pre-commit hook failed"]', 20000);
      await webview.clickByText("button", "Close");
      await webview.clickByText("button", "Show details");

      await webview.waitForSelector('dialog[aria-label="Operation log"]', 15000);
      await webview.waitForText('section[aria-label="Operation details"] h3', `Commit ${SUMMARY}`, 15000);
      await webview.waitForText('pre[aria-label="Output of pre-commit"]', HOOK_OUTPUT, 15000);
      const text = await webview.bodyText();
      if (!text.includes("The pre-commit hook failed.")) {
        throw new Error(`the log shows no error\npage text:\n${text}`);
      }
    } finally {
      await webview.close();
    }
  });
});
