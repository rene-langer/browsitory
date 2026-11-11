import { spawn } from "node:child_process";
import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";
import {
  resolveDevelopmentSidecarPath,
  resolvePackagedSidecarPath,
  SidecarBridge,
} from "./sidecarBridge";
import {
  commandTargetPath,
  contributedShortcutIds,
  findRepositoryRoot,
  type HostRequest,
  repoRelativePath,
  repositoryRoots,
  selectedLines,
  SHORTCUT_COMMAND_PREFIX,
  shortcutRequest,
} from "./hostRequests";
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

/** The open panel, its sidecar and what the host still has to tell its webview. */
interface PanelSession {
  readonly panel: vscode.WebviewPanel;
  readonly bridge: SidecarBridge;
  /** Set once the webview listens for host requests (`hostReady`); until then they wait. */
  ready: boolean;
  readonly queued: HostRequest[];
}

let session: PanelSession | undefined;

/** The context key the shortcut keybindings need (#679): the Browsitory webview has the focus. */
const FOCUS_CONTEXT_KEY = "browsitory.webviewFocused";

function setWebviewFocused(focused: boolean): void {
  void vscode.commands.executeCommand("setContext", FOCUS_CONTEXT_KEY, focused);
}

/** Sends `request` to the webview now, or once it listens. */
function sendHostRequest(target: PanelSession, request: HostRequest): void {
  if (!target.ready) {
    target.queued.push(request);
    return;
  }
  void target.panel.webview.postMessage({ jsonrpc: "2.0", method: "hostRequest", params: request });
}

/** A notification from the webview to the host itself (no `id`), handled here; else `false`. */
function handleHostNotification(target: PanelSession, message: unknown): boolean {
  if (typeof message !== "object" || message === null || "id" in message || !("method" in message)) return false;
  const { method, params } = message as { method: unknown; params?: unknown };
  if (method === "hostReady") {
    target.ready = true;
    for (const request of target.queued.splice(0)) sendHostRequest(target, request);
    return true;
  }
  if (method === "webviewFocus") {
    const focused = (params as { focused?: unknown } | undefined)?.focused;
    if (typeof focused === "boolean") setWebviewFocused(focused);
    return true;
  }
  return false;
}

async function pathExists(target: string): Promise<boolean> {
  try {
    await vscode.workspace.fs.stat(vscode.Uri.file(target));
    return true;
  } catch {
    return false;
  }
}

/** The active text editor's file, when it is one on disk. */
function activeEditorFile(): string | undefined {
  const uri = vscode.window.activeTextEditor?.document.uri;
  return uri?.scheme === "file" ? uri.fsPath : undefined;
}

function workspaceFolderPaths(): string[] {
  return (vscode.workspace.workspaceFolders ?? [])
    .filter((folder) => folder.uri.scheme === "file")
    .map((folder) => folder.uri.fsPath);
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

  const assetRoot = resolveWebviewAssetRoot(context.extensionUri, context.extensionMode);
  const webviewOptions: vscode.WebviewPanelOptions & vscode.WebviewOptions = {
    enableScripts: true,
    retainContextWhenHidden: true,
    localResourceRoots: [assetRoot],
  };

  /** Connects a new or restored panel to a sidecar and loads the app into it. */
  function startSession(panel: vscode.WebviewPanel): PanelSession {
    currentPanel = panel;
    panel.iconPath = vscode.Uri.joinPath(context.extensionUri, "icon.png");

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
      showSaveDialog: async (fileName) => {
        const folder = vscode.workspace.workspaceFolders?.[0]?.uri ?? vscode.Uri.file(os.homedir());
        const picked = await vscode.window.showSaveDialog({
          defaultUri: vscode.Uri.joinPath(folder, fileName),
          saveLabel: "Save",
        });
        return picked?.fsPath;
      },
      writeFile: (file, content) => vscode.workspace.fs.writeFile(vscode.Uri.file(file), content),
      readFile: (uri) => vscode.workspace.fs.readFile(uri),
      openExternal: (url) => vscode.env.openExternal(vscode.Uri.parse(url, true)),
      appendLine: (message) => output.appendLine(message),
      openFile: (file, line) =>
        vscode.commands.executeCommand("vscode.open", vscode.Uri.file(file), {
          preview: false,
          selection: line === null ? undefined : new vscode.Range(line - 1, 0, line - 1, 0),
        }),
      revealInOS: (target) => vscode.commands.executeCommand("revealFileInOS", vscode.Uri.file(target)),
      createTerminal: (cwd, name) => vscode.window.createTerminal({ cwd, name }).show(),
      executeCommand: (command) => vscode.commands.executeCommand(command),
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
    const started: PanelSession = { panel, bridge, ready: false, queued: [] };
    session = started;

    const script = vscode.Uri.joinPath(assetRoot, "assets", "vscode-main.js");
    const style = vscode.Uri.joinPath(assetRoot, "assets", "vscode-main.css");
    panel.webview.html = renderWebviewHtml(panel.webview, { script, style });

    const messageSubscription = panel.webview.onDidReceiveMessage((message: unknown) => {
      if (handleHostNotification(started, message)) return;
      void bridge.handleWebviewMessage(message).catch((error: unknown) => {
        output.appendLine(
          "Browsitory: failed to handle webview request: " +
            (error instanceof Error ? error.message : String(error)),
        );
      });
    });
    currentMessageSubscription = messageSubscription;
    context.subscriptions.push(messageSubscription);

    // A panel behind another editor tab keeps its page "visible"; tell the webview, which
    // pauses background work (Fetch automatically) while hidden.
    const viewStateSubscription = panel.onDidChangeViewState((event) => {
      if (!event.webviewPanel.active) setWebviewFocused(false);
      void panel.webview.postMessage({
        jsonrpc: "2.0",
        method: "panelVisibility",
        params: { visible: event.webviewPanel.visible },
      });
    });
    context.subscriptions.push(viewStateSubscription);

    const panelSubscription = panel.onDidDispose(() => {
      messageSubscription.dispose();
      viewStateSubscription.dispose();
      void bridge.dispose();
      setWebviewFocused(false);
      if (currentMessageSubscription === messageSubscription) {
        currentMessageSubscription = undefined;
      }
      if (currentPanelSubscription === panelSubscription) {
        currentPanelSubscription = undefined;
      }
      if (currentBridge === bridge) currentBridge = undefined;
      if (currentPanel === panel) currentPanel = undefined;
      if (session === started) session = undefined;
    });
    currentPanelSubscription = panelSubscription;
    context.subscriptions.push(panelSubscription);
    return started;
  }

  /** The open panel, shown; else a new one in the active editor group (#678). */
  function showPanel(): PanelSession {
    if (session) {
      session.panel.reveal();
      return session;
    }
    const panel = vscode.window.createWebviewPanel("browsitory", "Browsitory", vscode.ViewColumn.Active, webviewOptions);
    return startSession(panel);
  }

  /**
   * "Browsitory: Open" (#677): with a resource (Explorer, Source Control), opens its repository;
   * else the active editor's file's repository; else, when the panel opens, the workspace
   * folders' repositories. Without any, the panel restores its tabs or shows the picker.
   */
  async function open(argument?: unknown): Promise<void> {
    const explicit = commandTargetPath(argument);
    const opening = session === undefined;
    const editorFile = explicit === undefined ? activeEditorFile() : undefined;
    let roots: string[];
    if (explicit !== undefined) roots = await repositoryRoots([explicit], pathExists);
    else if (editorFile !== undefined) roots = await repositoryRoots([editorFile], pathExists);
    else roots = [];
    if (roots.length === 0 && explicit === undefined && opening) {
      roots = await repositoryRoots(workspaceFolderPaths(), pathExists);
    }
    const target = showPanel();
    if (roots.length > 0) sendHostRequest(target, { kind: "openRepositories", paths: roots });
    else if (explicit !== undefined) void vscode.window.showInformationMessage(`${path.basename(explicit)} isn't in a Git repository.`);
  }

  /** "Show File History" and "Show Line History" in Browsitory (#678). */
  async function showHistory(argument: unknown, lines: boolean): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    const file = commandTargetPath(argument) ?? activeEditorFile();
    if (file === undefined) {
      void vscode.window.showInformationMessage("Open a file to see its history in Browsitory.");
      return;
    }
    const root = await findRepositoryRoot(file, pathExists);
    if (root === undefined) {
      void vscode.window.showInformationMessage(`${path.basename(file)} isn't in a Git repository.`);
      return;
    }
    const request: HostRequest = { kind: "showHistory", repoPath: root, path: repoRelativePath(root, file) };
    if (lines) {
      if (editor === undefined || editor.document.uri.fsPath !== file) {
        void vscode.window.showInformationMessage("Select lines in the editor to see their history in Browsitory.");
        return;
      }
      request.lines = selectedLines(editor.selection);
    }
    sendHostRequest(showPanel(), request);
  }

  context.subscriptions.push(
    vscode.commands.registerCommand("browsitory.open", open),
    // The same, as the Explorer and Source Control menus' "Open in Browsitory".
    vscode.commands.registerCommand("browsitory.openRepository", open),
    vscode.commands.registerCommand("browsitory.showFileHistory", (argument?: unknown) => showHistory(argument, false)),
    vscode.commands.registerCommand("browsitory.showLineHistory", (argument?: unknown) => showHistory(argument, true)),
  );

  // The global shortcuts VSCode binds itself (#679): `package.json` contributes a keybinding for
  // each, active while the Browsitory webview has the focus, whose command hands the shortcut to
  // the webview (which then leaves those keys to these commands).
  for (const id of contributedShortcutIds(context.extension.packageJSON)) {
    context.subscriptions.push(
      vscode.commands.registerCommand(SHORTCUT_COMMAND_PREFIX + id, (args?: unknown) => {
        if (session) sendHostRequest(session, shortcutRequest(id, args));
      }),
    );
  }

  // Brings the panel back after a window reload (#678).
  context.subscriptions.push(
    vscode.window.registerWebviewPanelSerializer("browsitory", {
      deserializeWebviewPanel: async (panel) => {
        if (session) {
          panel.dispose();
          return;
        }
        panel.webview.options = webviewOptions;
        startSession(panel);
      },
    }),
  );
}

export async function deactivate(): Promise<void> {
  const bridge = currentBridge;
  const messageSubscription = currentMessageSubscription;
  const panelSubscription = currentPanelSubscription;
  currentBridge = undefined;
  currentMessageSubscription = undefined;
  currentPanelSubscription = undefined;
  currentPanel = undefined;
  session = undefined;
  messageSubscription?.dispose();
  panelSubscription?.dispose();
  await bridge?.dispose();
}
