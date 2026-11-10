import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension undo", () => {
  it("undoes a commit from the toolbar, leaving the change staged", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();
    const file = "vscode-undo.txt";
    const summary = "vscode-e2e: undo me";
    const tipBefore = git("rev-parse", "HEAD");

    await vscode.workspace.fs.writeFile(vscode.Uri.joinPath(workspaceFolder.uri, file), Buffer.from("undo me\n"));

    // A reused panel from an earlier spec would show pre-fixture state.
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);
    const waitUntil = async (done: () => boolean, what: string) => {
      const deadline = Date.now() + 15000;
      while (!done()) {
        if (Date.now() > deadline) throw new Error(what);
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
    };
    try {
      // Stage + commit through the app so the worker records the commit.
      await webview.waitForText("span", `${file} (New)`, 15000);
      await webview.clickByText("button", "Stage all");
      await webview.fill('textarea[aria-label="Commit message"]', summary);
      await webview.clickBySelector('button[data-action="commit"]');
      await waitUntil(() => git("log", "-1", "--format=%s") === summary, "commit did not land");

      // The label can show while a mutation is still pending, and a click on the then disabled
      // button does nothing: wait until it is enabled too.
      const undoButton = `button[aria-label="Undo Commit “${summary}”"]:not(:disabled)`;
      try {
        await webview.waitForSelector(undoButton, 15000);
      } catch (error) {
        // #342: say what the toolbar shows and what the undo history holds on disk.
        const shown = await webview.attributes('[aria-label="Undo"] button', "aria-label").catch(String);
        let stored = "";
        try {
          stored = fs.readFileSync(path.join(git("rev-parse", "--absolute-git-dir"), "browsitory", "undo.json"), "utf8");
        } catch (readError) {
          stored = String(readError);
        }
        const labels = stored.match(/"label":\s*"[^"]*"/g) ?? [];
        throw new Error(
          `${String(error)}\ntoolbar: ${JSON.stringify(shown)}\nundo.json labels: ${labels.join(", ")}\n` +
            `page text:\n${await webview.bodyText().catch(String)}`,
        );
      }
      await webview.click(undoButton);
      await waitUntil(() => git("rev-parse", "HEAD") === tipBefore, "undo did not move HEAD back");
      if (!git("diff", "--cached", "--name-only").split("\n").includes(file)) {
        throw new Error("undone commit's change is not staged");
      }
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      try {
        // A failed undo leaves the commit: drop it, so later specs see the fixture as before.
        git("reset", "-q", tipBefore);
      } catch {
        // best effort cleanup
      }
    }
  });
});
