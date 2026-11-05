import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension third-party licences", () => {
  it("loads the notices file into the licences dialog", async function () {
    // Same budget rationale as first-flow.spec.ts (webview iframe can take ~24s to appear).
    this.timeout(120000);
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);
    try {
      // `connectToWebview` returns once the webview has a document, not once the app rendered
      // into it: clicking at once could find no header yet (#182). Wait for each button.
      await webview.waitForSelector('button[aria-label="Release notes"]', 15000);
      await webview.click('button[aria-label="Release notes"]');
      await webview.waitForText("button", "Third-party licences", 15000);
      await webview.clickByText("button", "Third-party licences");
      // The text region renders only once third-party-notices.md was fetched from the
      // webview's own files, so this proves the CSP's connect-src and the asset path.
      await webview.waitForSelector('pre[aria-label="Licence texts"]', 15000);
      const text = await webview.bodyText();
      if (!text.includes("libgit2") || !text.includes("option-ext")) {
        throw new Error(`licences dialog lacks the expected notices: ${text.slice(0, 500)}`);
      }
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    }
  });
});
