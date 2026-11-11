import * as vscode from "vscode";
import { gitIn } from "../support/git";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension Timeline commit link", () => {
  it("selects a commit picked in VSCode's Timeline view in the graph (#759)", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = gitIn(cwd);
    const file = vscode.Uri.joinPath(workspaceFolder.uri, "vscode-timeline.txt");

    await vscode.workspace.fs.writeFile(file, Buffer.from("one\n"));
    git("add", "vscode-timeline.txt");
    git("commit", "-m", "vscode-e2e: timeline older");
    const older = git("rev-parse", "HEAD");
    await vscode.workspace.fs.writeFile(file, Buffer.from("two\n"));
    git("add", "vscode-timeline.txt");
    git("commit", "-m", "vscode-e2e: timeline newer");

    // What the Timeline's context menu runs: the Git timeline's item (its commit as `ref` and
    // `id`), the file's Uri and the source.
    await vscode.commands.executeCommand(
      "browsitory.showCommit",
      { id: older, ref: older, label: "vscode-e2e: timeline older", contextValue: "git:file:commit" },
      file,
      "git-history",
    );
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    try {
      await webview.waitForText('li[role="option"][aria-selected="true"]', "vscode-e2e: timeline older", 15000);
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    }
  });
});
