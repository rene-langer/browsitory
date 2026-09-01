import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension first flow", () => {
  it("opens a repo, stages a file, commits, and sees it in history", async function () {
    // Generous: VSCode's webview host creates the panel's real content iframe some time after
    // the panel itself exists, and that gap was measured anywhere from ~2s to ~24s on a loaded
    // machine. `connectToWebview` polls for it against its own 45s budget, so this outer timeout
    // has to sit above that or it would fire first and mask a still-progressing connect.
    this.timeout(120000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const fixtureFile = vscode.Uri.joinPath(workspaceFolder.uri, "first-flow-fixture.txt");
    await vscode.workspace.fs.writeFile(fixtureFile, Buffer.from("first flow\n"));

    await vscode.commands.executeCommand("browsitory.open");

    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    // Driven via raw CDP `Runtime.evaluate` (see `support/webviewPage.ts`) rather than
    // Playwright Locators — the webview's real content lives in a nested same-process iframe
    // that Playwright's own frame discovery doesn't reliably surface when connected externally
    // via `connectOverCDP` to this Electron instance. A plain DOM `.click()` also sidesteps the
    // old Playwright-actionability concern (the stage control is `opacity: 0` until its row is
    // hovered/focused, mirroring `e2e/specs/first-flow.spec.ts`'s own comment) since there's no
    // actionability check at this level.
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    try {
      await webview.waitForSelector('button[aria-label="Stage first-flow-fixture.txt"]', 15000);
      await webview.click('button[aria-label="Stage first-flow-fixture.txt"]');

      await webview.fill("textarea[placeholder='Commit message']", "e2e: first commit");
      await webview.clickByText("button", "Commit");

      await webview.waitForText("li", "e2e: first commit", 15000);
    } finally {
      await webview.close();
    }
  });
});
