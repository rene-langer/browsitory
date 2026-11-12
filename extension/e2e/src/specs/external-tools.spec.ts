import * as vscode from "vscode";
import { gitIn, gitWith } from "../support/git";
import { connectToWebview, type WebviewSession } from "../support/webviewPage";

// External tools in VSCode (#128): "Open in diff tool" hands off to VSCode's diff editor and
// "Open in merge tool" to its merge editor, both reading repository versions through the
// `browsitory-file:` content provider (the sidecar's `read_file_version`).

const FILE_VERSION_SCHEME = "browsitory-file";

function allTabInputs(): unknown[] {
  return vscode.window.tabGroups.all.flatMap((group) => group.tabs.map((tab) => tab.input));
}

/** Waits until an open tab's input passes `pick`, and returns it. */
async function waitForTab<T>(pick: (input: unknown) => T | undefined, what: string, timeoutMs = 15000): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const input of allTabInputs()) {
      const picked = pick(input);
      if (picked !== undefined) return picked;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`no ${what} tab opened within ${timeoutMs}ms`);
}

async function waitForTabLabel(label: string, timeoutMs = 15000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const labels = vscode.window.tabGroups.all.flatMap((group) => group.tabs.map((tab) => tab.label));
    if (labels.includes(label)) return;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  const labels = vscode.window.tabGroups.all.flatMap((group) => group.tabs.map((tab) => tab.label));
  throw new Error(`no "${label}" tab within ${timeoutMs}ms; open tabs: ${JSON.stringify(labels)}`);
}

async function contents(uri: vscode.Uri): Promise<string> {
  return (await vscode.workspace.openTextDocument(uri)).getText();
}

async function openWebview(): Promise<WebviewSession> {
  await vscode.commands.executeCommand("browsitory.open");
  const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
  if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
  return connectToWebview(`http://127.0.0.1:${cdpPort}`);
}

async function pickFileAction(webview: WebviewSession, file: string, label: string) {
  const actions = `button[aria-label="Actions for ${file}"]`;
  await webview.waitForSelector(actions);
  await webview.click(actions);
  await webview.waitForText('[role="menuitem"]', label);
  await webview.clickByText('[role="menuitem"]', label);
}

describe("Browsitory VSCode extension external tools", () => {
  it("opens a changed file in VSCode's diff editor, the staged version read through the content provider", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const git = gitIn(workspaceFolder.uri.fsPath);
    const name = "vscode-difftool.txt";
    const file = vscode.Uri.joinPath(workspaceFolder.uri, name);
    await vscode.workspace.fs.writeFile(file, Buffer.from("one\ntwo\n"));
    git("add", name);
    git("commit", "-m", "vscode-e2e: diff tool base");
    await vscode.workspace.fs.writeFile(file, Buffer.from("one\nTWO\n"));

    const webview = await openWebview();
    try {
      await pickFileAction(webview, name, "Open in diff tool");

      const diff = await waitForTab(
        (input) =>
          input instanceof vscode.TabInputTextDiff && input.modified.fsPath === file.fsPath ? input : undefined,
        `diff editor for ${name}`,
      );
      // The left side is the index version, served by the content provider; the right side is
      // the working tree file itself.
      if (diff.original.scheme !== FILE_VERSION_SCHEME) {
        throw new Error(`left side is ${diff.original.toString()}, not a ${FILE_VERSION_SCHEME}: URI`);
      }
      if (diff.modified.scheme !== "file") throw new Error(`right side is ${diff.modified.toString()}`);
      const left = await contents(diff.original);
      if (left !== "one\ntwo\n") throw new Error(`left side reads ${JSON.stringify(left)}`);
    } finally {
      await webview.close();
      // Leave no open panel behind: later specs re-run `browsitory.open`.
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      git("checkout", "--", name);
    }
  });

  it("hands a conflicted file to VSCode's merge editor with both sides and the base", async function () {
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = gitIn(cwd);
    const name = "vscode-mergetool.txt";
    const file = vscode.Uri.joinPath(workspaceFolder.uri, name);
    const baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    const theirsBranch = "vscode-e2e-mergetool-theirs";

    await vscode.workspace.fs.writeFile(file, Buffer.from("base\n"));
    git("add", name);
    git("commit", "-m", "vscode-e2e: merge tool base");
    git("checkout", "-b", theirsBranch);
    await vscode.workspace.fs.writeFile(file, Buffer.from("theirs\n"));
    git("commit", "-am", "vscode-e2e: merge tool theirs");
    git("checkout", baseBranch);
    await vscode.workspace.fs.writeFile(file, Buffer.from("ours\n"));
    git("commit", "-am", "vscode-e2e: merge tool ours");
    // Exits non-zero on the conflict, which is the point.
    try {
      gitWith(cwd, ["merge", theirsBranch], { stdio: "ignore" });
    } catch {
      // conflicted
    }
    if (!git("status", "--porcelain").includes(`UU ${name}`)) throw new Error("the merge did not conflict");

    const webview = await openWebview();
    try {
      await pickFileAction(webview, name, "Open in merge tool");

      // The merge editor's tab is "Merging: <file>" (its input type, TabInputTextMerge, isn't in
      // the stable API); the fallback, a plain open of the file, would be "<file>".
      await waitForTabLabel(`Merging: ${name}`);
      // Its three inputs are documents read through the content provider.
      const deadline = Date.now() + 15000;
      let texts: string[] = [];
      for (;;) {
        const documents = vscode.workspace.textDocuments.filter((doc) => doc.uri.scheme === FILE_VERSION_SCHEME);
        texts = documents.map((doc) => doc.getText()).sort();
        if (["base\n", "ours\n", "theirs\n"].every((text) => texts.includes(text))) break;
        if (Date.now() > deadline) {
          throw new Error(`the merge editor's ${FILE_VERSION_SCHEME}: inputs read ${JSON.stringify(texts)}`);
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
    } finally {
      try {
        await webview.close();
        // The merge editor fills its result (the work tree file) when it opens, so it is dirty:
        // a plain close would ask to save, which VSCode refuses to show in tests. Revert and
        // close it first; it is the active editor.
        if (vscode.window.tabGroups.activeTabGroup.activeTab?.label === `Merging: ${name}`) {
          await vscode.commands.executeCommand("workbench.action.revertAndCloseActiveEditor");
        }
        await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      } finally {
        // Later specs commit in this repository: never leave the merge in progress.
        gitWith(cwd, ["merge", "--abort"], { stdio: "ignore" });
        gitWith(cwd, ["branch", "-D", theirsBranch], { stdio: "ignore" });
      }
    }
  });
});
