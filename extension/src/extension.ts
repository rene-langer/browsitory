import { spawn } from "node:child_process";
import * as vscode from "vscode";
import {
  resolveDevelopmentSidecarPath,
  resolvePackagedSidecarPath,
  SidecarBridge,
} from "./sidecarBridge";
import { FILE_VERSION_SCHEME, parseFileVersionUri } from "./toolUris";
import { renderWebviewHtml } from "./webviewHtml";

let currentPanel: vscode.WebviewPanel | undefined;
let currentBridge: SidecarBridge | undefined;
let currentMessageSubscription: vscode.Disposable | undefined;
let currentPanelSubscription: vscode.Disposable | undefined;

export function resolveWebviewAssetRoot(
  extensionUri: vscode.Uri,
  mode: vscode.ExtensionMode,
): vscode.Uri {
  return mode !== vscode.ExtensionMode.Production
    ? vscode.Uri.joinPath(extensionUri, "..", "frontend", "dist-vscode")
    : vscode.Uri.joinPath(extensionUri, "webview");
}

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel("Browsitory");
  context.subscriptions.push(output);

  // Repository file versions for the diff and merge editors (`toolUris.ts`). Read through the
  // open panel's sidecar; without a panel there is no repository to read from.
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(FILE_VERSION_SCHEME, {
      provideTextDocumentContent: async (uri) => {
        if (!currentBridge) throw new Error("Browsitory is not open.");
        return currentBridge.readFileVersion(parseFileVersionUri(uri));
      },
    }),
  );

  const command = vscode.commands.registerCommand("browsitory.open", () => {
    if (currentPanel) {
      currentPanel.reveal(currentPanel.viewColumn ?? vscode.ViewColumn.One);
      return;
    }

    const assetRoot = resolveWebviewAssetRoot(context.extensionUri, context.extensionMode);
    const panel = vscode.window.createWebviewPanel(
      "browsitory",
      "Browsitory",
      vscode.ViewColumn.One,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [assetRoot],
      },
    );
    currentPanel = panel;

    const executablePath =
      context.extensionMode !== vscode.ExtensionMode.Production
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
      openFile: (file, line) =>
        vscode.commands.executeCommand("vscode.open", vscode.Uri.file(file), {
          preview: false,
          selection: line === null ? undefined : new vscode.Range(line - 1, 0, line - 1, 0),
        }),
      revealInOS: (target) => vscode.commands.executeCommand("revealFileInOS", vscode.Uri.file(target)),
      createTerminal: (cwd, name) => vscode.window.createTerminal({ cwd, name }).show(),
      openDiff: (left, right, title) =>
        vscode.commands.executeCommand(
          "vscode.diff",
          vscode.Uri.parse(left, true),
          vscode.Uri.parse(right, true),
          title,
        ),
      openMergeEditor: async (sides) => {
        const output = vscode.Uri.parse(sides.output, true);
        try {
          // Internal command (the one VSCode's own Git extension uses); a plain open with
          // VSCode's conflict decorations is the fallback.
          await vscode.commands.executeCommand("_open.mergeEditor", {
            base: vscode.Uri.parse(sides.base, true),
            input1: { uri: vscode.Uri.parse(sides.ours, true), title: "Ours" },
            input2: { uri: vscode.Uri.parse(sides.theirs, true), title: "Theirs" },
            output,
          });
          return true;
        } catch {
          await vscode.commands.executeCommand("vscode.open", output);
          return false;
        }
      },
    });
    currentBridge = bridge;
    context.subscriptions.push(bridge);

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
    currentMessageSubscription = messageSubscription;
    context.subscriptions.push(messageSubscription);

    const panelSubscription = panel.onDidDispose(() => {
      messageSubscription.dispose();
      void bridge.dispose();
      if (currentMessageSubscription === messageSubscription) {
        currentMessageSubscription = undefined;
      }
      if (currentPanelSubscription === panelSubscription) {
        currentPanelSubscription = undefined;
      }
      if (currentBridge === bridge) currentBridge = undefined;
      if (currentPanel === panel) currentPanel = undefined;
    });
    currentPanelSubscription = panelSubscription;
    context.subscriptions.push(panelSubscription);
  });

  context.subscriptions.push(command);
}

export async function deactivate(): Promise<void> {
  const bridge = currentBridge;
  const messageSubscription = currentMessageSubscription;
  const panelSubscription = currentPanelSubscription;
  currentBridge = undefined;
  currentMessageSubscription = undefined;
  currentPanelSubscription = undefined;
  currentPanel = undefined;
  messageSubscription?.dispose();
  panelSubscription?.dispose();
  await bridge?.dispose();
}
