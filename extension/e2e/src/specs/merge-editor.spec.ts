import * as vscode from "vscode";
import { gitIn, gitWith } from "../support/git";
import { connectToWebview, type WebviewSession } from "../support/webviewPage";

// #880: the three-pane merge editor (#875 … #879) in the VSCode webview: a two-block conflict
// resolved with the take-side arrows and an edit, saved, and the merge continued.

/** Ten lines; `two` and `nine` are far enough apart for git to make two conflicts of them. */
function lines(two: string, nine: string): string {
  return ["one", two, "three", "four", "five", "six", "seven", "eight", nine, "ten", ""].join("\n");
}

async function waitFor(done: () => boolean | Promise<boolean>, what: string, timeoutMs = 15000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!(await done())) {
    if (Date.now() > deadline) throw new Error(what);
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

/** The result field's value in the block named `block`. */
async function blockResult(webview: WebviewSession, block: string): Promise<string> {
  const [value] = await webview.values(`section[aria-label="${block}"] [data-pane="result"] textarea`);
  return value ?? "<no result field>";
}

describe("Browsitory VSCode extension three-pane merge editor", () => {
  it("resolves two conflicts with the arrows and an edit, saves, and commits the merge", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = gitIn(cwd);
    const name = "vscode-merge-editor.txt";
    const file = vscode.Uri.joinPath(workspaceFolder.uri, name);
    const baseBranch = git("rev-parse", "--abbrev-ref", "HEAD");
    const theirsBranch = "vscode-e2e-merge-editor-theirs";

    await vscode.workspace.fs.writeFile(file, Buffer.from(lines("two", "nine")));
    git("add", name);
    git("commit", "-m", "vscode-e2e: merge editor base");
    git("checkout", "-b", theirsBranch);
    await vscode.workspace.fs.writeFile(file, Buffer.from(lines("two feature", "nine feature")));
    git("commit", "-am", "vscode-e2e: merge editor theirs");
    git("checkout", baseBranch);
    await vscode.workspace.fs.writeFile(file, Buffer.from(lines("two main", "nine main")));
    git("commit", "-am", "vscode-e2e: merge editor ours");
    try {
      gitWith(cwd, ["merge", "--no-edit", theirsBranch], { stdio: "ignore" });
    } catch {
      // conflicted, which is the point
    }
    if (!git("status", "--porcelain").includes(`UU ${name}`)) throw new Error("the merge did not conflict");

    // The test window is small: below 680 px the merge editor shows the blocks instead of the
    // panes. Zoomed out, the webview has room for them next to the sidebar and the history.
    await vscode.commands.executeCommand("workbench.action.closeSidebar");
    await vscode.commands.executeCommand("workbench.action.closePanel");
    for (let i = 0; i < 3; i++) await vscode.commands.executeCommand("workbench.action.zoomOut");
    // A reused panel from an earlier spec would show pre-fixture state.
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);
    let committed = false;
    try {
      await webview.waitForText("span", `${name} (Conflicted)`, 15000);
      await webview.clickByText("span", `${name} (Conflicted)`);
      const first = "Conflict 1 of 2";
      const second = "Conflict 2 of 2";
      await webview.waitForSelector(`section[aria-label="${second}"]`);
      try {
        await webview.waitForSelector('[aria-label="Merge editor layout"] button[aria-pressed="true"]:first-child', 5000);
      } catch (error) {
        throw new Error(
          `${String(error)}: the merge editor shows the blocks, not the three panes; ` +
            `layout buttons pressed: ${JSON.stringify(await webview.attributes('[aria-label="Merge editor layout"] button', "aria-pressed"))}`,
        );
      }
      await webview.waitForText('[role="status"]', "0 of 2 resolved");
      if (!(await blockResult(webview, first)).includes("<<<<<<<")) throw new Error("no conflict markers in the result");

      // Take theirs in the first block; ours, then theirs after it, in the second.
      await webview.click(`section[aria-label="${first}"] button[aria-label="Take theirs"]`);
      await webview.click(`section[aria-label="${second}"] button[aria-label="Take ours"]`);
      await webview.click(`section[aria-label="${second}"] button[aria-label="Take theirs"]`);
      await webview.waitForText('[role="status"]', "2 of 2 resolved");
      const results = [await blockResult(webview, first), await blockResult(webview, second)];
      if (results[0] !== "two feature\n" || results[1] !== "nine main\nnine feature\n") {
        throw new Error(`the arrows took ${JSON.stringify(results)}`);
      }

      // An edit in the second block's result.
      await webview.fill(`section[aria-label="${second}"] [data-pane="result"] textarea`, "nine, both\n");
      await webview.waitForText(`section[aria-label="${second}"] h4`, "Edited");

      await webview.clickByText("button", "Save resolution");
      const expected = lines("two feature", "nine, both");
      const onDisk = async () => Buffer.from(await vscode.workspace.fs.readFile(file)).toString("utf8");
      await waitFor(async () => (await onDisk()) === expected, "the resolution was not saved");
      await waitFor(() => git("status", "--porcelain", "--", name) === `M  ${name}`, "the file was not staged");

      // Continue the merge: commit it.
      await webview.waitForSelector('button[data-action="commit"]:not([disabled]):not([aria-disabled="true"])');
      await webview.clickBySelector('button[data-action="commit"]');
      await waitFor(
        () => git("rev-list", "--parents", "-n", "1", "HEAD").split(" ").length === 3,
        "the merge commit did not land",
      );
      committed = true;
      if (git("show", `HEAD:${name}`) !== expected.trim()) throw new Error("the merge commit has another result");
    } catch (error) {
      throw new Error(`${String(error)}\npage text:\n${await webview.bodyText().catch(String)}`);
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.zoomReset");
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      // Later specs commit in this repository: never leave the merge in progress.
      try {
        if (!committed) gitWith(cwd, ["merge", "--abort"], { stdio: "ignore" });
        gitWith(cwd, ["branch", "-D", theirsBranch], { stdio: "ignore" });
      } catch {
        // best effort cleanup
      }
    }
  });
});
