import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension commit signing", () => {
  it("shows a good signature for a commit signed through the app", async function () {
    this.timeout(120000);
    try {
      execFileSync("ssh-keygen", ["-?"], { stdio: "ignore" });
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") this.skip();
    }
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();
    const file = "vscode-signing.txt";
    const summary = "vscode-e2e: signed commit";
    const configKeys = ["commit.gpgSign", "gpg.format", "user.signingKey", "gpg.ssh.allowedSignersFile"];

    const keyDir = fs.mkdtempSync(path.join(os.tmpdir(), "browsitory-vscode-ssh-"));
    const key = path.join(keyDir, "id_e2e");
    execFileSync("ssh-keygen", ["-q", "-t", "ed25519", "-N", "", "-f", key]);
    const pub = fs.readFileSync(`${key}.pub`, "utf8").trim();
    fs.writeFileSync(path.join(keyDir, "allowed_signers"), `${git("config", "user.email")} ${pub}\n`);
    git("config", "commit.gpgSign", "true");
    git("config", "gpg.format", "ssh");
    git("config", "user.signingKey", key);
    git("config", "gpg.ssh.allowedSignersFile", path.join(keyDir, "allowed_signers"));

    await vscode.workspace.fs.writeFile(vscode.Uri.joinPath(workspaceFolder.uri, file), Buffer.from("signed\n"));
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
      await webview.waitForText("span", `${file} (New)`, 15000);
      await webview.clickByText("button", "Stage all");
      await webview.fill('textarea[aria-label="Commit message"]', summary);
      await webview.clickByText("button", "Commit");
      await waitUntil(() => git("log", "-1", "--format=%s") === summary, "commit did not land");
      await webview.waitForText("li", summary, 15000);
      await webview.clickByTextContaining("li", summary);
      await webview.waitForText("span", "Good signature", 15000);
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      for (const configKey of configKeys) {
        try {
          git("config", "--unset", configKey);
        } catch {
          // already unset
        }
      }
      fs.rmSync(keyDir, { recursive: true, force: true });
    }
  });
});
