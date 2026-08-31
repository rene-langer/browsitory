import { spawn } from "node:child_process";
import * as vscode from "vscode";
import {
  resolveDevelopmentSidecarPath,
  resolvePackagedSidecarPath,
  SidecarBridge,
} from "./sidecarBridge";
import { renderWebviewHtml } from "./webviewHtml";

let currentPanel: vscode.WebviewPanel | undefined;
let currentBridge: SidecarBridge | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel("Browsitory");
  context.subscriptions.push(output);

  const command = vscode.commands.registerCommand("browsitory.open", () => {
    if (currentPanel) {
      currentPanel.reveal(currentPanel.viewColumn ?? vscode.ViewColumn.One);
      return;
    }

    const assetRoot = vscode.Uri.joinPath(
      context.extensionUri,
      "..",
      "frontend",
      "dist-vscode",
    );
    const panel = vscode.window.createWebviewPanel(
      "browsitory",
      "Browsitory",
      vscode.ViewColumn.One,
      {
        enableScripts: true,
        localResourceRoots: [assetRoot],
      },
    );
    currentPanel = panel;

    const executablePath =
      context.extensionMode === vscode.ExtensionMode.Development
        ? resolveDevelopmentSidecarPath(context.extensionUri.fsPath)
        : resolvePackagedSidecarPath(context.extensionUri.fsPath);

    const bridge = new SidecarBridge({
      spawn: (sidecarPath) =>
        spawn(sidecarPath, [], {
          stdio: ["pipe", "pipe", "pipe"],
          windowsHide: true,
        }),
      executablePath,
      context,
      postToWebview: (message) => panel.webview.postMessage(message),
      showOpenDialog: (options) => vscode.window.showOpenDialog(options),
      openExternal: (url) => vscode.env.openExternal(vscode.Uri.parse(url, true)),
      appendLine: (message) => output.appendLine(message),
    });
    currentBridge = bridge;

    const script = vscode.Uri.joinPath(assetRoot, "assets", "vscode-main.js");
    const style = vscode.Uri.joinPath(assetRoot, "assets", "vscode-main.css");
    panel.webview.html = renderWebviewHtml(panel.webview, { script, style });

    const messageSubscription = panel.webview.onDidReceiveMessage((message: unknown) => {
      void bridge.handleWebviewMessage(message).catch((error: unknown) => {
        output.appendLine(
          "Browsitory: failed to handle webview request: " +
            (error instanceof Error ? error.message : String(error)),
        );
      });
    });
    context.subscriptions.push(messageSubscription);

    const panelSubscription = panel.onDidDispose(() => {
      bridge.dispose();
      if (currentBridge === bridge) currentBridge = undefined;
      if (currentPanel === panel) currentPanel = undefined;
    });
    context.subscriptions.push(panelSubscription);
  });

  context.subscriptions.push(command);
}

export function deactivate(): void {
  currentBridge?.dispose();
  currentBridge = undefined;
  currentPanel = undefined;
}
