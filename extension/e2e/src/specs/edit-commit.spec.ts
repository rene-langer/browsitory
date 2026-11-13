import * as vscode from "vscode";
import { gitIn } from "../support/git";
import { connectToWebview } from "../support/webviewPage";

/** Polls `check` until it holds, failing with `message` (and what `describe` says) after `ms`. */
async function waitFor(check: () => boolean, message: () => string, ms = 20000): Promise<void> {
  const deadline = Date.now() + ms;
  while (!check()) {
    if (Date.now() > deadline) throw new Error(message());
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

// Edit an older commit (#882, #885): the stop has the commit's changes staged on its parent; a
// changed file staged there goes into the commit, and Continue replays the commits after it.
describe("Browsitory VSCode extension edit commit", () => {
  it("edits an older commit's file and continues: the history keeps the edit", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = gitIn(cwd);
    const write = (file: string, content: string) =>
      vscode.workspace.fs.writeFile(vscode.Uri.joinPath(workspaceFolder.uri, file), Buffer.from(content));

    const branch = git("rev-parse", "--abbrev-ref", "HEAD");
    await write("vscode-edit-older.txt", "first\n");
    git("add", "vscode-edit-older.txt");
    git("commit", "-m", "vscode-e2e: edit older");
    await write("vscode-edit-newer.txt", "newer\n");
    git("add", "vscode-edit-newer.txt");
    git("commit", "-m", "vscode-e2e: edit newer");
    const subjects = () => git("log", "--format=%s", "-2").split("\n");

    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    try {
      await webview.waitForText("li", "vscode-e2e: edit older", 15000);
      await webview.contextMenuByText("li", "vscode-e2e: edit older");
      await webview.clickByText("button", "Edit commit…");
      await webview.waitForText("[aria-label='Operation status']", "Editing ", 20000);

      await write("vscode-edit-older.txt", "first, edited\n");
      git("add", "vscode-edit-older.txt");
      await webview.clickByText("[aria-label='Operation status'] button", "Continue");

      await waitFor(
        () => git("rev-parse", "--abbrev-ref", "HEAD") === branch,
        () => `the edit never finished: HEAD is ${git("rev-parse", "--abbrev-ref", "HEAD")}`,
      );
      await waitFor(
        () => JSON.stringify(subjects()) === JSON.stringify(["vscode-e2e: edit newer", "vscode-e2e: edit older"]),
        () => `history is ${JSON.stringify(subjects())}`,
      );
      const older = git("log", "--format=%H", "-1", "--skip=1");
      if (git("show", `${older}:vscode-edit-older.txt`) !== "first, edited") {
        throw new Error("the older commit doesn't have the edited file");
      }
      if (git("show", "HEAD:vscode-edit-newer.txt") !== "newer") {
        throw new Error("the newer commit lost its file");
      }
      if (git("status", "--porcelain", "--untracked-files=no") !== "") {
        throw new Error(`changes left behind: ${git("status", "--porcelain")}`);
      }
    } finally {
      await webview.close();
      // Leave no open panel behind, as cherry-pick.spec.ts does.
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    }
  });
});
