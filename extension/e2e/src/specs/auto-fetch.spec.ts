import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import * as vscode from "vscode";
import { setTestIdentity } from "../support/fixtureRepo";
import { git } from "../support/git";
import { connectToWebview } from "../support/webviewPage";

// "Fetch automatically" (#440, #546) in the VSCode extension: the webview's timer, the sidecar's
// `background_fetch` and the panel-visibility pause, end to end against a local bare remote. The
// app-wide interval lives in the suite's own config dir (`prepareConfigDir` in
// `support/launch.ts`), shared by every spec, so this one turns it off again when it is done.
const REMOTE_NAME = "autofetch";
// The first background fetch runs a minute (the shortest interval) after the page loaded.
const AUTO_FETCH_TIMEOUT_MS = 120_000;

function configFile(): string {
  const dir = process.env["BROWSITORY_CONFIG_DIR"];
  if (!dir) throw new Error("BROWSITORY_CONFIG_DIR not set: runTests.ts must isolate the config dir");
  return path.join(dir, "config.toml");
}

/** Sets `[fetch] auto_fetch_minutes`, keeping whatever else the sidecar wrote to config.toml. */
function setAutoFetchMinutes(minutes: number): void {
  const file = configFile();
  const text = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : "";
  let updated: string;
  if (/^auto_fetch_minutes = \d+$/m.test(text)) {
    updated = text.replace(/^auto_fetch_minutes = \d+$/m, `auto_fetch_minutes = ${minutes}`);
  } else if (/^\[fetch\]$/m.test(text)) {
    updated = text.replace(/^\[fetch\]$/m, `[fetch]\nauto_fetch_minutes = ${minutes}`);
  } else {
    updated = `${text}${text === "" || text.endsWith("\n") ? "" : "\n"}\n[fetch]\nauto_fetch_minutes = ${minutes}\n`;
  }
  fs.writeFileSync(file, updated);
}

describe("Browsitory VSCode extension fetch automatically", () => {
  it("fetches a remote in the background while the panel is open", async function () {
    // Up to two minutes for the fetch, plus the webview connect (see first-flow.spec.ts).
    this.timeout(240_000);
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const trackingTip = () => {
      try {
        return git(cwd, "rev-parse", "--verify", "--quiet", `refs/remotes/${REMOTE_NAME}/main`);
      } catch {
        return null;
      }
    };

    // A bare remote holding one commit the fixture repository has never fetched.
    const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-vscode-e2e-autofetch-"));
    const bare = path.join(scratch, "remote.git");
    const source = path.join(scratch, "source");
    git(scratch, "init", "--bare", bare);
    git(cwd, "push", bare, "HEAD:refs/heads/main");
    git(scratch, "clone", "--branch", "main", bare, source);
    setTestIdentity(source);
    fs.writeFileSync(path.join(source, "auto-fetched.txt"), "from the remote\n");
    git(source, "add", "auto-fetched.txt");
    git(source, "commit", "-m", "vscode-e2e: only on the auto-fetch remote");
    git(source, "push", "origin", "main");
    const remoteTip = git(source, "rev-parse", "main");
    git(cwd, "remote", "add", REMOTE_NAME, bare);

    // Before the panel opens: the webview reads the interval when it loads.
    setAutoFetchMinutes(1);
    // A reused panel from an earlier spec would keep its old settings.
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);
    try {
      if (trackingTip() !== null) throw new Error(`refs/remotes/${REMOTE_NAME}/main exists before any fetch`);

      const deadline = Date.now() + AUTO_FETCH_TIMEOUT_MS;
      while (trackingTip() !== remoteTip) {
        if (Date.now() > deadline) {
          throw new Error(
            `the background fetch did not fetch ${REMOTE_NAME} within two minutes ` +
              `(tracking tip ${String(trackingTip())}, remote tip ${remoteTip})\n` +
              `page text:\n${await webview.bodyText().catch(String)}`,
          );
        }
        await new Promise((resolve) => setTimeout(resolve, 1000));
      }
      // The sync bar says when, quietly: no "Fetched from …" toast.
      await webview.waitForText("span", "Fetched just now", 15000);
      const text = await webview.bodyText();
      if (text.includes("Fetched from")) throw new Error("a background fetch showed the manual fetch's toast");
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      setAutoFetchMinutes(0);
      try {
        git(cwd, "remote", "remove", REMOTE_NAME);
      } catch {
        // already gone
      }
      fs.rmSync(scratch, { recursive: true, force: true });
    }
  });
});
