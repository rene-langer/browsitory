import { EventEmitter } from "node:events";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ExtensionContext, Uri } from "vscode";

class FakeWritable extends EventEmitter {
  readonly write = vi.fn((_chunk: string) => true);
}

class FakeChild extends EventEmitter {
  readonly stdin = new FakeWritable();
  readonly stdout = new EventEmitter();
  readonly stderr = new EventEmitter();
  readonly kill = vi.fn(() => true);
}

function fakeUri(fsPath: string): Uri {
  return {
    fsPath,
    toString: () => `vscode-resource:${fsPath}`,
  } as unknown as Uri;
}

function createHarness() {
  const child = new FakeChild();
  const spawn = vi.fn(() => child);
  const postMessage = vi.fn(async (_message: unknown) => true);
  const appendLine = vi.fn();
  const messageListeners = new Set<(message: unknown) => void>();
  const panelDisposeListeners = new Set<() => void>();
  const viewStateListeners = new Set<(event: { webviewPanel: { visible: boolean } }) => void>();
  const commands = new Map<string, (...args: unknown[]) => unknown>();
  let serializer: { deserializeWebviewPanel: (panel: unknown, state: unknown) => Promise<void> } | undefined;
  /** Paths `workspace.fs.stat` finds. */
  const existing = new Set<string>();
  let contentProvider:
    | { provideTextDocumentContent: (uri: { path: string; query: string }) => Promise<string> }
    | undefined;

  const webview = {
    cspSource: "vscode-webview:",
    html: "",
    asWebviewUri: (uri: Uri) => uri,
    postMessage,
    onDidReceiveMessage(listener: (message: unknown) => void) {
      messageListeners.add(listener);
      return { dispose: () => messageListeners.delete(listener) };
    },
  };
  const panel = {
    webview,
    viewColumn: 1,
    reveal: vi.fn(),
    dispose: vi.fn(),
    iconPath: undefined as Uri | undefined,
    visible: true,
    active: true,
    onDidDispose(listener: () => void) {
      panelDisposeListeners.add(listener);
      return { dispose: () => panelDisposeListeners.delete(listener) };
    },
    onDidChangeViewState(listener: (event: { webviewPanel: { visible: boolean } }) => void) {
      viewStateListeners.add(listener);
      return { dispose: () => viewStateListeners.delete(listener) };
    },
  };
  const vscode = {
    window: {
      createOutputChannel: vi.fn(() => ({ appendLine, dispose: vi.fn() })),
      createWebviewPanel: vi.fn(() => panel),
      showOpenDialog: vi.fn(async () => undefined),
      showInformationMessage: vi.fn(async () => undefined),
      activeTextEditor: undefined as
        | undefined
        | {
            document: { uri: { scheme: string; fsPath: string } };
            selection: { start: { line: number }; end: { line: number; character: number } };
          },
      registerWebviewPanelSerializer: vi.fn((_viewType: string, value: typeof serializer) => {
        serializer = value;
        return { dispose: vi.fn() };
      }),
    },
    commands: {
      registerCommand: vi.fn((name: string, handler: (...args: unknown[]) => unknown) => {
        commands.set(name, handler);
        return { dispose: () => commands.delete(name) };
      }),
      executeCommand: vi.fn(async () => undefined),
    },
    workspace: {
      workspaceFolders: undefined as undefined | Array<{ uri: { scheme: string; fsPath: string } }>,
      fs: {
        stat: vi.fn(async (uri: Uri) => {
          if (!existing.has(uri.fsPath)) throw new Error("ENOENT");
          return {};
        }),
      },
      registerTextDocumentContentProvider: vi.fn(
        (_scheme: string, provider: { provideTextDocumentContent: (uri: { path: string; query: string }) => Promise<string> }) => {
          contentProvider = provider;
          return { dispose: vi.fn() };
        },
      ),
    },
    env: { openExternal: vi.fn(async () => true) },
    Uri: {
      joinPath: (base: Uri, ...parts: string[]) => fakeUri([base.fsPath, ...parts].join("/")),
      parse: (value: string) => fakeUri(value),
      file: (value: string) => fakeUri(value),
    },
    ViewColumn: { One: 1, Active: -1 },
    ExtensionMode: { Development: 1, Production: 2, Test: 3 },
  };
  const context = {
    subscriptions: [],
    extensionUri: fakeUri("/workspace/extension"),
    extensionMode: 1,
    extension: {
      packageJSON: {
        version: "1.2.3",
        contributes: {
          commands: [
            { command: "browsitory.open" },
            { command: "browsitory.shortcut.close-tab" },
            { command: "browsitory.shortcut.tab-by-number" },
          ],
        },
      },
    },
    globalState: { get: vi.fn(), update: vi.fn(async () => undefined) },
  } as unknown as ExtensionContext;

  return {
    child,
    spawn,
    postMessage,
    appendLine,
    messageListeners,
    panelDisposeListeners,
    viewStateListeners,
    vscode,
    panel,
    existing,
    commands,
    runCommand: (name: string, ...args: unknown[]) => {
      const handler = commands.get(name);
      if (!handler) throw new Error(`${name} was not registered`);
      return handler(...args);
    },
    restorePanel: () => {
      if (!serializer) throw new Error("no serializer registered");
      return serializer.deserializeWebviewPanel(panel, undefined);
    },
    setPanelVisible: (visible: boolean) => {
      panel.visible = visible;
      for (const listener of [...viewStateListeners]) listener({ webviewPanel: panel });
    },
    context,
    provideContent: (uri: { path: string; query: string }) => {
      if (!contentProvider) throw new Error("content provider was not registered");
      return contentProvider.provideTextDocumentContent(uri);
    },
    openPanel: () => {
      const open = commands.get("browsitory.open");
      if (!open) throw new Error("open command was not registered");
      return open();
    },
    dispatch: async (message: unknown) => {
      for (const listener of [...messageListeners]) listener(message);
      await Promise.resolve();
    },
    disposePanel: async () => {
      for (const listener of [...panelDisposeListeners]) listener();
      await Promise.resolve();
    },
  };
}

describe("extension lifecycle", () => {
  let harness: ReturnType<typeof createHarness>;
  let activate: typeof import("./extension.js").activate;
  let deactivate: typeof import("./extension.js").deactivate;

  beforeEach(async () => {
    vi.resetModules();
    harness = createHarness();
    vi.doMock("node:child_process", () => ({ spawn: harness.spawn }));
    vi.doMock("vscode", () => harness.vscode);
    const extension = await import("./extension.js");
    activate = extension.activate;
    deactivate = extension.deactivate;
  });

  afterEach(async () => {
    await deactivate();
    vi.doUnmock("node:child_process");
    vi.doUnmock("vscode");
    vi.useRealTimers();
  });

  it("deactivate kills the sidecar, rejects pending work, and removes host listeners", async () => {
    vi.useFakeTimers();
    activate(harness.context);
    await harness.openPanel();
    await harness.dispatch({
      jsonrpc: "2.0", id: 91, method: "get_status", params: { repoPath: "/repo" },
    });

    await deactivate();

    expect(harness.child.kill).toHaveBeenCalledOnce();
    expect(harness.postMessage.mock.calls.map(([message]) => message)).toEqual([
      {
        jsonrpc: "2.0",
        id: 91,
        error: { code: -32001, message: "Browsitory sidecar bridge disposed" },
      },
      {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: { state: "failed", message: "Browsitory sidecar bridge disposed" },
      },
    ]);
    expect(harness.messageListeners.size).toBe(0);
    expect(harness.panelDisposeListeners.size).toBe(0);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("serves browsitory-file URIs through the open panel's sidecar", async () => {
    activate(harness.context);
    expect(harness.vscode.workspace.registerTextDocumentContentProvider).toHaveBeenCalledWith(
      "browsitory-file",
      expect.anything(),
    );
    const query = Buffer.from(
      JSON.stringify({ repoPath: "/repo", path: "a.ts", version: { kind: "Head" } }),
      "utf8",
    ).toString("base64url");

    await expect(harness.provideContent({ path: "/a.ts", query })).rejects.toThrow("Browsitory is not open.");

    await harness.openPanel();
    const content = harness.provideContent({ path: "/a.ts", query });
    await vi.waitFor(() => expect(harness.child.stdin.write).toHaveBeenCalledTimes(1));
    const request = JSON.parse(harness.child.stdin.write.mock.calls[0]?.[0] ?? "{}") as { id: number; method: string };
    expect(request.method).toBe("read_file_version");
    harness.child.stdout.emit(
      "data",
      Buffer.from(`${JSON.stringify({ jsonrpc: "2.0", id: request.id, result: { text: "hi" } })}\n`),
    );
    await expect(content).resolves.toBe("hi");
    await expect(harness.provideContent({ path: "/a.ts", query: "bogus" })).rejects.toThrow();
  });

  it("retains the webview context while the panel is hidden", async () => {
    activate(harness.context);
    await harness.openPanel();

    expect(harness.vscode.window.createWebviewPanel).toHaveBeenCalledWith(
      "browsitory",
      "Browsitory",
      -1,
      expect.objectContaining({
        enableScripts: true,
        retainContextWhenHidden: true,
      }),
    );
  });

  it("tells the webview when its panel is hidden or shown, and stops when the panel closes", async () => {
    activate(harness.context);
    await harness.openPanel();
    harness.postMessage.mockClear();

    harness.setPanelVisible(false);
    harness.setPanelVisible(true);

    expect(harness.postMessage.mock.calls.map(([message]) => message)).toEqual([
      { jsonrpc: "2.0", method: "panelVisibility", params: { visible: false } },
      { jsonrpc: "2.0", method: "panelVisibility", params: { visible: true } },
    ]);
    await harness.disposePanel();
    expect(harness.viewStateListeners.size).toBe(0);
  });

  it("delivers delayed sidecar replies through Webview.postMessage", async () => {
    activate(harness.context);
    await harness.openPanel();
    await harness.dispatch({
      jsonrpc: "2.0", id: 93, method: "get_status", params: { repoPath: "/repo" },
    });
    harness.postMessage.mockClear();

    const response = { jsonrpc: "2.0", id: 93, result: [] };
    harness.child.stdout.emit("data", Buffer.from(`${JSON.stringify(response)}\n`));

    await vi.waitFor(() => expect(harness.postMessage).toHaveBeenCalledWith(response));
  });

  it("closing the panel disposes the same bridge and message listener", async () => {
    activate(harness.context);
    await harness.openPanel();
    await harness.dispatch({
      jsonrpc: "2.0", id: 92, method: "list_recent_repos", params: {},
    });

    await harness.disposePanel();

    expect(harness.child.kill).toHaveBeenCalledOnce();
    expect(harness.postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 92,
      error: { code: -32001, message: "Browsitory sidecar bridge disposed" },
    });
    expect(harness.messageListeners.size).toBe(0);
  });
});

describe("extension commands and integration points (#677, #678, #679)", () => {
  let harness: ReturnType<typeof createHarness>;
  let activate: typeof import("./extension.js").activate;
  let deactivate: typeof import("./extension.js").deactivate;

  beforeEach(async () => {
    vi.resetModules();
    harness = createHarness();
    vi.doMock("node:child_process", () => ({ spawn: harness.spawn }));
    vi.doMock("vscode", () => harness.vscode);
    const extension = await import("./extension.js");
    activate = extension.activate;
    deactivate = extension.deactivate;
  });

  afterEach(async () => {
    await deactivate();
    vi.doUnmock("node:child_process");
    vi.doUnmock("vscode");
  });

  const hostRequests = () =>
    harness.postMessage.mock.calls
      .map(([message]) => message as { method?: string; params?: unknown })
      .filter((message) => message.method === "hostRequest")
      .map((message) => message.params);

  it("opens the workspace folders' repositories once the webview listens", async () => {
    harness.vscode.workspace.workspaceFolders = [
      { uri: { scheme: "file", fsPath: "/work/app" } },
      { uri: { scheme: "file", fsPath: "/work/notes" } },
      { uri: { scheme: "file", fsPath: "/work/app/sub" } },
    ];
    harness.existing.add("/work/app/.git");
    activate(harness.context);

    await harness.openPanel();
    expect(hostRequests()).toEqual([]);
    await harness.dispatch({ jsonrpc: "2.0", method: "hostReady" });

    // /work/notes isn't a repository; /work/app/sub is inside /work/app.
    expect(hostRequests()).toEqual([{ kind: "openRepositories", paths: ["/work/app"] }]);
  });

  it("prefers the active editor's repository, and opens a folder passed by a menu", async () => {
    harness.existing.add("/work/lib/.git");
    harness.existing.add("/other/.git");
    harness.vscode.workspace.workspaceFolders = [{ uri: { scheme: "file", fsPath: "/other" } }];
    harness.vscode.window.activeTextEditor = {
      document: { uri: { scheme: "file", fsPath: "/work/lib/src/a.ts" } },
      selection: { start: { line: 0 }, end: { line: 0, character: 0 } },
    };
    activate(harness.context);
    await harness.openPanel();
    await harness.dispatch({ jsonrpc: "2.0", method: "hostReady" });
    await harness.runCommand("browsitory.openRepository", { scheme: "file", fsPath: "/other/docs" });
    // The Source Control title bar passes its provider.
    await harness.runCommand("browsitory.openRepository", { rootUri: { scheme: "file", fsPath: "/work/lib" } });

    expect(hostRequests()).toEqual([
      { kind: "openRepositories", paths: ["/work/lib"] },
      { kind: "openRepositories", paths: ["/other"] },
      { kind: "openRepositories", paths: ["/work/lib"] },
    ]);
    expect(harness.vscode.window.createWebviewPanel).toHaveBeenCalledOnce();
    expect(harness.panel.reveal).toHaveBeenCalledTimes(2);
    expect(harness.panel.iconPath?.fsPath).toBe("/workspace/extension/icon.png");
  });

  it("says so when a folder isn't in a repository", async () => {
    activate(harness.context);
    await harness.runCommand("browsitory.openRepository", { scheme: "file", fsPath: "/tmp/plain" });
    expect(harness.vscode.window.showInformationMessage).toHaveBeenCalledWith("plain isn't in a Git repository.");
  });

  it("shows a file's history and the selected lines' history", async () => {
    harness.existing.add("/work/lib/.git");
    harness.vscode.window.activeTextEditor = {
      document: { uri: { scheme: "file", fsPath: "/work/lib/src/a.ts" } },
      // Lines 3 to 5; the selection ends at the start of line 6.
      selection: { start: { line: 2 }, end: { line: 5, character: 0 } },
    };
    activate(harness.context);
    await harness.runCommand("browsitory.showFileHistory", { scheme: "file", fsPath: "/work/lib/README.md" });
    await harness.runCommand("browsitory.showLineHistory", { scheme: "file", fsPath: "/work/lib/src/a.ts" });
    await harness.dispatch({ jsonrpc: "2.0", method: "hostReady" });

    expect(hostRequests()).toEqual([
      { kind: "showHistory", repoPath: "/work/lib", path: "README.md" },
      { kind: "showHistory", repoPath: "/work/lib", path: "src/a.ts", lines: { startLine: 3, endLine: 5 } },
    ]);
  });

  it("hands contributed shortcut keys to the webview and tracks its focus for their when clause", async () => {
    activate(harness.context);
    expect([...harness.commands.keys()].filter((name) => name.startsWith("browsitory.shortcut."))).toEqual([
      "browsitory.shortcut.close-tab",
      "browsitory.shortcut.tab-by-number",
    ]);
    await harness.openPanel();
    await harness.dispatch({ jsonrpc: "2.0", method: "hostReady" });
    await harness.dispatch({ jsonrpc: "2.0", method: "webviewFocus", params: { focused: true } });
    await harness.runCommand("browsitory.shortcut.close-tab");
    await harness.runCommand("browsitory.shortcut.tab-by-number", { index: 3 });

    expect(hostRequests()).toEqual([
      { kind: "shortcut", id: "close-tab" },
      { kind: "shortcut", id: "tab-by-number", index: 3 },
    ]);
    expect(harness.vscode.commands.executeCommand).toHaveBeenCalledWith("setContext", "browsitory.webviewFocused", true);
    await harness.disposePanel();
    expect(harness.vscode.commands.executeCommand).toHaveBeenLastCalledWith("setContext", "browsitory.webviewFocused", false);
    // Host notifications never reach the sidecar.
    expect(harness.spawn).not.toHaveBeenCalled();
  });

  it("restores the panel after a reload, once", async () => {
    activate(harness.context);
    await harness.restorePanel();
    expect(harness.panel.webview.html).toContain("vscode-main.js");
    await harness.restorePanel();
    expect(harness.panel.dispose).toHaveBeenCalledOnce();
  });
});

describe("selectedLines", () => {
  it("counts from 1 and leaves out a last line the selection only touches", async () => {
    const { selectedLines } = await import("./hostRequests.js");
    expect(selectedLines({ start: { line: 4 }, end: { line: 4, character: 7 } })).toEqual({ startLine: 5, endLine: 5 });
    expect(selectedLines({ start: { line: 0 }, end: { line: 2, character: 0 } })).toEqual({ startLine: 1, endLine: 2 });
  });
});
