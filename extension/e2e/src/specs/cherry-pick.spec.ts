import { execFileSync } from "node:child_process";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension cherry-pick", () => {
  it("cherry-picks a commit from another branch via the commit menu", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();

    const baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    git("checkout", "-b", "vscode-e2e-pick-source");
    await vscode.workspace.fs.writeFile(
      vscode.Uri.joinPath(workspaceFolder.uri, "vscode-picked.txt"),
      Buffer.from("picked\n"),
    );
    git("add", "vscode-picked.txt");
    git("commit", "-m", "vscode-e2e: commit to cherry-pick");
    git("checkout", baseBranch);

    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    try {
      await webview.waitForText("li", "vscode-e2e: commit to cherry-pick", 15000);
      await webview.contextMenuByText("li", "vscode-e2e: commit to cherry-pick");
      await webview.clickByText("button", "Cherry-pick");

      const deadline = Date.now() + 15000;
      while (git("log", "-1", "--format=%s") !== "vscode-e2e: commit to cherry-pick") {
        if (Date.now() > deadline) throw new Error("cherry-pick did not land on the base branch");
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
    } finally {
      await webview.close();
      // Leave no open panel behind: first-flow.spec.ts re-runs `browsitory.open` and would
      // otherwise reuse this panel's stale (pre-fixture-write) state.
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    }
  });
});
