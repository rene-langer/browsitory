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
  let openCommand: (() => void) | undefined;

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
    onDidDispose(listener: () => void) {
      panelDisposeListeners.add(listener);
      return { dispose: () => panelDisposeListeners.delete(listener) };
    },
  };
  const vscode = {
    window: {
      createOutputChannel: vi.fn(() => ({ appendLine, dispose: vi.fn() })),
      createWebviewPanel: vi.fn(() => panel),
      showOpenDialog: vi.fn(async () => undefined),
    },
    commands: {
      registerCommand: vi.fn((_name: string, handler: () => void) => {
        openCommand = handler;
        return { dispose: () => { openCommand = undefined; } };
      }),
    },
    env: { openExternal: vi.fn(async () => true) },
    Uri: {
      joinPath: (base: Uri, ...parts: string[]) => fakeUri([base.fsPath, ...parts].join("/")),
      parse: (value: string) => fakeUri(value),
    },
    ViewColumn: { One: 1 },
    ExtensionMode: { Development: 1, Production: 2, Test: 3 },
  };
  const context = {
    subscriptions: [],
    extensionUri: fakeUri("/workspace/extension"),
    extensionMode: 1,
    extension: { packageJSON: { version: "1.2.3" } },
    globalState: { get: vi.fn(), update: vi.fn(async () => undefined) },
  } as unknown as ExtensionContext;

  return {
    child,
    spawn,
    postMessage,
    appendLine,
    messageListeners,
    panelDisposeListeners,
    vscode,
    context,
    openPanel: () => {
      if (!openCommand) throw new Error("open command was not registered");
      openCommand();
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
    harness.openPanel();
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

  it("closing the panel disposes the same bridge and message listener", async () => {
    activate(harness.context);
    harness.openPanel();
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
