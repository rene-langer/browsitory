import { execFileSync } from "node:child_process";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension line staging", () => {
  it("stages a single selected line of a hunk", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString();
    const file = "vscode-lines.txt";
    const target = vscode.Uri.joinPath(workspaceFolder.uri, file);

    await vscode.workspace.fs.writeFile(target, Buffer.from("a\nb\nc\n"));
    git("add", file);
    git("commit", "-m", "vscode-e2e: seed line fixture");
    await vscode.workspace.fs.writeFile(target, Buffer.from("a\nB\nc\nd\n"));

    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    try {
      await webview.waitForText("span", `${file} (Modified)`, 15000);
      await webview.clickByText("span", `${file} (Modified)`);
      await webview.waitForSelector(".diff-line-add", 15000);
      await webview.clickBySelector(".diff-line-add", -1);
      await webview.clickByText("button", "Stage lines (1)");

      const deadline = Date.now() + 15000;
      while (git("show", `:${file}`) !== "a\nb\nc\nd\n") {
        if (Date.now() > deadline) throw new Error("expected only the added line in the index");
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      git("reset", "-q");
      git("checkout", "--", file);
    }
  });
});
