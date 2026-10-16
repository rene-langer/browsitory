import { execFileSync } from "node:child_process";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension file history", () => {
  it("opens a file's history from the diff pane", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();
    const file = vscode.Uri.joinPath(workspaceFolder.uri, "vscode-history.txt");

    await vscode.workspace.fs.writeFile(file, Buffer.from("one\n"));
    git("add", "vscode-history.txt");
    git("commit", "-m", "vscode-e2e: history first");
    await vscode.workspace.fs.writeFile(file, Buffer.from("two\n"));

    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    try {
      await webview.waitForSelector(`button[aria-label="History of vscode-history.txt"]`);
      await webview.click(`button[aria-label="History of vscode-history.txt"]`);
      await webview.waitForSelector(`[role="toolbar"][aria-label="File history"]`);
      await webview.waitForText("li", "vscode-e2e: history first", 15000);
    } finally {
      await webview.close();
      // Leave no open panel behind: later specs re-run `browsitory.open`.
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      git("checkout", "--", "vscode-history.txt");
    }
  });
});
