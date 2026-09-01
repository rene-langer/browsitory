import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension first flow", () => {
  it("opens a repo, stages a file, commits, and sees it in history", async function () {
    this.timeout(30000);

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const fixtureFile = vscode.Uri.joinPath(workspaceFolder.uri, "first-flow-fixture.txt");
    await vscode.workspace.fs.writeFile(fixtureFile, Buffer.from("first flow\n"));

    await vscode.commands.executeCommand("browsitory.open");

    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const page = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

    // The stage control is `opacity: 0` until its row is hovered/focused (mirrors
    // `e2e/specs/first-flow.spec.ts`'s own comment), and Playwright's own actionability check
    // (like WebdriverIO's) rejects a fully transparent element for a plain `.click()`.
    const stageButton = page.locator('button[aria-label="Stage first-flow-fixture.txt"]');
    await stageButton.waitFor({ state: "attached", timeout: 10000 });
    await stageButton.evaluate((el) => (el as HTMLElement).click());

    const commitMessageInput = page.locator("textarea[placeholder='Commit message']");
    await commitMessageInput.fill("e2e: first commit");
    await page.locator("button", { hasText: "Commit" }).click();

    const historyEntry = page.locator("li", { hasText: "e2e: first commit" });
    await historyEntry.waitFor({ state: "attached", timeout: 10000 });
  });
});
