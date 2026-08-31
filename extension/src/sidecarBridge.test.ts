import { EventEmitter } from "node:events";
import { describe, expect, it, vi } from "vitest";
import type { ExtensionContext, Uri } from "vscode";
import {
  resolveDevelopmentSidecarPath,
  resolvePackagedSidecarPath,
  SidecarBridge,
  type SidecarProcess,
} from "./sidecarBridge";

class FakeReadable extends EventEmitter {
  push(chunk: Buffer | string) {
    this.emit("data", typeof chunk === "string" ? Buffer.from(chunk) : chunk);
  }
}

class FakeWritable extends EventEmitter {
  readonly write = vi.fn((_chunk: string) => true);
}

class FakeSidecar extends EventEmitter implements SidecarProcess {
  readonly stdout = new FakeReadable();
  readonly stderr = new FakeReadable();
  readonly stdin = new FakeWritable();
  readonly kill = vi.fn(() => true);
}

function fakeContext(version = "1.2.3") {
  const values = new Map<string, unknown>([["lastSeenVersion", "0.9.0"]]);
  const get = vi.fn(<T>(key: string) => values.get(key) as T | undefined);
  const update = vi.fn(async (key: string, value: unknown) => {
    values.set(key, value);
  });
  return {
    context: {
      extension: { packageJSON: { version } },
      globalState: { get, update },
    } as unknown as ExtensionContext,
    get,
    update,
  };
}

function createBridge(overrides: {
  folder?: Uri[] | undefined;
  externalResult?: boolean;
  spawnResults?: Array<FakeSidecar | Error>;
} = {}) {
  const child = new FakeSidecar();
  const spawnResults = overrides.spawnResults ?? [child];
  let spawnIndex = 0;
  const spawn = vi.fn(() => {
    const result = spawnResults[spawnIndex++] ?? spawnResults.at(-1) ?? child;
    if (result instanceof Error) throw result;
    return result;
  });
  const postToWebview = vi.fn();
  const showOpenDialog = vi.fn(async () => overrides.folder);
  const openExternal = vi.fn(async () => overrides.externalResult ?? true);
  const appendLine = vi.fn();
  const state = fakeContext();
  const bridge = new SidecarBridge({
    spawn,
    executablePath: "/workspace/target/debug/vscode-sidecar",
    context: state.context,
    postToWebview,
    showOpenDialog,
    openExternal,
    appendLine,
  });
  return {
    bridge,
    child,
    spawn,
    postToWebview,
    showOpenDialog,
    openExternal,
    appendLine,
    ...state,
  };
}

function writtenRequest(sidecar: FakeSidecar, index: number) {
  const chunk = sidecar.stdin.write.mock.calls[index]?.[0];
  if (typeof chunk !== "string") throw new Error(`missing sidecar request ${index}`);
  return JSON.parse(chunk) as {
    jsonrpc: "2.0";
    id: number;
    method: string;
    params: Record<string, unknown>;
  };
}

async function completePersistedRepositoryRestore(
  sidecar: FakeSidecar,
  repoPaths: readonly string[] = [],
) {
  await vi.waitFor(() => expect(sidecar.stdin.write).toHaveBeenCalledTimes(1));
  const listRequest = writtenRequest(sidecar, 0);
  expect(listRequest).toMatchObject({
    jsonrpc: "2.0",
    method: "list_open_repos",
    params: {},
  });
  sidecar.stdout.push(
    `${JSON.stringify({
      jsonrpc: "2.0",
      id: listRequest.id,
      result: {
        entries: repoPaths.map((path) => ({ path, workspaceId: null })),
        activePath: repoPaths[0] ?? null,
      },
    })}\n`,
  );

  for (const [offset, repoPath] of repoPaths.entries()) {
    const index = offset + 1;
    await vi.waitFor(() => expect(sidecar.stdin.write).toHaveBeenCalledTimes(index + 1));
    const openRequest = writtenRequest(sidecar, index);
    expect(openRequest).toMatchObject({
      jsonrpc: "2.0",
      method: "open_repo",
      params: { path: repoPath },
    });
    sidecar.stdout.push(
      `${JSON.stringify({ jsonrpc: "2.0", id: openRequest.id, result: null })}\n`,
    );
  }
}

describe("SidecarBridge", () => {
  it("lazily spawns one sidecar, writes one request per line, and preserves split UTF-8 replies", async () => {
    const { bridge, child, spawn, postToWebview } = createBridge();
    const first = {
      jsonrpc: "2.0" as const,
      id: 7,
      method: "get_status",
      params: { repoPath: "/repo" },
    };
    const second = {
      jsonrpc: "2.0" as const,
      id: 8,
      method: "list_recent_repos",
      params: {},
    };

    await bridge.handleWebviewMessage(first);
    await bridge.handleWebviewMessage(second);

    expect(spawn).toHaveBeenCalledOnce();
    expect(spawn).toHaveBeenCalledWith("/workspace/target/debug/vscode-sidecar");
    expect(child.stdin.write).toHaveBeenNthCalledWith(1, `${JSON.stringify(first)}\n`);
    expect(child.stdin.write).toHaveBeenNthCalledWith(2, `${JSON.stringify(second)}\n`);

    const response = { jsonrpc: "2.0", id: 7, result: { summary: "café 🚀" } };
    const notification = {
      jsonrpc: "2.0",
      method: "transferProgress",
      params: { message: "reçu" },
    };
    const bytes = Buffer.from(`${JSON.stringify(response)}\n${JSON.stringify(notification)}\n`);
    const splitAt = bytes.indexOf(Buffer.from("é")) + 1;
    child.stdout.push(bytes.subarray(0, splitAt));
    child.stdout.push(bytes.subarray(splitAt));

    expect(postToWebview).toHaveBeenNthCalledWith(1, response);
    expect(postToWebview).toHaveBeenNthCalledWith(2, notification);
  });

  it("logs malformed stdout lines and continues relaying later JSON-RPC objects", async () => {
    const { bridge, child, postToWebview, appendLine } = createBridge();
    await bridge.handleWebviewMessage({
      jsonrpc: "2.0", id: 2, method: "get_status", params: { repoPath: "/repo" },
    });
    postToWebview.mockClear();
    appendLine.mockClear();
    const valid = { jsonrpc: "2.0", id: 3, result: null };

    child.stdout.push(`not-json\n${JSON.stringify(valid)}\n`);

    expect(appendLine).toHaveBeenCalledWith(expect.stringContaining("malformed sidecar stdout"));
    expect(postToWebview).toHaveBeenCalledWith(valid);
  });

  it("routes the five VSCode-native methods without spawning the sidecar", async () => {
    const folder = { fsPath: "/repos/a" } as Uri;
    const {
      bridge,
      spawn,
      postToWebview,
      showOpenDialog,
      openExternal,
      get,
      update,
    } = createBridge({ folder: [folder], externalResult: true });

    await bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 1, method: "pick_repo_folder", params: {} });
    await bridge.handleWebviewMessage({
      jsonrpc: "2.0",
      id: 2,
      method: "open_external_url",
      params: { url: "https://example.com/pull/1" },
    });
    await bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 3, method: "get_app_version", params: {} });
    await bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 4, method: "get_last_seen_version", params: {} });
    await bridge.handleWebviewMessage({
      jsonrpc: "2.0",
      id: 5,
      method: "set_last_seen_version",
      params: { version: "1.2.3" },
    });

    expect(spawn).not.toHaveBeenCalled();
    expect(showOpenDialog).toHaveBeenCalledWith({
      canSelectFiles: false,
      canSelectFolders: true,
      canSelectMany: false,
      openLabel: "Open Repository",
    });
    expect(openExternal).toHaveBeenCalledWith("https://example.com/pull/1");
    expect(get).toHaveBeenCalledWith("lastSeenVersion");
    expect(update).toHaveBeenCalledWith("lastSeenVersion", "1.2.3");
    expect(postToWebview.mock.calls.map(([message]) => message)).toEqual([
      { jsonrpc: "2.0", id: 1, result: "/repos/a" },
      { jsonrpc: "2.0", id: 2, result: true },
      { jsonrpc: "2.0", id: 3, result: "1.2.3" },
      { jsonrpc: "2.0", id: 4, result: "0.9.0" },
      { jsonrpc: "2.0", id: 5, result: null },
    ]);
  });

  it("returns null when the native folder picker is cancelled", async () => {
    const { bridge, spawn, postToWebview } = createBridge({ folder: undefined });

    await bridge.handleWebviewMessage({ jsonrpc: "2.0", id: 11, method: "pick_repo_folder", params: {} });

    expect(spawn).not.toHaveBeenCalled();
    expect(postToWebview).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 11, result: null });
  });

  it("returns an invalid-params error with the incoming id", async () => {
    const { bridge, spawn, postToWebview, openExternal, update } = createBridge();

    await bridge.handleWebviewMessage({
      jsonrpc: "2.0",
      id: 21,
      method: "open_external_url",
      params: { url: 42 },
    });
    await bridge.handleWebviewMessage({
      jsonrpc: "2.0",
      id: 22,
      method: "set_last_seen_version",
      params: {},
    });

    expect(spawn).not.toHaveBeenCalled();
    expect(openExternal).not.toHaveBeenCalled();
    expect(update).not.toHaveBeenCalled();
    expect(postToWebview.mock.calls.map(([message]) => message)).toEqual([
      {
        jsonrpc: "2.0",
        id: 21,
        error: { code: -32602, message: "open_external_url requires a string url" },
      },
      {
        jsonrpc: "2.0",
        id: 22,
        error: { code: -32602, message: "set_last_seen_version requires a string version" },
      },
    ]);
  });
  it("resolves development and packaged binaries at isolated paths", () => {
    expect(resolveDevelopmentSidecarPath("/workspace/extension", "linux")).toBe(
      "/workspace/target/debug/vscode-sidecar",
    );
    expect(resolveDevelopmentSidecarPath("/workspace/extension", "win32")).toBe(
      "/workspace/target/debug/vscode-sidecar.exe",
    );
    expect(resolvePackagedSidecarPath("/workspace/extension", "linux")).toBe(
      "/workspace/extension/bin/vscode-sidecar",
    );
  });

  it("kills the spawned sidecar when disposed", async () => {
    const { bridge, child } = createBridge();
    await bridge.handleWebviewMessage({
      jsonrpc: "2.0",
      id: 31,
      method: "get_status",
      params: { repoPath: "/repo" },
    });

    await bridge.dispose();

    expect(child.kill).toHaveBeenCalledOnce();
  });

  it("restores persisted repositories before forwarding the first request after exit", async () => {
    const first = new FakeSidecar();
    const replacement = new FakeSidecar();
    const { bridge, spawn, postToWebview } = createBridge({
      spawnResults: [first, replacement],
    });

    await bridge.handleWebviewMessage({
      jsonrpc: "2.0", id: 41, method: "get_status", params: { repoPath: "/repo" },
    });
    await bridge.handleWebviewMessage({
      jsonrpc: "2.0", id: 42, method: "list_recent_repos", params: {},
    });

    first.emit("exit", 17, null);

    expect(postToWebview.mock.calls.map(([message]) => message)).toEqual([
      {
        jsonrpc: "2.0",
        id: 41,
        error: { code: -32001, message: "Browsitory sidecar exited with code 17" },
      },
      {
        jsonrpc: "2.0",
        id: 42,
        error: { code: -32001, message: "Browsitory sidecar exited with code 17" },
      },
      {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: {
          state: "reconnecting",
          message: "Browsitory sidecar exited with code 17",
        },
      },
    ]);
    expect(spawn).toHaveBeenCalledOnce();

    const next = {
      jsonrpc: "2.0" as const,
      id: 43,
      method: "get_status",
      params: { repoPath: "/repo" },
    };
    const handling = bridge.handleWebviewMessage(next);
    await completePersistedRepositoryRestore(replacement, ["/repo"]);
    await handling;

    expect(spawn).toHaveBeenCalledTimes(2);
    expect(writtenRequest(replacement, 2)).toEqual(next);
    expect(first.stdin.write).not.toHaveBeenCalledWith(`${JSON.stringify(next)}\n`);
  });

  it("handles a process error like an exit without replaying pending requests", async () => {
    const first = new FakeSidecar();
    const replacement = new FakeSidecar();
    const { bridge, spawn, postToWebview } = createBridge({
      spawnResults: [first, replacement],
    });
    const pending = [
      { jsonrpc: "2.0" as const, id: 51, method: "get_status", params: { repoPath: "/repo" } },
      { jsonrpc: "2.0" as const, id: 52, method: "commit", params: { repoPath: "/repo", message: "once" } },
    ];
    for (const request of pending) await bridge.handleWebviewMessage(request);

    first.emit("error", new Error("crashed"));

    expect(postToWebview.mock.calls.map(([message]) => message)).toEqual([
      {
        jsonrpc: "2.0",
        id: 51,
        error: { code: -32001, message: "Browsitory sidecar process error: crashed" },
      },
      {
        jsonrpc: "2.0",
        id: 52,
        error: { code: -32001, message: "Browsitory sidecar process error: crashed" },
      },
      {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: {
          state: "reconnecting",
          message: "Browsitory sidecar process error: crashed",
        },
      },
    ]);

    const retry = {
      jsonrpc: "2.0" as const,
      id: 53,
      method: "get_status",
      params: { repoPath: "/repo" },
    };
    const handling = bridge.handleWebviewMessage(retry);
    await completePersistedRepositoryRestore(replacement);
    await handling;

    expect(spawn).toHaveBeenCalledTimes(2);
    expect(replacement.stdin.write).toHaveBeenCalledTimes(2);
    expect(replacement.stdin.write).toHaveBeenCalledWith(`${JSON.stringify(retry)}\n`);
    expect(first.stdin.write).toHaveBeenCalledTimes(2);
  });

  it("aborts repository restoration when the replacement sidecar exits", async () => {
    const first = new FakeSidecar();
    const replacement = new FakeSidecar();
    const nextReplacement = new FakeSidecar();
    const { bridge, spawn, postToWebview } = createBridge({
      spawnResults: [first, replacement, nextReplacement],
    });
    await bridge.handleWebviewMessage({
      jsonrpc: "2.0", id: 54, method: "get_status", params: { repoPath: "/repo-a" },
    });
    first.emit("exit", 1, null);
    postToWebview.mockClear();

    const retry = {
      jsonrpc: "2.0" as const,
      id: 55,
      method: "get_status",
      params: { repoPath: "/repo-a" },
    };
    const handling = bridge.handleWebviewMessage(retry);
    await vi.waitFor(() => expect(replacement.stdin.write).toHaveBeenCalledTimes(1));
    const listRequest = writtenRequest(replacement, 0);
    replacement.stdout.push(
      `${JSON.stringify({
        jsonrpc: "2.0",
        id: listRequest.id,
        result: {
          entries: [
            { path: "/repo-a", workspaceId: null },
            { path: "/repo-b", workspaceId: null },
          ],
          activePath: "/repo-a",
        },
      })}\n`,
    );
    await vi.waitFor(() => expect(replacement.stdin.write).toHaveBeenCalledTimes(2));
    replacement.emit("exit", 9, null);
    await handling;

    expect(replacement.stdin.write).toHaveBeenCalledTimes(2);
    expect(postToWebview).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 55,
      error: { code: -32001, message: "Browsitory sidecar exited with code 9" },
    });

    const nextHandling = bridge.handleWebviewMessage({
      jsonrpc: "2.0", id: 56, method: "list_recent_repos", params: {},
    });
    await completePersistedRepositoryRestore(nextReplacement);
    await nextHandling;
    expect(spawn).toHaveBeenCalledTimes(3);
  });

  it("handles a synchronous stdin write failure and never replays the failed mutation", async () => {
    const first = new FakeSidecar();
    const replacement = new FakeSidecar();
    first.stdin.write
      .mockImplementationOnce(() => true)
      .mockImplementationOnce(() => {
        throw new Error("broken pipe");
      });
    const { bridge, spawn, postToWebview } = createBridge({
      spawnResults: [first, replacement],
    });
    const status = {
      jsonrpc: "2.0" as const,
      id: 61,
      method: "get_status",
      params: { repoPath: "/repo" },
    };
    const mutation = {
      jsonrpc: "2.0" as const,
      id: 62,
      method: "commit",
      params: { repoPath: "/repo", message: "once" },
    };

    await bridge.handleWebviewMessage(status);
    await bridge.handleWebviewMessage(mutation);

    expect(postToWebview.mock.calls.map(([message]) => message)).toEqual([
      {
        jsonrpc: "2.0",
        id: 61,
        error: {
          code: -32001,
          message: "Browsitory sidecar stdin write failed: broken pipe",
        },
      },
      {
        jsonrpc: "2.0",
        id: 62,
        error: {
          code: -32001,
          message: "Browsitory sidecar stdin write failed: broken pipe",
        },
      },
      {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: {
          state: "reconnecting",
          message: "Browsitory sidecar stdin write failed: broken pipe",
        },
      },
    ]);

    const retry = {
      jsonrpc: "2.0" as const,
      id: 63,
      method: "get_status",
      params: { repoPath: "/repo" },
    };
    const handling = bridge.handleWebviewMessage(retry);
    await completePersistedRepositoryRestore(replacement);
    await handling;

    expect(spawn).toHaveBeenCalledTimes(2);
    expect(replacement.stdin.write).toHaveBeenCalledWith(`${JSON.stringify(retry)}\n`);
    expect(replacement.stdin.write).not.toHaveBeenCalledWith(`${JSON.stringify(mutation)}\n`);
  });

  it("moves to failed when a reconnect spawn fails and retries only on a later request", async () => {
    const first = new FakeSidecar();
    const replacement = new FakeSidecar();
    const { bridge, spawn, postToWebview } = createBridge({
      spawnResults: [first, new Error("executable missing"), replacement],
    });
    await bridge.handleWebviewMessage({
      jsonrpc: "2.0", id: 71, method: "get_status", params: { repoPath: "/repo" },
    });
    first.emit("exit", 1, null);
    postToWebview.mockClear();

    await bridge.handleWebviewMessage({
      jsonrpc: "2.0", id: 72, method: "get_status", params: { repoPath: "/repo" },
    });

    expect(postToWebview.mock.calls.map(([message]) => message)).toEqual([
      {
        jsonrpc: "2.0",
        id: 72,
        error: {
          code: -32001,
          message: "Browsitory sidecar failed to start: executable missing",
        },
      },
      {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: {
          state: "failed",
          message: "Browsitory sidecar failed to start: executable missing",
        },
      },
    ]);
    expect(spawn).toHaveBeenCalledTimes(2);

    const retry = {
      jsonrpc: "2.0" as const,
      id: 73,
      method: "list_recent_repos",
      params: {},
    };
    const handling = bridge.handleWebviewMessage(retry);
    await completePersistedRepositoryRestore(replacement);
    await handling;

    expect(spawn).toHaveBeenCalledTimes(3);
    expect(replacement.stdin.write).toHaveBeenCalledWith(`${JSON.stringify(retry)}\n`);
  });

  it("rejects pending requests and removes process listeners when disposed", async () => {
    vi.useFakeTimers();
    try {
      const child = new FakeSidecar();
      const { bridge, postToWebview } = createBridge({ spawnResults: [child] });
      await bridge.handleWebviewMessage({
        jsonrpc: "2.0", id: 81, method: "get_status", params: { repoPath: "/repo" },
      });
      await bridge.handleWebviewMessage({
        jsonrpc: "2.0", id: 82, method: "list_recent_repos", params: {},
      });

      await bridge.dispose();

      expect(child.kill).toHaveBeenCalledOnce();
      expect(postToWebview.mock.calls.map(([message]) => message)).toEqual([
        {
          jsonrpc: "2.0",
          id: 81,
          error: { code: -32001, message: "Browsitory sidecar bridge disposed" },
        },
        {
          jsonrpc: "2.0",
          id: 82,
          error: { code: -32001, message: "Browsitory sidecar bridge disposed" },
        },
        {
          jsonrpc: "2.0",
          method: "transportStatus",
          params: { state: "failed", message: "Browsitory sidecar bridge disposed" },
        },
      ]);
      expect(child.listenerCount("error")).toBe(0);
      expect(child.listenerCount("exit")).toBe(0);
      expect(child.stdin.listenerCount("error")).toBe(0);
      expect(child.stdout.listenerCount("data")).toBe(0);
      expect(child.stderr.listenerCount("data")).toBe(0);
      expect(vi.getTimerCount()).toBe(0);
    } finally {
      vi.useRealTimers();
    }
  });

});
